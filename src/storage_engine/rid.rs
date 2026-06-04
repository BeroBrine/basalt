#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Hash)]
struct RID {
    page_id: u32,
    slot_id: u32,
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
        let page_id = (val << 32) as u32;

        // mask with 0xFFFFFFF (32 1 bits) to extract the slot id 
        let slot_id = (val & 0xFFFFFFFF) as u32;

        Self {
            page_id,
            slot_id
        }

    }


}
