# Contributing to glean-hs

Thank you for your interest in contributing to glean-hs.

## Getting Started

1. Fork the repository
2. Follow the build instructions in [README.md](README.md)
3. Make your changes
4. Run the test suite: `cargo test && cabal test`
5. Open a pull request

---

## Priority Contributions

### 1. Composite Key Storage (High Priority)

**What:** Replace the current batch storage model with per-fact composite keys.

**Why it matters:** The current implementation stores facts in batches and
scans all batches to answer queries — O(n). For large codebases (50K+ facts,
e.g. indexing the Stackage package set), this becomes slow. Composite keys enable O(1)
point lookups via RocksDB's native prefix scan and bloom filters.

**Current storage model (batch):**
```
RocksDB key:   "batch:0000000000000000"
RocksDB value: [fact1_bytes][fact2_bytes]...[factN_bytes]

Each fact in the value:
  [8 bytes] word64LE(pid)
  [4 bytes] word32LE(body_length)
  [n bytes] body bytes
```

**Target storage model (composite keys):**
```
RocksDB key:   encode(pid) ++ fact_key_bytes
RocksDB value: fact_value_bytes

Where encode(pid) = store_nat(pid) from src/rts/nat.rs
      fact_key_bytes = the fact's key field bytes
```

**Files to modify:**
- `src/storage/rocksdb.rs` — `glean_rocksdb_store` and `glean_rocksdb_retrieve`
- `haskell/src/Glean/Query.hs` — `loadAllFacts` (replace batch scan with prefix scan)
- `haskell/src/Glean/FFI.hs` — may need new FFI functions for prefix iteration

**The nat encoding** (from `src/rts/nat.rs`):
```rust
// Prefix-free variable-length nat encoding
// Values < 128: stored as single byte
// Values >= 128: stored with continuation bits
// This ensures no two different pids produce a prefix of each other
pub fn store_nat(buf: &mut [u8], val: u64) -> usize { ... }
```

**Tests to add** (`haskell/test/Test/Storage.hs`):
- Point lookup by pid + key returns correct fact
- Prefix scan by pid returns all facts for that predicate
- No facts from other predicates appear in results

---

### 2. LMDB Backend (Medium Priority)

**What:** Add an LMDB storage backend alongside the existing RocksDB backend.

**Why it matters:** Meta Glean's benchmarks show LMDB performs 30-40% better
than RocksDB for their workloads. For read-heavy query patterns (index once,
query many times), LMDB may be significantly faster.

**Note on OS compatibility:**
- Linux: LMDB works excellently — sparse files, fast fdatasync
- macOS: LMDB works well — sparse files, good performance
- Windows: LMDB has a significant limitation — `map_size` must be set
  upfront and immediately allocates that much disk space. Not recommended
  on Windows; use RocksDB instead.

**How to implement:**
The `Storage` typeclass in `haskell/src/Glean/Storage.hs` abstracts the
backend. A new `Glean.LMDB` module implementing `Storage` is all that's needed.

```haskell
-- haskell/src/Glean/LMDB.hs
data LMDBStore = ...
instance Storage LMDBStore where
  open   = ...
  close  = ...
  store  = ...
  retrieve = ...
```

On the Rust side, `lmdb-rs` or `heed` crates provide LMDB bindings.

---

### 3. Domain Schemas (Community)

**What:** Write Angle schema files for domains beyond Haskell code.

**Why it matters:** glean-hs can index any structured knowledge domain.
The `src.1` schema (Haskell code) is the first example. Community schemas
extend the platform to new domains.

**Examples:**
- `bio.1` — biological pathways (genes, proteins, interactions)
- `transport.1` — transportation networks (routes, carriers, schedules)
- `supply.1` — supply chains (components, suppliers, shipments)
- `legal.1` — legal documents (statutes, cases, citations)

**How to contribute a schema:**
1. Create `haskell/schema/your_domain/domain.angle`
2. Define predicates following the pattern in `haskell/schema/src.angle`
3. Write a corresponding indexer in `haskell/src/Glean/Indexer/`
4. Add tests
5. Open a pull request

---

### 4. Language Indexers (Community)

**What:** Write indexers for languages other than Haskell.

**Current indexer:** `haskell/src/Glean/Indexer/HIE.hs` reads GHC HIE files.

**To add a new language indexer:**
1. Find the language's semantic data source
   (e.g. rust-analyzer for Rust, LSP data for others)
2. Create `haskell/src/Glean/Indexer/YourLanguage.hs`
3. Read the data source and convert to `DefinitionFact`, `ReferenceFact`, etc.
4. Add a CLI subcommand in `haskell/app/Main.hs`
5. Add tests

The fact types in `haskell/src/Glean/Indexer/Types.hs` are language-agnostic
and work for any language.

---

### 5. Pure-Rust Storage Backend (Windows-Friendly)

**What:** Add a storage backend using a pure-Rust key-value store
(Fjall, Sled, or Redb) that requires no C++ compilation.

**Why it matters:** `rust-rocksdb` requires MSVC and LLVM on Windows.
A pure-Rust backend would make glean-hs truly zero-C++ on all platforms.

**Candidates:**
- [Fjall](https://crates.io/crates/fjall) — LSM-tree, RocksDB alternative
- [Redb](https://crates.io/crates/redb) — LMDB-inspired, pure Rust

---

## Running Tests

```bash
# Rust tests (127 tests)
cargo test

# Haskell tests (7 tests)
cabal test

# Integration test (index glean-hs itself)
cabal build --ghc-options="-fwrite-ide-info -hiedir=.hie"
cabal run glean-hs -- index --hie-dir .hie --db /tmp/glean-test
cabal run glean-hs -- query --db /tmp/glean-test "checkError"
cabal run glean-hs -- stats --db /tmp/glean-test
```

---

## Code Style

- Rust: `cargo fmt` before committing
- Haskell: follow existing module structure
- Commits: use conventional commit messages
  (`feat:`, `fix:`, `docs:`, `chore:`)

---

## Questions?

Open an issue at [github.com/XF-Interchange/glean-hs](https://github.com/XF-Interchange/glean-hs).
