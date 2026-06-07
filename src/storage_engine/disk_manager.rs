use std::{
    fs::{File, OpenOptions},
    os::unix::fs::FileExt,
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    error::{BasaltError, Result},
    storage_engine::page::{PAGE_SIZE, TablePageGuard, PAGE_HEADER_SIZE},
};

pub struct DiskManager {
    file: File,
    next_page_id: AtomicU32,
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
    
        let next_page_id = (file_len  / PAGE_SIZE as u64) as u32; 

        Ok(Self {
            file,
            next_page_id: AtomicU32::new(next_page_id),
        })
    }

    pub fn read_page(&self, page_id: u32, page: &mut TablePageGuard) -> Result<()> {
        self.validate_page_id(page_id)?;

        let offset = (page_id * PAGE_SIZE as u32) as u64; 

        // this operation allows to bypass the sequental cursor for file.
        // allows to read at an offset in a file without affecting the current cursor basically a
        // no lock multi threaded implementation.
        // rust allows this only for a immutable self ref -> &self;
        self.file.read_exact_at(page.data, offset)?;

        Ok(())
    }

    pub fn write_page(&self, page_id: u32, page: &TablePageGuard) -> Result<()> {
        self.validate_page_id(page_id)?;

        let offset = ((page_id as usize) * PAGE_SIZE) as u64;

        self.file.write_all_at(page.data, offset)?;

        Ok(())
    }

    pub fn validate_page_id(&self, page_id: u32) -> Result<()> {
        // ordering relaxed is used to atomically read the value of next_page_id which is extremely
        // fast as it doesn't care about the current memory layout of multiple threads that are r/w
        // to this variable.
        if page_id >= self.next_page_id.load(Ordering::Relaxed) {
            return Err(BasaltError::PageOutOfBounds(page_id));
        }
        Ok(())
    }

    pub fn allocate_page(&self) -> u32 {
        // SeqCst guarantees that at the time of operation , the value of this variable will be the
        // same for all the threads
        //NOTE: return PREVIOUS value of next page id
        self.next_page_id.fetch_add(1, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_disk_manager_new() {
        let file_path = "test_db_new.db";
        {
            let dm = DiskManager::new(file_path).unwrap();
            assert_eq!(dm.allocate_page(), 0);
        }
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_disk_manager_allocate_page() {
        let file_path = "test_db_allocate.db";
        {
            let dm = DiskManager::new(file_path).unwrap();
            assert_eq!(dm.allocate_page(), 0);
            assert_eq!(dm.allocate_page(), 1);
            assert_eq!(dm.allocate_page(), 2);
        }
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_disk_manager_read_write_page() {
        let file_path = "test_db_rw.db";
        {
            let dm = DiskManager::new(file_path).unwrap();
            let page_id = dm.allocate_page();
            
            let mut data = [0u8; PAGE_SIZE];
            let mut page = TablePageGuard::new(&mut data);
            page.init(page_id, 0, 0);
            
            let test_data = b"DiskManager Test";
            page.data[PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + test_data.len()].copy_from_slice(test_data);

            dm.write_page(page_id, &page).unwrap();

            let mut read_data = [0u8; PAGE_SIZE];
            let mut read_page = TablePageGuard::new(&mut read_data);
            dm.read_page(page_id, &mut read_page).unwrap();

            assert_eq!(&read_page.data[PAGE_HEADER_SIZE..PAGE_HEADER_SIZE + test_data.len()], test_data);
        }
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_disk_manager_reopen() {
        let file_path = "test_db_reopen.db";
        {
            let dm = DiskManager::new(file_path).unwrap();
            let p0 = dm.allocate_page(); // 0
            let p1 = dm.allocate_page(); // 1
            
            let mut data = [0u8; PAGE_SIZE];
            let page = TablePageGuard::new(&mut data);
            
            // Writing to the second page will grow the file to at least 2 * PAGE_SIZE
            dm.write_page(p1, &page).unwrap();
        }
        {
            let dm = DiskManager::new(file_path).unwrap();
            assert_eq!(dm.allocate_page(), 2);
        }
        fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn test_disk_manager_invalid_page() {
        let file_path = "test_db_invalid.db";
        {
            let dm = DiskManager::new(file_path).unwrap();
            let mut data = [0u8; PAGE_SIZE];
            let mut page = TablePageGuard::new(&mut data);
            let result = dm.read_page(0, &mut page);
            assert!(matches!(result, Err(BasaltError::PageOutOfBounds(0))));
        }
        fs::remove_file(file_path).unwrap();
    }
}
