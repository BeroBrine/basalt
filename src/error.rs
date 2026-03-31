use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BasaltError {
    #[error("I/O Error")]
    IoError(#[from] io::Error),

    #[error("Page {0} is out of bounds")]
    PageOutOfBounds(u64),

    #[error("Page corrupted")]
    CorruptedPage,

    #[error("Slot Index {0} is out of bounds")]
    SlotOutOfBounds(u16),

    #[error("Slot {0} is a tombstone slot.")]
    TombstoneSlot(u16),
}

pub type Result<T> = std::result::Result<T, BasaltError>;
