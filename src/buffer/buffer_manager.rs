use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    buffer::replacer::Replacer,
    error::{BasaltError, Result},
    storage_engine::{disk_manager::DiskManager, page::TablePageGuard},
};

struct Frame {
    data: [u8; 4096],
    page_id: Option<u32>,
    is_dirty: bool,
    pin_count: usize,
}

impl Frame {
    pub fn new() -> Self {
        Self {
            data: [0u8; 4096], 
            page_id: None,
            is_dirty: false,
            pin_count: 0,
        }
    }
}

pub struct BufferManager {
    // the physical memory frame where the page will be held
    // Arc is used so that any background thread that is accessing the frame is gauranteed that
    // the frame will stay alive until the ref count drops to 0.
    // RwLock is for solving data race corruptions.
    frames: Vec<Arc<RwLock<Frame>>>,
    // page table allows to check if the cache even contains the requested page in constant time.
    // if it is not found in this table , it is a cache miss and must be loaded from the hard drive
    // Logical Page ID -> Physical frame index
    page_table: RwLock<HashMap<u32, usize>>,
    // list of frames that are currently empty.
    free_list: Mutex<VecDeque<usize>>,
    replacer: Arc<dyn Replacer>,
    disk_manager: Arc<DiskManager>,
}

impl BufferManager {
    pub fn new(
        pool_size: usize,
        replacer: Arc<dyn Replacer>,
        disk_manager: Arc<DiskManager>,
    ) -> Self {
        let mut frames: Vec<Arc<RwLock<Frame>>> = Vec::new();
        let mut free_list: VecDeque<usize> = VecDeque::new();

        // initialize page in each frame and add the frame to free list.
        for i in 0..pool_size {
            frames.push(Arc::new(RwLock::new(Frame::new())));
            free_list.push_back(i);
        }

        BufferManager {
            frames,
            page_table: RwLock::new(HashMap::with_capacity(pool_size)),
            free_list: Mutex::new(free_list),
            replacer,
            disk_manager,
        }
    }

    pub fn fetch_page(&self, page_id: u32) -> Result<Arc<RwLock<Frame>>> {
        // Step 1: Check in the cache
        // we must never hold the read lock for the entirety of the function as holding the lock
        // while performing disk operations will halt the whole database;
        {
            let table = self.page_table.read().unwrap();
            if let Some(&frame_id) = table.get(&page_id) {
                let frame_arc = Arc::clone(&self.frames[frame_id]);
                let mut frame = frame_arc.write().unwrap();

                frame.pin_count += 1;
                self.replacer.pin(frame_id);
                return Ok(frame_arc.clone());
            }
        } // Read lock will be automatically dropped here.

        // Step 2:  Cache miss. Find a victim frame to load the page
        
        let victim_frame_id = self.find_victim_frame_id()?;
        let victim_frame_arc = Arc::clone(&self.frames[victim_frame_id]);

        // acquire a write lock on the page table first to maintain lock order: page_table -> frame
        let mut table = self.page_table.write().unwrap();
        // acquire a write lock on the frame.
        let mut frame = victim_frame_arc.write().unwrap();

        // if the frame we stole is dirty. save it's contents to the disk first.
        if frame.is_dirty {
            if let Some(old_page_id) = frame.page_id {
                self.disk_manager.write_page(old_page_id, &TablePageGuard::new(&mut frame.data))?;
            }
            frame.is_dirty = false;
        }

        // Step 3: update the page table.
        if let Some(old_page_id) = frame.page_id {
            table.remove(&old_page_id);
        }
        table.insert(page_id, victim_frame_id);
        
        // drop table lock before disk operation to improve concurrency? 
        // No, keep it until frame is loaded if we want absolute atomicity of the table entry pointing to valid data.
        drop(table);

        // Step4: read the data from the disk into the frame.
        self.disk_manager.read_page(page_id, &mut TablePageGuard::new(&mut frame.data))?;

        // Step 5: Reset the metadata for the frame;
        frame.page_id = Some(page_id);
        frame.pin_count = 1;
        self.replacer.pin(victim_frame_id);

        Ok(victim_frame_arc.clone())
    }

    pub fn unpin_page(&self, page_id: u32, is_dirty: bool) -> Result<()> {
        let frame_id = *self.page_table.read().unwrap().get(&page_id).unwrap();

        let frame_arc = Arc::clone(&self.frames[frame_id]);
        let mut frame = frame_arc.write().unwrap();

        if frame.pin_count == 0 {
            // a thread is trying to unpin a frame which has never been pinned. This is a serious
            // logic execution flow.
            panic!("Attempted to unpin a page with pin_count 0!");
        }

        // if the thread marked the page as dirty , make sure the frame remembers it too.
        frame.is_dirty |= is_dirty;

        frame.pin_count -= 1;

        // if no thread is using the frame , tell the clock it can be evicted.
        if frame.pin_count == 0 {
            self.replacer.unpin(frame_id);
        }

        Ok(())
    }

