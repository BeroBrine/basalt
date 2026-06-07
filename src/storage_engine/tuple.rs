use crate::storage_engine::rid::RID;

pub struct Tuple {
    pub rid: Option<RID>,
    pub data: Vec<u8>

}


impl Tuple {

    //new tuple unallocated on disk (without any rid)
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            rid: None,
            data
        }
    }

    // create a tuple with existing rid and data
    pub fn with_rid(rid: RID , data: Vec<u8>) -> Self {
        Self {
            rid: Some(rid), 
            data
        }

    }

    // get the length of the raw payload
    pub fn get_data_length(&self) -> usize {
        self.data.len()
    }

}
