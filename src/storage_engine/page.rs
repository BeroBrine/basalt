use crate::error::{BasaltError, Result};

/// A Slotted Page encapsulating a 4KB block of memory.
/// Layout:
/// [Header (24B)] [Slot Array ->] ... [Free Space] ... [<- Data Records]
pub const PAGE_SIZE: usize = 4096;
pub const SLOT_SIZE: usize = 4; // 2 byte offset + 2 bye length

/// Header Offsets. These use byte index (slot_idx is the only that uses logical index )
pub const OFFSET_PAGE_ID: usize = 0; // 8 bytes
pub const OFFSET_LSN: usize = 8; // 8 bytes. Reserving space for future implementation of WAL 
pub const OFFSET_FREESPACE_UPPER_BOUND: usize = 16; // 2 bytes  starts from the end
pub const OFFSET_FREESPACE_LOWER_BOUND: usize = 18; // 2 bytes  starts from the header
pub const OFFSET_NUMBER_OF_SLOTS: usize = 20; // 2 bytes
pub const OFFSET_RESERVED: usize = 24; // reserved space for better padding alignment.
pub const PAGE_HEADER_SIZE: usize = 24; // ideally should be a multiple of 8.

pub struct Page {
    data: [u8; PAGE_SIZE],
}

impl Page {
    pub fn new(page_id: u64) -> Self {
        let mut page = Self {
            data: [0; PAGE_SIZE],
        };

        page.set_page_id(page_id);
        page.set_lsn(0);
        page.set_no_of_slots(0);
        page.set_freespace_upper_bound_offset(PAGE_SIZE as u16);
        page.set_freespace_lower_bound_offset(PAGE_HEADER_SIZE as u16);

        page
    }

    pub fn get_page_id(&self) -> u64 {
        // slice the page id bytes from the header
        let page_id_bytes_slice = OFFSET_PAGE_ID..OFFSET_PAGE_ID + 8; // 8 bytes
        let page_id_le_bytes = self.data[page_id_bytes_slice]
            .try_into()
            .expect("Error reading page id bytes");
        u64::from_le_bytes(page_id_le_bytes)
    }

    pub fn set_page_id(&mut self, page_id: u64) {
        let page_id_le_bytes = page_id.to_le_bytes();
        let page_id_bytes_slice = OFFSET_PAGE_ID..OFFSET_PAGE_ID + 8; // 8 bytes
        self.data[page_id_bytes_slice].copy_from_slice(&page_id_le_bytes);
    }

    pub fn get_lsn(&self) -> u64 {
        let lsn_bytes_slice = OFFSET_LSN..OFFSET_LSN + 8; // 8 bytes
        let lsn_le_bytes = self.data[lsn_bytes_slice]
            .try_into()
            .expect("Error reading lsn.");
        u64::from_le_bytes(lsn_le_bytes)
    }

    pub fn set_lsn(&mut self, lsn: u64) {
        let lsn_bytes_slice = OFFSET_LSN..OFFSET_LSN + 8;
        let lsn_le_bytes = lsn.to_le_bytes();
        self.data[lsn_bytes_slice].copy_from_slice(&lsn_le_bytes);
    }

    pub fn get_no_of_slots(&self) -> u16 {
        let no_of_slots_bytes_slice = OFFSET_NUMBER_OF_SLOTS..OFFSET_NUMBER_OF_SLOTS + 2; // 2 bytes
        let no_of_slots_le_bytes = self.data[no_of_slots_bytes_slice]
            .try_into()
            .expect("Error reading number of slots bytes");
        u16::from_le_bytes(no_of_slots_le_bytes)
    }

    pub fn set_no_of_slots(&mut self, no_of_slots: u16) {
        let no_of_slots_le_bytes = no_of_slots.to_le_bytes();
        let no_of_slots_bytes_slice = OFFSET_NUMBER_OF_SLOTS..OFFSET_NUMBER_OF_SLOTS + 2; // 2 bytes
        self.data[no_of_slots_bytes_slice].copy_from_slice(&no_of_slots_le_bytes);
    }

    pub fn get_freespace_lower_bound_offset(&self) -> u16 {
        let free_space_offset_slice =
            OFFSET_FREESPACE_LOWER_BOUND..OFFSET_FREESPACE_LOWER_BOUND + 2; // 2 bytes
        let free_space_offset_le_bytes = self.data[free_space_offset_slice]
            .try_into()
            .expect("Error reading free space offset bytes");
        u16::from_le_bytes(free_space_offset_le_bytes)
    }

