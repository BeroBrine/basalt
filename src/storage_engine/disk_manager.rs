use std::{
    fs::{File, OpenOptions},
    os::unix::fs::FileExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    error::{BasaltError, Result},
    storage_engine::page::{PAGE_SIZE, Page},
};

pub struct DiskManager {
    file: File,
    next_page_id: AtomicU64,
}

impl DiskManager {
    pub fn new(file_path: impl AsRef<Path>) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(file_path)?;

        // hydrate next page id from the actual file size
        let metadata = file.metadata()?;

        let file_len = metadata.len();

        let next_page_id = file_len / (PAGE_SIZE as u64);

        Ok(Self {
            file,
            next_page_id: AtomicU64::new(next_page_id),
        })
    }

    pub fn read_page(&self, page_id: u64, page: &mut Page) -> Result<()> {
        self.validate_page_id(page_id)?;

        let offset = page_id * (PAGE_SIZE as u64);

        // this operation allows to bypass the sequental cursor for file.
        // allows to read at an offset in a file without affecting the current cursor basically a
        // no lock multi threaded implementation.
        // rust allows this only for a immutable self ref -> &self;
        self.file.read_exact_at(page.get_raw_data_mut(), offset)?;

        Ok(())
    }

    pub fn write_page(&self, page_id: u64, page: &mut Page) -> Result<()> {
        self.validate_page_id(page_id)?;

        let offset = page_id * (PAGE_SIZE as u64);

        self.file.write_all_at(page.get_raw_data_mut(), offset)?;

        Ok(())
    }

    pub fn validate_page_id(&self, page_id: u64) -> Result<()> {
        // ordering relaxed is used to atomically read the value of next_page_id which is extremely
        // fast as it doesn't care about the current memory layout of multiple threads that are r/w
        // to this variable.
        if page_id >= self.next_page_id.load(Ordering::Relaxed) {
            return Err(BasaltError::PageOutOfBounds(page_id));
        }
        Ok(())
    }

    pub fn allocate_page(&self) -> u64 {
        // SeqCst guarantees that at the time of operation , the value of this variable will be the
        // same for all the threads
        //NOTE: return PREVIOUS value of next page id
        self.next_page_id.fetch_add(1, Ordering::SeqCst)
    }
}
