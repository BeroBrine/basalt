use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BasaltError {
    #[error("I/O Error")]
    IoError(#[from] io::Error),

    #[error("Page {0} is out of bounds.")]
    PageOutOfBounds(u32),

    #[error("Page Id {0} is not found.")]
    PageNotFound(u32),

    #[error("Page is corrupted")]
    CorruptedPage,

    #[error("Slot Index {0} is out of bounds")]
    SlotOutOfBounds(u32),

    #[error("Slot {0} is a tombstone slot.")]
    TombstoneSlot(u32),

    #[error("The database engine ran out of allocated memory!")]
    OutOfMemory,

    #[error("The current directory page is full")]
    DirectoryNotEnoughSpace
}

pub type Result<T> = std::result::Result<T, BasaltError>;