    pub fn set_freespace_lower_bound_offset(&mut self, new_offset: u16) {
        let free_space_offset_bytes_slice =
            OFFSET_FREESPACE_LOWER_BOUND..OFFSET_FREESPACE_LOWER_BOUND + 2;
        let new_offset_le_bytes = new_offset.to_le_bytes();
        self.data[free_space_offset_bytes_slice].copy_from_slice(&new_offset_le_bytes);
    }

    pub fn get_freespace_upper_bound_offset(&self) -> u16 {
        let free_space_offset_slice =
            OFFSET_FREESPACE_UPPER_BOUND..OFFSET_FREESPACE_UPPER_BOUND + 2; // 2 bytes
        let free_space_offset_le_bytes = self.data[free_space_offset_slice]
            .try_into()
            .expect("Error reading free space offset bytes");
        u16::from_le_bytes(free_space_offset_le_bytes)
    }

    pub fn set_freespace_upper_bound_offset(&mut self, new_offset: u16) {
        let free_space_offset_bytes_slice =
            OFFSET_FREESPACE_UPPER_BOUND..OFFSET_FREESPACE_UPPER_BOUND + 2;
        let new_offset_le_bytes = new_offset.to_le_bytes();
        self.data[free_space_offset_bytes_slice].copy_from_slice(&new_offset_le_bytes);
    }

    pub fn get_freespace(&self) -> u16 {
        self.get_freespace_upper_bound_offset() - self.get_freespace_lower_bound_offset()
    }

    pub fn is_tombstone_slot(&self, slot_idx: u16) -> Result<bool> {
        let (record_offset, record_len) = self.get_record_offset_len_tuple(slot_idx)?;

        Ok(record_offset == 0 && record_len == 0)
    }

    pub fn get_slot(&self, slot_idx: u16) -> Result<&[u8]> {
        if slot_idx >= self.get_no_of_slots() {
            return Err(BasaltError::SlotOutOfBounds(slot_idx));
        }

        if self.is_tombstone_slot(slot_idx)? {
            return Err(BasaltError::TombstoneSlot(slot_idx));
        }

        let slot_offset = PAGE_HEADER_SIZE + (slot_idx as usize * SLOT_SIZE);
        Ok(&self.data[slot_offset..slot_offset + SLOT_SIZE])
    }

    pub fn get_slot_mut(&mut self, slot_idx: u16) -> Result<&mut [u8]> {
        if slot_idx >= self.get_no_of_slots() {
            return Err(BasaltError::SlotOutOfBounds(slot_idx));
        }

        if self.is_tombstone_slot(slot_idx)? {
            return Err(BasaltError::TombstoneSlot(slot_idx));
        }

        let slot_offset = PAGE_HEADER_SIZE + (slot_idx as usize * SLOT_SIZE);

        Ok(&mut self.data[slot_offset..slot_offset + SLOT_SIZE])
    }

    // internal helper function that does not check if the slot is deleted or not
    fn get_slot_raw(&self, slot_idx: u16) -> Result<&[u8]> {
        if slot_idx >= self.get_no_of_slots() {
            return Err(BasaltError::SlotOutOfBounds(slot_idx));
        }
        let slot_offset = PAGE_HEADER_SIZE + (slot_idx as usize * SLOT_SIZE);
        Ok(&self.data[slot_offset..slot_offset + SLOT_SIZE])
    }

    // Fetches the slot and returns  a tuple containing record offset ptr and record len
    pub fn get_record_offset_len_tuple(&self, slot_idx: u16) -> Result<(usize, usize)> {
        // using the internal helper to bypass the tombstone check
        let slot = self.get_slot_raw(slot_idx)?;

        let record_offset_le_bytes = slot[0..2].try_into().unwrap();
        let record_offset = u16::from_le_bytes(record_offset_le_bytes) as usize;

        let record_len_le_bytes = slot[2..4].try_into().unwrap();
        let record_len = u16::from_le_bytes(record_len_le_bytes) as usize;

        Ok((record_offset, record_len))
    }

