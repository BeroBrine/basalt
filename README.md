# Basalt Database Engine

Basalt is a relational database storage engine built from scratch in Rust. The primary objective of this project is to explore the rigorous realities of database internals, focusing heavily on zero-copy memory management, disk-level data integrity, and high-concurrency systems programming.

## Architecture

Basalt is being constructed sequentially from the disk up. Currently, the project is focused on the foundational memory storage layer: the Slotted Page.

### The Slotted Page
The storage model is built around a custom 4KB slotted page architecture. This layer serves as the absolute source of truth for raw byte management before data touches the hard drive or the B-Tree indexes. 

Core implementation details include:

* **Zero-Copy Memory Safety:** Pages are backed directly by raw byte arrays (`[u8; 4096]`). Reads and writes leverage Rust's strict slice semantics to guarantee safe memory access without incurring the overhead of heap allocations.
* **Stable Pointers & Tombstones:** In relational databases, shifting a slot array to reclaim memory will corrupt external index pointers (Record IDs). Basalt solves this by utilizing a tombstone mechanism; deleted slots are marked and recycled for future inserts, ensuring that logical pointers remain strictly immutable.
* **Stack-Allocated Compaction:** Page defragmentation (vacuuming) is handled via a compile-time sized temporary buffer. This keeps the entire compaction process within the CPU's L1 cache, bypassing the heap entirely and ensuring that byte-shifting is exceptionally fast and functionally atomic.
* **Platform Agnosticism:** All page headers and slot directories are strictly serialized using Little Endian byte ordering. This ensures that Basalt `.db` files remain fully portable across different CPU architectures.
* **Corruption Protection:** Strict bounds-checking is enforced at the page level. The engine validates all offset and length parameters before slicing physical memory, preventing panics in the event of disk-level data corruption.

## Roadmap

The current development trajectory is focused on moving up the storage stack:

1. **Storage Layer (Completed):** Slotted pages, raw byte manipulation, and fragmentation management.
2. **Disk Manager (Up Next):** Interfacing with the operating system. This will utilize Unix-specific file extensions to allow for highly concurrent, lock-free page reads using direct byte offsets.
3. **Buffer Pool Manager:** Implementing cache eviction policies (such as LRU) to manage the boundary between volatile RAM and persistent disk storage.
4. **Indexing:** Developing a disk-resident B+ Tree for efficient range scans and point queries.
5. **Execution Engine:** Implementing relational algebra operators and a query execution plan.

## Getting Started

To run the test suite and verify the integrity of the storage layer:

```bash
git clone https://github.com/BeroBrine/basalt.git
cd basalt
cargo test
```

