# Basalt: A High-Performance Storage Engine in Rust

Basalt is a persistent storage engine built from the ground up in Rust, designed with a focus on low-level memory management, concurrency safety, and efficient disk I/O. It implements a slotted-page architecture with a robust buffer pool management system.

## 🏗 Architecture Overview

The project is structured into several core layers, each handling a specific aspect of the database's storage lifecycle:

### 1. Storage Engine (Slotted Pages)
Basalt uses a **Slotted Page** format for its 4KB disk blocks. This architecture allows for:
- **Variable-length Record Support:** Efficiently managing tuples of different sizes.
- **Tombstone Deletions:** Records are marked as deleted (tombstoned) for O(1) deletion performance.
- **In-Memory Vacuuming:** A defragmentation process that reclaims space from tombstoned records by shifting live data and updating the slot array.
- **Overflow Chaining:** Handles tuples larger than a single page by transparently chaining chunks across multiple pages using a bottom-up recursive approach.

### 2. Buffer Pool Manager
To minimize disk latency, Basalt implements a thread-safe Buffer Pool Manager:
- **Clock Replacement Policy:** An efficient approximation of LRU (Least Recently Used) for page eviction decisions.
- **Pinning Mechanism:** Ensures that pages currently being accessed by the engine are not evicted.
- **Concurrency Control:** Utilizes `RwLock` and `Arc` to provide safe multi-threaded access to cached frames.
- **Deadlock Avoidance:** Implements a strict lock hierarchy (Page Table -> Frame) to prevent cyclic waits during high-concurrency workloads.

### 3. Table Heap & Directory Management
Data is organized in a **Table Heap**:
- **Directory Chaining:** A multi-level linked list of directory pages that track the free space available in every data page.
- **Smart Allocation:** The engine automatically finds the best-fit page for new records or allocates fresh pages if the current heap is full.
- **RID-based Addressing:** Every record is uniquely identified by a Record ID (RID) consisting of a `PageID` and a `SlotID`.

## 🛠 Technical Implementation Details

### Concurrency & Deadlock Resolution
One of the key engineering challenges was resolving a race condition between the global Page Table lock and individual Frame locks. We enforced a **Hierarchical Locking Order**:
1. Acquire the **Page Table** lock (Read/Write).
2. Acquire the **Frame** lock.
3. Perform the operation.
4. Release locks in reverse order.
This ensures that the engine can handle multiple concurrent readers and writers without risk of deadlock or data corruption.

### Record Overflow Handling
For records exceeding the `MAX_TUPLE_SIZE` (~2KB - 4KB), Basalt employs a recursive fragmentation strategy. Each fragment stores a 1-byte flag (`FLAG_NORMAL` or `FLAG_OVERFLOW`) and a pointer (RID) to the next fragment, allowing for perfectly reassembled reads of arbitrary data sizes.

## 🚀 Getting Started

### Prerequisites
- Rust (Latest Stable)
- Cargo

### Running Tests
The project includes a comprehensive test suite covering basic CRUD, overflow handling, and directory expansion.
```bash
cargo test
```

## 🗺 Roadmap

Basalt is currently in its first phase of development. The next phases include:

### Phase 2: Durability & Recovery (Next)
- **Write-Ahead Logging (WAL):** Implementing the ARIES recovery algorithm.
- **Log Sequence Numbers (LSN):** Tracking page updates for ACID compliance.
- **Checkpointing:** Reducing recovery time by periodically flushing dirty pages and log records.

### Phase 3: Access Methods
- **B+ Tree Indexing:** Supporting efficient range scans and point lookups.
- **Index Management:** Automated index updates during heap modifications.

### Phase 4: Query Execution
- **SQL Parser:** Basic query parsing into logical plans.
- **Execution Engine:** A Volcano-style iterator model for query processing.

---

**Developed by [Abhishek Rana](https://www.linkedin.com/in/abhishek-rana-650735244/)**
