use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    u32,
};

use crate::{
    buffer::buffer_manager::{BufferManager, DIR_END_MARKER},
    error::{BasaltError, Result},
    storage_engine::{
        directory_page::{DIR_MAX_ENTRIES, DirectoryPageGuard},
        page::TablePageGuard,
        rid::RID,
        tuple::Tuple,
    },
};

pub const MAX_TUPLE_SIZE: usize = 2000; // ~2000 bytes for header and other records from PAGE_SIZE bytes 
const FLAG_NORMAL: u8 = 0;
const FLAG_OVERFLOW: u8 = 1;

pub struct TableHeap {
    bm: Arc<BufferManager>,
    // first page in the table heap is always a directory page.
    first_directory_page_id: u32,
    next_available_page_id: AtomicU32,
}

impl TableHeap {
    pub fn new(
        bm: Arc<BufferManager>,
        first_directory_page_id: u32,
        next_available_page_id: AtomicU32,
    ) -> Self {
        Self {
            bm,
            first_directory_page_id,
            next_available_page_id,
        }
    }

    pub fn insert_tuple(&self, tuple: &mut Tuple) -> Result<RID> {
        let rid = self.insert_internal(&tuple.data)?;
        tuple.rid = Some(rid);
        Ok(rid)
    }

    pub fn get_tuple(&self, rid: RID) -> Result<Tuple> {
        let mut page_id = rid.page_id;
        let mut slot_id = rid.slot_id;

        let mut full_data = Vec::new();

        loop {
            let data_frame_arc = self.bm.fetch_page(page_id)?;
            let physical_data = {
                let mut data_frame = data_frame_arc.write().unwrap();
                let data_page = TablePageGuard::new(&mut data_frame.data);
                data_page.get_record(slot_id)?.to_vec()
            };
            self.bm.unpin_page(page_id, false)?;

            let flag = physical_data[0];

            if flag == FLAG_NORMAL {
                full_data.extend_from_slice(&physical_data[1..]);
                break;
            } else if flag == FLAG_OVERFLOW {
                // the record is not complete , need to read further
                let next_page_id_offset = 1..5;
                let next_slot_id_offset = 5..9;
                let head_data_offset = 9..;

                let next_page_id_bytes_slice = &physical_data[next_page_id_offset];
                let next_slot_id_bytes_slice = &physical_data[next_slot_id_offset];

                let next_page_id = u32::from_le_bytes(next_page_id_bytes_slice.try_into().unwrap());
                let next_slot_id = u32::from_le_bytes(next_slot_id_bytes_slice.try_into().unwrap());

                // append the head data
                full_data.extend_from_slice(&physical_data[head_data_offset]);
                page_id = next_page_id;
                slot_id = next_slot_id;
            } else {
                return Err(BasaltError::CorruptedPage);
            }
        }
        Ok(Tuple::with_rid(rid, full_data))
    }

    // internal recursive function to handle overflowing records.
    fn insert_internal(&self, data: &[u8]) -> Result<RID> {
        let record_len = data.len();
        // base case -> the data fits
        if record_len <= MAX_TUPLE_SIZE {
            let mut physical_data = Vec::with_capacity(record_len);
            physical_data.push(FLAG_NORMAL);
            physical_data.extend_from_slice(data);

            return self.write_to_slotted_page(&physical_data);
        }

        // overflow case => split the data
        let head = &data[..MAX_TUPLE_SIZE];
        let tail = &data[MAX_TUPLE_SIZE..];

        // insert the tail first (bottom-up chaining)
        let tail_rid = self.insert_internal(tail)?;

        // build the head data
        // [1B Overflow flag] + [8B Next RID] + [Head Record Data]
        let mut physical_data = Vec::with_capacity(head.len());
        physical_data.push(FLAG_OVERFLOW);
        physical_data.extend_from_slice(&tail_rid.page_id.to_le_bytes());
        physical_data.extend_from_slice(&tail_rid.slot_id.to_le_bytes());
        physical_data.extend_from_slice(head);

        self.write_to_slotted_page(&physical_data)
    }

