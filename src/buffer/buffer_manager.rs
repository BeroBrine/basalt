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
        // acquire a write lock on the page table
        {
            let mut table = self.page_table.write().unwrap();
            if let Some(old_page_id) = frame.page_id {
                table.remove(&old_page_id);
            }
            table.insert(page_id, victim_frame_id);
        } // write lock will be droppped.

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
