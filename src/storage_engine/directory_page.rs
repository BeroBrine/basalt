// directory that will contain page id and it's corresponding free space
// record -> [page_id (4 bytes) | FreeSpace (4 bytes) ]

use crate::{error::{BasaltError, Result}, storage_engine::page::PAGE_SIZE};


// header layout
// 4 bytes -> no of entries
// 4 bytes -> next directory page id

const DIR_HEADER_SIZE: usize = 8;
const DIR_ENTRY_SIZE: usize = 8; 
const DIR_MAX_ENTRIES: u32 = ((PAGE_SIZE - DIR_HEADER_SIZE)  / DIR_ENTRY_SIZE ) as u32;

pub struct DirectoryPageGuard<'a> {
    pub data: &'a mut [u8; PAGE_SIZE],
}


impl<'a> DirectoryPageGuard<'a> {

    pub fn new(data: &'a mut [u8; PAGE_SIZE]) -> Self {
        Self {data}
    }

    pub fn init(next_directory_page_id: u32) {

    }


    pub fn get_no_of_entries(&self) -> u32 {
        let mut no_of_entries_bytes = [0u8; 4];
        let no_of_entries_header_offset = 0..4;

        no_of_entries_bytes.copy_from_slice(&self.data[no_of_entries_header_offset]);

        u32::from_le_bytes(no_of_entries_bytes)

    }

    pub fn set_no_of_entries(&mut self , entries_no: u32) {
        let entries_no_le_bytes = entries_no.to_le_bytes();
        let no_of_entries_header_offset = 0..4;
        
        self.data[no_of_entries_header_offset].copy_from_slice(&entries_no_le_bytes);
    }

    pub fn get_next_directory_page_id(&self) -> u32 {
        let mut next_directory_page_id_bytes = [0u8; 4];
        let next_directory_page_id_header_offset = 4..8;

        next_directory_page_id_bytes.copy_from_slice(&self.data[next_directory_page_id_header_offset]);

        u32::from_le_bytes(next_directory_page_id_bytes)
    }

    pub fn set_next_directory_page_id(&mut self , next_directory_page_id: u32) {
        let next_directory_page_id_le_bytes = next_directory_page_id.to_le_bytes();

        let next_directory_page_id_header_offset = 4..8;

        self.data[next_directory_page_id_header_offset].copy_from_slice(&next_directory_page_id_le_bytes);
    }

    fn get_entry_offset(index_no: u32) -> usize {
        (index_no as usize * DIR_ENTRY_SIZE) + DIR_HEADER_SIZE
    }

    // searches in the directory for the page id that contains the required freespace , returns
    // PageId if found , None if not found
    pub fn get_required_freespace_page_id(&self , freespace_required: u32) -> Option<u32> {
        let no_of_entries = self.get_no_of_entries();
        

        for i in 0..no_of_entries {
            let entry_offset = Self::get_entry_offset(i);

            let mut page_id_bytes = [0u8;4];
            let mut freespace_bytes = [0u8;4];

            let entry_page_id_bytes = &self.data[entry_offset..entry_offset + 4]; 
            let entry_freespace_bytes = &self.data[entry_offset + 4.. entry_offset + 8]; 

            page_id_bytes.copy_from_slice(entry_page_id_bytes);
            freespace_bytes.copy_from_slice(entry_freespace_bytes);

            let page_id = u32::from_le_bytes(page_id_bytes);
            let freespace = u32::from_le_bytes(freespace_bytes);

            if freespace >= freespace_required {
                return Some(page_id);
            }

        }
        None

    }

    pub fn update_page_freespace(&mut self , page_id: u32 , freespace: u32) -> Result<()> {

        let no_of_entries = self.get_no_of_entries();
        let mut count = 0;

        for count in 0..no_of_entries {
            let entry_offset = Self::get_entry_offset(count);

            let mut page_id_bytes = [0u8;4];
            let entry_page_id_bytes = &self.data[entry_offset..entry_offset + 4]; 
            page_id_bytes.copy_from_slice(entry_page_id_bytes);

            let entry_page_id = u32::from_le_bytes(page_id_bytes);

            if page_id == entry_page_id {
                self.data[entry_offset+4..entry_offset+8].copy_from_slice(&freespace.to_le_bytes());
                return Ok(())
            }
        }

        if count >= DIR_MAX_ENTRIES {
            // allocate a new directory page
            return Err(BasaltError::DirectoryNotEnoughSpace)
        }

        let new_offset = Self::get_entry_offset(count);
        self.data[new_offset..new_offset + 4].copy_from_slice(&page_id.to_le_bytes());
        self.data[new_offset + 4..new_offset + 8].copy_from_slice(&freespace.to_le_bytes());

        self.set_no_of_entries(count + 1);



        Ok(())


    }


    

}
