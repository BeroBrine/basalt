use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{
    buffer::replacer::{self, Replacer},
    storage_engine::{
        disk_manager::{self, DiskManager},
        page::Page,
    },
};

struct Frame {
    page: Page,
    page_id: Option<usize>,
    is_dirty: bool,
    pin_count: usize,
}

impl Frame {
    pub fn new() -> Self {
        Self {
            page: Page::new(0),
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
    page_table: RwLock<HashMap<usize, usize>>,
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

        // initialize page in each frame.
        for _ in 0..pool_size {
            frames.push(Arc::new(RwLock::new(Frame::new())));
        }

        BufferManager {
            frames,
            page_table: RwLock::new(HashMap::with_capacity(pool_size)),
            replacer,
            disk_manager,
        }
    }
}