    pub fn insert(&mut self, record: &[u8]) -> Option<u16> {
        let record_len = record.len() as u16;

        // Sanity check: is the record itself bigger than the page?
        if record_len >= PAGE_SIZE as u16 {
            return None;
        }

        let lower_bound_offset = self.get_freespace_lower_bound_offset() as usize;
        let upper_bound_offset = self.get_freespace_upper_bound_offset() as usize;
        let slots_no = self.get_no_of_slots();
        // hunt for existing tombstone
        let mut tombstone_slot_idx: Option<u16> = None;

        for i in 0..slots_no {
            let (record_offset, record_len) = self.get_record_offset_len_tuple(i).unwrap();

            // skip live slots
            if record_offset > 0 && record_len > 0 {
                continue;
            }

            // fetch the first tombstone encountered
            tombstone_slot_idx = Some(i);
            break;
        }

        let extra_space_needed_for_slot = if tombstone_slot_idx.is_some() {
            0
        } else {
            SLOT_SIZE as u16
        };

        let total_record_len = record_len + extra_space_needed_for_slot;

        let free_space = self.get_freespace();

        if free_space < total_record_len {
            return None; // NOTE: Vacumming will be done by the engine layer. SOC 
        }

        // Assign record.. (at the end of the page)
        let new_upper_offset = upper_bound_offset - record_len as usize;

        self.data[new_upper_offset..upper_bound_offset].copy_from_slice(record);

        // Assign tombstone or append at the end.. (after the headers)
        //
        let final_slot_idx = tombstone_slot_idx.unwrap_or(slots_no);

        // since insert is an trusted mutation that grows the array , this operation is safe.
        let slot_offset = PAGE_HEADER_SIZE + (final_slot_idx as usize * SLOT_SIZE);
        let slot = &mut self.data[slot_offset..slot_offset + SLOT_SIZE];

        // casting to u16 is necessary as new_upper_offset is usize , to_le_bytes will generate an
        // array of 8 bytes instead of 2 bytes(u16) which will panic the program as copy from slice
        // requires both dest and src to be the same size.
        slot[0..2].copy_from_slice(&(new_upper_offset as u16).to_le_bytes()); // record offset
        slot[2..4].copy_from_slice(&record_len.to_le_bytes()); // record size

        let new_lower_offset = if tombstone_slot_idx.is_some() {
            lower_bound_offset
        } else {
            lower_bound_offset + SLOT_SIZE
        };

        // update headers
        self.set_freespace_upper_bound_offset(new_upper_offset as u16);
        self.set_freespace_lower_bound_offset(new_lower_offset as u16);

        // only increase if new slot was assigned
        if tombstone_slot_idx.is_none() {
            self.set_no_of_slots(slots_no + 1);
        }

        Some(final_slot_idx)
    }

    pub fn get_record(&self, slot_idx: u16) -> Result<&[u8]> {
        let (record_offset, record_len) = self.get_record_offset_len_tuple(slot_idx)?;

        // protection against data corruption on disk
        if record_offset + record_len > PAGE_SIZE {
            return Err(BasaltError::CorruptedPage);
        }

        Ok(&self.data[record_offset..record_offset + record_len])
    }

    // will the record ever be mutated directly?
    pub fn get_record_mut(&mut self, slot_idx: u16) -> Result<&mut [u8]> {
        let (record_offset, record_len) = self.get_record_offset_len_tuple(slot_idx)?;

        // protection against data corruption on disk
        if record_offset + record_len > PAGE_SIZE {
            return Err(BasaltError::CorruptedPage);
        }

        Ok(&mut self.data[record_offset..record_offset + record_len])
    }

    // tombstone strategy
    pub fn delete(&mut self, slot_idx: u16) -> Result<()> {
        let tombstone_slot = &mut self.get_slot_mut(slot_idx)?;

        tombstone_slot.fill(0);

        Ok(())
    }