    pub fn write_to_slotted_page(&self, physical_data: &[u8]) -> Result<RID> {
        let required_space = physical_data.len() as u32;

        let mut target_page_id: u32 = u32::MAX;
        let mut target_directory_page_id: u32 = u32::MAX;

        let mut is_new_data_page = false;

        let mut curr_directory_page_id = self.first_directory_page_id;

        // searches for the required space in the directory page or allocates a brand new page
        loop {
            let dir_page_arc = self.bm.fetch_page(curr_directory_page_id)?;
            let next_directory_page_id: u32;
            let mut needs_new_directory_page = false;
            {
                let mut dir_page = dir_page_arc.write().unwrap();
                let dir_guard = DirectoryPageGuard::new(&mut dir_page.data);

                // search in the directory if there is a page that can fit the record
                if let Some(pid) = dir_guard.get_required_freespace_page_id(required_space) {
                    target_page_id = pid;
                    target_directory_page_id = curr_directory_page_id;
                    next_directory_page_id = DIR_END_MARKER; // signal to break
                } else {
                    next_directory_page_id = dir_guard.get_next_directory_page_id();

                    // if there is no next directory , we are at the end of the dir linked list
                    if next_directory_page_id == DIR_END_MARKER {
                        if dir_guard.get_no_of_entries() < DIR_MAX_ENTRIES {
                            target_page_id =
                                self.next_available_page_id.fetch_add(1, Ordering::SeqCst);
                            target_directory_page_id = curr_directory_page_id;
                            is_new_data_page = true;
                        } else {
                            needs_new_directory_page = true;
                        }
                    }
                }
            } // Scoped to drop the RwLock before and buffer manager calls

            // break if we found a page for the data or minted a fresh page
            if target_page_id != u32::MAX || is_new_data_page {
                self.bm.unpin_page(curr_directory_page_id, false)?;
                break;
            }

            if needs_new_directory_page {
                // create a new directory page
                let new_dir_pid = self.next_available_page_id.fetch_add(1, Ordering::SeqCst);
                let new_dir_arc = self.bm.new_page(new_dir_pid)?;
                {
                    let mut new_dir_frame = new_dir_arc.write().unwrap();
                    let mut new_dir_guard = DirectoryPageGuard::new(&mut new_dir_frame.data);
                    new_dir_guard.init(DIR_END_MARKER);
                }
                self.bm.unpin_page(new_dir_pid, true)?;

                // link the directory pages
                {
                    let mut old_dir_page = dir_page_arc.write().unwrap();
                    let mut old_dir_page_guard = DirectoryPageGuard::new(&mut old_dir_page.data);
                    old_dir_page_guard.set_next_directory_page_id(new_dir_pid);
                }
                self.bm.unpin_page(curr_directory_page_id, true)?;

                // move the cursor to the newly minted directory page
                curr_directory_page_id = new_dir_pid;
                continue;
            }

            self.bm.unpin_page(curr_directory_page_id, false)?;

            curr_directory_page_id = next_directory_page_id;
        }

        // fetch or create the chosen page.
        let data_frame_arc = if is_new_data_page {
            let new_page_arc = self.bm.new_page(target_page_id)?;
            {
                let mut new_page = new_page_arc.write().unwrap();
                let mut new_page_guard = TablePageGuard::new(&mut new_page.data);
                new_page_guard.init(target_page_id, u32::MAX, u32::MAX);
            }
            new_page_arc
        } else {
            self.bm.fetch_page(target_page_id)?
        };

        // write the tuple data
        let slot_id;
        let remaining_space;

        {
            let mut data_frame = data_frame_arc.write().unwrap();
            let mut page = TablePageGuard::new(&mut data_frame.data);

            slot_id = page.insert(physical_data).unwrap();
            remaining_space = page.get_freespace() as u32;
        }

        self.bm.unpin_page(target_page_id, true)?;

        // update the directory page that tracks this record
        let dir_page_arc = self.bm.fetch_page(target_directory_page_id)?;
        {
            let mut dir_page = dir_page_arc.write().unwrap();
            let mut dir_page_guard = DirectoryPageGuard::new(&mut dir_page.data);
            dir_page_guard.update_page_freespace(target_page_id, remaining_space)?;
        }
        self.bm.unpin_page(target_directory_page_id, true)?;

        Ok(RID::new(target_page_id, slot_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::replacer::ClockReplacer;
    use crate::storage_engine::disk_manager::DiskManager;
    use std::fs::remove_file;

    #[test]
    fn test_tuple_overflow_chaining() {
        let db_file = "test_overflow.db";
        let _ = remove_file(db_file); // Clean up old test runs

        let disk_manager = Arc::new(DiskManager::new(db_file).unwrap());
        // Pre-allocate pages to avoid PageOutOfBounds when evicting from buffer pool
        for _ in 0..10 {
            disk_manager.allocate_page();
        }

        let replacer = Arc::new(ClockReplacer::new(10));
        let buffer_manager = Arc::new(BufferManager::new(10, replacer, disk_manager));

        // Initialize the first directory page
        {
            let frame_arc = buffer_manager.new_page(0).unwrap();
            let mut frame = frame_arc.write().unwrap();
            let mut dir = DirectoryPageGuard::new(&mut frame.data);
            dir.init(DIR_END_MARKER);
        }
        buffer_manager.unpin_page(0, true).unwrap();

        let table_heap = TableHeap::new(buffer_manager.clone(), 0, AtomicU32::new(1));

        // 1. Create a massive 5000-byte tuple (forces 3 chunks)
        let mut large_data = vec![0u8; 5000];
        for i in 0..5000 {
            large_data[i] = (i % 255) as u8; // Give it a recognizable pattern
        }

        let mut tuple = Tuple::new(large_data.clone());

        // 2. Insert it
        let rid = table_heap
            .insert_tuple(&mut tuple)
            .expect("Failed to insert large tuple");

        // 3. Read it back
        let fetched_tuple = table_heap
            .get_tuple(rid)
            .expect("Failed to fetch large tuple");

        // 4. Verify perfectly reassembled data
        assert_eq!(fetched_tuple.data.len(), 5000);
        assert_eq!(fetched_tuple.data, large_data);

        let _ = remove_file(db_file);
    }

    #[test]
    fn test_table_heap_basic_insert_get() {
        let db_file = "test_basic.db";
        let _ = remove_file(db_file);

        let disk_manager = Arc::new(DiskManager::new(db_file).unwrap());
        for _ in 0..10 {
            disk_manager.allocate_page();
        }

        let replacer = Arc::new(ClockReplacer::new(10));
        let buffer_manager = Arc::new(BufferManager::new(10, replacer, disk_manager));

        // Initialize the first directory page
        {
            let frame_arc = buffer_manager.new_page(0).unwrap();
            let mut frame = frame_arc.write().unwrap();
            let mut dir = DirectoryPageGuard::new(&mut frame.data);
            dir.init(DIR_END_MARKER);
        }
        buffer_manager.unpin_page(0, true).unwrap();

        let table_heap = TableHeap::new(buffer_manager.clone(), 0, AtomicU32::new(1));

        let data1 = b"tuple1".to_vec();
        let data2 = b"tuple2".to_vec();
        let data3 = b"tuple3".to_vec();

        let mut t1 = Tuple::new(data1.clone());
        let mut t2 = Tuple::new(data2.clone());
        let mut t3 = Tuple::new(data3.clone());

        let rid1 = table_heap.insert_tuple(&mut t1).unwrap();
        let rid2 = table_heap.insert_tuple(&mut t2).unwrap();
        let rid3 = table_heap.insert_tuple(&mut t3).unwrap();

        assert_eq!(table_heap.get_tuple(rid1).unwrap().data, data1);
        assert_eq!(table_heap.get_tuple(rid2).unwrap().data, data2);
        assert_eq!(table_heap.get_tuple(rid3).unwrap().data, data3);

        let _ = remove_file(db_file);
    }

    #[test]
    fn test_table_heap_fill_page() {
        let db_file = "test_fill_page.db";
        let _ = remove_file(db_file);

        let disk_manager = Arc::new(DiskManager::new(db_file).unwrap());
        for _ in 0..50 {
            disk_manager.allocate_page();
        }

        let replacer = Arc::new(ClockReplacer::new(50));
        let buffer_manager = Arc::new(BufferManager::new(50, replacer, disk_manager));

        {
            let frame_arc = buffer_manager.new_page(0).unwrap();
            let mut frame = frame_arc.write().unwrap();
            let mut dir = DirectoryPageGuard::new(&mut frame.data);
            dir.init(DIR_END_MARKER);
        }
        buffer_manager.unpin_page(0, true).unwrap();

        let table_heap = TableHeap::new(buffer_manager.clone(), 0, AtomicU32::new(1));

        let mut rids = Vec::new();
        let tuple_size = 500;
        let num_tuples = 30; // 500 * 30 = 15,000 bytes. Should definitely use multiple pages.

        for i in 0..num_tuples {
            let data = vec![i as u8; tuple_size];
            let mut tuple = Tuple::new(data);
            let rid = table_heap.insert_tuple(&mut tuple).unwrap();
            rids.push((rid, i as u8));
        }

        for (rid, val) in rids {
            let fetched = table_heap.get_tuple(rid).unwrap();
            assert_eq!(fetched.data.len(), tuple_size);
            assert!(fetched.data.iter().all(|&b| b == val));
        }

        let _ = remove_file(db_file);
    }

    #[test]
    fn test_table_heap_directory_expansion() {
        let db_file = "test_dir_expansion.db";
        let _ = remove_file(db_file);

        let disk_manager = Arc::new(DiskManager::new(db_file).unwrap());
        // Pre-allocate enough pages for both directory and data pages
        for _ in 0..1500 {
            disk_manager.allocate_page();
        }

        let replacer = Arc::new(ClockReplacer::new(100));
        let buffer_manager = Arc::new(BufferManager::new(100, replacer, disk_manager));

        {
            let frame_arc = buffer_manager.new_page(0).unwrap();
            let mut frame = frame_arc.write().unwrap();
            let mut dir = DirectoryPageGuard::new(&mut frame.data);
            dir.init(DIR_END_MARKER);
        }
        buffer_manager.unpin_page(0, true).unwrap();

        let table_heap = TableHeap::new(buffer_manager.clone(), 0, AtomicU32::new(1));

        // DIR_MAX_ENTRIES is 511.
        // We need > 511 data pages to trigger expansion.
        // Use a tuple size that makes it difficult to fit more than one large chunk per page.
        let tuple_size = 3000;
        let num_tuples = 600;

        let mut rids = Vec::new();
        for i in 0..num_tuples {
            let data = vec![(i % 256) as u8; tuple_size];
            let mut tuple = Tuple::new(data);
            let rid = table_heap.insert_tuple(&mut tuple).expect("Failed to insert tuple during expansion test");
            rids.push(rid);
        }

        // Verify some samples, especially those that would likely be in the second directory page
        let sample_indices = [0, 100, 510, 511, 512, 599];
        for &idx in &sample_indices {
            let fetched = table_heap.get_tuple(rids[idx]).unwrap();
            assert_eq!(fetched.data.len(), tuple_size);
            assert!(fetched.data.iter().all(|&b| b == (idx % 256) as u8));
        }

        let _ = remove_file(db_file);
    }
}