    fn find_victim_frame_id(&self) -> Result<usize> {
        // first find in the free frames list.
        if let Some(frame_id) = self.free_list.lock().unwrap().pop_front() {
            return Ok(frame_id);
        }

        // evict a frame using the replacer
        if let Some(frame_id) = self.replacer.victim() {
            return Ok(frame_id);
        }

        Err(BasaltError::OutOfMemory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::replacer::ClockReplacer;
    use std::fs;

    #[test]
    fn test_buffer_manager_fetch_page() {
        let file_path = "test_bm_fetch.db";
        let disk_manager = Arc::new(DiskManager::new(file_path).unwrap());
        let replacer = Arc::new(ClockReplacer::new(10));
        let bm = BufferManager::new(10, replacer, disk_manager.clone());

        let page_id = disk_manager.allocate_page();
        
        // Write an empty page to disk first so fetch_page doesn't fail on read
        {
            let mut data = [0u8; 4096];
            disk_manager.write_page(page_id, &TablePageGuard::new(&mut data)).unwrap();
        }
        
        // Fetch page
        let frame_arc = bm.fetch_page(page_id).unwrap();
        {
            let frame = frame_arc.read().unwrap();
            assert_eq!(frame.page_id, Some(page_id));
            assert_eq!(frame.pin_count, 1);
        }

        bm.unpin_page(page_id, false).unwrap();
        {
            let frame = frame_arc.read().unwrap();
            assert_eq!(frame.pin_count, 0);
        }

        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_buffer_manager_eviction() {
        let file_path = "test_bm_evict.db";
        let disk_manager = Arc::new(DiskManager::new(file_path).unwrap());
        let pool_size = 2;
        let replacer = Arc::new(ClockReplacer::new(pool_size));
        let bm = BufferManager::new(pool_size, replacer, disk_manager.clone());

        let p0 = disk_manager.allocate_page();
        let p1 = disk_manager.allocate_page();
        let p2 = disk_manager.allocate_page();

        // Write pages to disk
        let mut data = [0u8; 4096];
        disk_manager.write_page(p0, &TablePageGuard::new(&mut data)).unwrap();
        disk_manager.write_page(p1, &TablePageGuard::new(&mut data)).unwrap();
        disk_manager.write_page(p2, &TablePageGuard::new(&mut data)).unwrap();

        // Fetch p0, p1
        let _f0 = bm.fetch_page(p0).unwrap();
        let _f1 = bm.fetch_page(p1).unwrap();

        bm.unpin_page(p0, false).unwrap();
        bm.unpin_page(p1, false).unwrap();

        // Fetch p2, should evict p0 or p1 (Clock hand starts at 0, so p0)
        let _f2 = bm.fetch_page(p2).unwrap();
        
        {
            let table = bm.page_table.read().unwrap();
            assert!(table.contains_key(&p2));
            assert!(!table.contains_key(&p0) || !table.contains_key(&p1));
        }

        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_buffer_manager_dirty_page() {
        let file_path = "test_bm_dirty.db";
        let disk_manager = Arc::new(DiskManager::new(file_path).unwrap());
        let pool_size = 1;
        let replacer = Arc::new(ClockReplacer::new(pool_size));
        let bm = BufferManager::new(pool_size, replacer, disk_manager.clone());

        let p0 = disk_manager.allocate_page();
        let p1 = disk_manager.allocate_page();

        // Write pages to disk
        let mut data = [0u8; 4096];
        disk_manager.write_page(p0, &TablePageGuard::new(&mut data)).unwrap();
        disk_manager.write_page(p1, &TablePageGuard::new(&mut data)).unwrap();

        // Fetch p0 and mark dirty
        {
            let f0_arc = bm.fetch_page(p0).unwrap();
            {
                let mut f0 = f0_arc.write().unwrap();
                f0.data[0] = 42;
            }
            bm.unpin_page(p0, true).unwrap();
        }

        // Fetch p1, should evict p0 and write it to disk
        let _f1 = bm.fetch_page(p1).unwrap();
        bm.unpin_page(p1, false).unwrap();

        // Re-fetch p0 and check data
        {
            let f0_arc = bm.fetch_page(p0).unwrap();
            let f0 = f0_arc.read().unwrap();
            assert_eq!(f0.data[0], 42);
        }

        fs::remove_file(file_path).unwrap();
    }
}