    pub fn vacuum(&mut self) -> Result<()> {
        // rust stores this on the CPU stack (L1 cache) as it's size is known on compile time. no
        // heap allocation is done so optimizing this function for in place vacuum is trivial.
        let mut temp_buf = [0u8; PAGE_SIZE];

        // copy the headers and slot array -> lower bound
        let lower_bound = self.get_freespace_lower_bound_offset() as usize;
        // copy from slice demands that the both src and dest be the same size.
        temp_buf[0..lower_bound].copy_from_slice(&self.data[0..lower_bound]);

        let mut new_upper_bound = PAGE_SIZE;
        let total_slots = self.get_no_of_slots();

        for i in 0..total_slots {
            // only apply operations on live slots
            if self.is_tombstone_slot(i)? {
                continue;
            }

            // move data to temp buf
            let (_, record_len) = self.get_record_offset_len_tuple(i)?;

            let live_record = self.get_record(i)?;
            let upper_bound = new_upper_bound - record_len;
            new_upper_bound = upper_bound;

            temp_buf[upper_bound..upper_bound + record_len].copy_from_slice(live_record);

            // update the temp buf slot
            let slot_offset = PAGE_HEADER_SIZE + (i as usize * SLOT_SIZE);
            let slot = &mut temp_buf[slot_offset..slot_offset + SLOT_SIZE];

            slot[0..2].copy_from_slice(&(upper_bound as u16).to_le_bytes());
            slot[2..4].copy_from_slice(&(record_len as u16).to_le_bytes());
        }

        self.data = temp_buf;
        self.set_freespace_upper_bound_offset(new_upper_bound as u16);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_initilization() {
        let page_id = 42;
        let page = Page::new(page_id);

        assert_eq!(page.get_page_id(), 42);
        assert_eq!(page.get_lsn(), 0);
        assert_eq!(page.get_no_of_slots(), 0);
        assert_eq!(page.get_freespace_upper_bound_offset(), PAGE_SIZE as u16);
        assert_eq!(
            page.get_freespace_lower_bound_offset(),
            PAGE_HEADER_SIZE as u16
        );

        // freespace -> 4096 - 24 = 4072 free space left
        assert_eq!(page.get_freespace(), (PAGE_SIZE - PAGE_HEADER_SIZE) as u16)
    }

    #[test]
    fn test_insert_and_get() {
        let mut page = Page::new(1);
        let record = b"Hello! Basalt!"; // 14 bytes 

        let slot_idx = page.insert(record).expect("Failed to insert record");

        assert_eq!(slot_idx, 0);
        assert_eq!(page.get_no_of_slots(), 1);

        // checking upper bound moved by 14 bytes
        assert_eq!(
            page.get_freespace_upper_bound_offset(),
            (PAGE_SIZE - record.len()) as u16
        );

        // checking lower bound moved by 4 bytes (Slot Size)
        assert_eq!(
            page.get_freespace_lower_bound_offset(),
            (PAGE_HEADER_SIZE + SLOT_SIZE) as u16
        );

        let fetched_record = page.get_record(slot_idx).expect("Failed to get record");
        assert_eq!(fetched_record, record);
    }

    #[test]
    fn test_insert_out_of_space() {
        let mut page = Page::new(1);
        let large_record = &[0u8; PAGE_SIZE + 1];

        let result = page.insert(large_record);

        assert!(
            result.is_none(),
            "Should reject records that are bigger than {PAGE_SIZE} bytes"
        );

        // testing data that fits in the raw page but not in the available space of 4076 bytes
        let huge_record = &[0u8; 4077];
        let insert_result = page.insert(huge_record);

        assert!(
            insert_result.is_none(),
            "Should reject records that do not fit in the available space."
        );
    }

    #[test]
    fn test_delete_and_tombstone_reuse() {
        let mut page = Page::new(1);

        let record1 = b"record_1";
        let record2 = b"record_2";
        let record3 = b"record_3";

        let slot0 = page.insert(record1).expect("Failed to insert record 1");
        let _ = page.insert(record2).expect("Failed to insert record 2");
        let slot_count = page.get_no_of_slots();
        let no_of_test_slot = 2;

        assert_eq!(slot_count, no_of_test_slot);

        page.delete(slot0).expect("Failed to delete slot 0");

        // fetching deleted slot should return expected error.
        let get_result = page.get_record(slot0);

        assert!(matches!(get_result, Err(BasaltError::TombstoneSlot(_))));

        let is_slot0_tombstone = page
            .is_tombstone_slot(slot0)
            .expect("Failed to check if slot0 is a tombstone or not");

        assert!(is_slot0_tombstone, "Slot0 must be a tombstone");

        let slot3 = page.insert(record3).expect("Failed to insert record 3");

        assert_eq!(slot3, 0, "Slot3 must be inserted at idx 0");

        let is_slot3_tombstone = page
            .is_tombstone_slot(slot3)
            .expect("Failed to check if slot3 is a tombstone or not");

        assert!(!is_slot3_tombstone, "Slot3 must not be a tombstone");

        // The no of slots still must remain 2 as no new slot should not be assigned
        assert_eq!(page.get_no_of_slots(), 2);
    }

    #[test]
    fn test_vacuum() {
        let mut page = Page::new(1);

        let record1 = b"HELLO";
        let record2 = b"WORLD";
        let record3 = b"BASALT";

        let _ = page.insert(record1).expect("Failed to insert record 1 ");
        let slot1 = page.insert(record2).expect("Failed to insert record 2 ");
        let _ = page.insert(record3).expect("Failed to insert record 3 ");

        let free_space_before_delete = page.get_freespace();

        // Deleting the middle record
        page.delete(slot1).expect("Failed to delete slot 1");

        // the space must remain same as before because it's only fragmented.
        let free_space_after_delete = page.get_freespace();

        assert_eq!(free_space_before_delete, free_space_after_delete);

        page.vacuum().expect("Failed to vacuum page");

        let free_space_after_vacuuming = page.get_freespace();

        // free_space_after_vacuuming must exactly be more record2.len then free_space_after_delete
        // as record2 is now defragmented
        assert_eq!(
            free_space_after_vacuuming,
            free_space_after_delete + record2.len() as u16
        );
    }
}
