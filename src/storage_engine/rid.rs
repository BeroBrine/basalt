#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Hash)]
pub struct RID {
    pub page_id: u32,
    pub slot_id: u32,
}

impl RID {
    pub fn new(page_id: u32, slot_id: u32) -> Self {
        Self { page_id, slot_id }
    }

    // packs page id and slot id into a 64 bit number
    // first 32 bits are page id and rest 32 bits are for slot_id
    pub fn to_u64(rid: RID) -> u64 {
        let mut packed_u64: u64 = 0x0000000000000000;

        // 32 zeroes assigned at the front by converting to u64
        let mut page_id = rid.page_id as u64;
        let slot_id = rid.slot_id as u64;

        // bit shift all the zeroes to the left
        page_id = page_id << 32;


        // packing the first 32 bits with page_id
        packed_u64 |= page_id;

        // packing the rest of the 32 bits with slot_id;
        packed_u64 |= slot_id;

        packed_u64
    }


    pub fn from_u64(val: u64) -> Self { 
        // the first 32 bits are packed with page id 
        let page_id = (val >> 32) as u32;

        // mask with 0xFFFFFFFF (32 1 bits) to extract the slot id 
        let slot_id = (val & 0xFFFFFFFF) as u32;

        Self {
            page_id,
            slot_id
        }

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rid_new() {
        let rid = RID::new(10, 20);
        assert_eq!(rid.page_id, 10);
        assert_eq!(rid.slot_id, 20);
    }

    #[test]
    fn test_rid_to_u64() {
        let rid = RID::new(0x12345678, 0x9ABCDEF0);
        let packed = RID::to_u64(rid);
        assert_eq!(packed, 0x123456789ABCDEF0);
    }

    #[test]
    fn test_rid_from_u64() {
        let packed: u64 = 0x123456789ABCDEF0;
        let rid = RID::from_u64(packed);
        assert_eq!(rid.page_id, 0x12345678);
        assert_eq!(rid.slot_id, 0x9ABCDEF0);
    }

    #[test]
    fn test_rid_to_and_from_u64() {
        let rid = RID::new(42, 84);
        let packed = RID::to_u64(rid);
        let unpacked = RID::from_u64(packed);
        
        assert_eq!(unpacked.page_id, 42);
        assert_eq!(unpacked.slot_id, 84);
        assert_eq!(rid, unpacked);
    }
}
