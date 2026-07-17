# glean-hs

> Docker-free Haskell code indexing — built by [XF-Interchange LLC](https://xf-interchange.ai)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

---

## What is glean-hs?

**glean-hs** lets you index a Haskell codebase and ask questions about it:

```bash
# Where is this function defined?
glean-hs query --db ./mydb "validateCDTCode"

# What calls this function?
glean-hs query --db ./mydb "ref:validateCDTCode"

# What's in this module?
glean-hs query --db ./mydb "mod:SeidoClaims.Validation"
```

Think of it like indexing a database. Without an index, finding where a
function is defined means searching through every source file manually —
slow for large projects. glean-hs indexes your code once and answers any
question about it instantly, just like a database query.

---

## The Problem it Solves

[Meta Glean](https://github.com/facebookincubant/glean) is a powerful code
indexing system that supports Haskell. It enables go-to-definition across
modules, find-all-references, dead code detection, and dependency analysis.

**The barrier:** Glean requires Docker because its C++ dependencies
(`folly`, `RocksDB`, `fbthrift`) are difficult to build natively on
macOS and Windows.

**glean-hs eliminates the Docker requirement** by reimplementing those C++
dependencies in Rust:

```
C++ dependency  →  Rust equivalent
────────────────────────────────────
RocksDB         →  rust-rocksdb
folly utilities →  Rust standard library + crates
fbthrift        →  avoided entirely
```

---

## Quick Start

### Step 1 — Install the prerequisites

You need two tools. Both install with a single command.

**Rust** (the language our storage layer is written in):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

**GHC + Cabal** (the Haskell compiler and build tool):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://get-ghcup.haskell.org | sh
ghcup install ghc 9.12.2
ghcup set ghc 9.12.2
```

> **Why version 9.12.2 specifically?**
> glean-hs uses GHC internal APIs for reading HIE files
> (`GHC.Iface.Ext.*`). These APIs change between major GHC versions.
> GHC 9.12.2 is the version glean-hs has been tested and built against.
> Using a different version may cause compilation errors.

### Step 2 — Build glean-hs

```bash
git clone https://github.com/XF-Interchange/glean-hs
cd glean-hs
cargo build --release   # builds the Rust storage layer (~4 minutes first time)
cabal build             # builds the Haskell layer (~2 minutes first time)
```

### Step 3 — Index a Haskell project

First, tell GHC to generate HIE files (the semantic data glean-hs reads):

```bash
# In your Haskell project directory:
cabal build --ghc-options="-fwrite-ide-info -hiedir=.hie"
```

Then index it:

```bash
cabal run glean-hs -- index --hie-dir .hie --db ./mydb --verbose
```

### Step 4 — Query it

```bash
# Find where a function is defined
cabal run glean-hs -- query --db ./mydb "myFunction"

# Find all references to a function
cabal run glean-hs -- query --db ./mydb "ref:myFunction"

# Show glean-hs database statistics
cabal run glean-hs -- stats --db ./mydb
```

---

## Key Concepts

### HIE Files — What They Are

When GHC compiles your Haskell code, it understands everything about it —
what every name means, what type every expression has, and where every
function is defined.

Normally GHC uses this knowledge just to compile your code and then
discards it. With the `-fwrite-ide-info` flag, GHC saves that knowledge
to `.hie` files in your project directory.

glean-hs reads those `.hie` files. It doesn't parse your Haskell source
directly — GHC already did the hard work. glean-hs just reads what GHC
understood.

You don't need to understand the format of these files. Just tell GHC to
generate them, and glean-hs handles the rest.

### Facts and Predicates — Plain English

A **fact** is one piece of information about your code. For example:

> "The function `validateCDTCode` is defined in `SeidoClaims.Validation` at line 42"

That's one fact.

A **predicate** is the category a fact belongs to — think of it like a
table name in a database:

| Predicate | What it stores |
|-----------|----------------|
| `src.Definition` | A name defined at a location |
| `src.Reference` | A name used at a location |
| `src.Module` | A Haskell module and its source file |
| `src.Import` | A module import relationship |

glean-hs stores thousands of facts about your code. When you run a query,
it searches those facts and returns the ones that match.

### Schema

The schema defines what kinds of facts exist. glean-hs uses `src.1` —
a minimal, general-purpose schema the community can extend for their
own domains. See `haskell/schema/src.angle` and `haskell/schema/SCHEMA.md`.

---

## Building on Different Operating Systems

### macOS ✅ Tested

Works out of the box. No extra steps.

```bash
cargo build --release
cabal build
```

**About the linker warning:**

You may see this during `cargo build`:
```
ld: warning: object file was built for newer macOS version (26.x) than being linked (10.12)
```

**What causes it:** `librocksdb-sys` is compiled by your Rust toolchain
targeting your current macOS version. But Cargo's default minimum
deployment target is macOS 10.12 (2016) — a very old version set to
maximize compatibility. The linker sees the mismatch and warns you.

**Why it's harmless:** The binary links and runs correctly on your machine.
It simply wouldn't run on actual macOS 10.12 — which nobody uses anymore.

**To suppress it permanently** (add to your `~/.zshrc`):
```bash
export MACOSX_DEPLOYMENT_TARGET=14.0
```
This setting persists across macOS updates — you only need to set it once.

### Linux ✅ Should work

The same build steps apply. Community testing welcome — please open an
issue if you encounter problems.

### Windows ⚠️ Extra steps required

`rust-rocksdb` requires C++ compilation on Windows:

1. Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/)
   — select the **"Desktop development with C++"** workload
2. Install [LLVM](https://releases.llvm.org/) and check
   **"Add LLVM to the system PATH"** during installation
3. Then build normally:

```bash
cargo build --release
cabal build
```

**Note:** The first `cargo build --release` will be slow (several minutes)
because it compiles RocksDB's C++ source from scratch. Subsequent builds
are fast due to caching.

---

## GHC Version Compatibility

glean-hs is built and tested with **GHC 9.12.2**.

The HIE indexer uses GHC internal APIs which may change between major
GHC versions. If you upgrade GHC, you may need to update the imports
in `haskell/src/Glean/Indexer/HIE.hs`. The `hie-compat` library
(already a dependency) abstracts some of these differences.

---

## CLI Reference

```
glean-hs index  --hie-dir DIR  --db PATH [--verbose] [--max-files N]
glean-hs query  --db PATH  QUERY
glean-hs stats  --db PATH
```

### Query syntax

| Query | Returns |
|-------|---------|
| `"functionName"` | Definitions of that name |
| `"ref:functionName"` | References to that name |
| `"mod:Module.Name"` | All facts in that module |

---

## Beyond Code Indexing

glean-hs is not limited to Haskell code. Any structured knowledge domain
can be expressed as Glean facts and queried with Angle:

- **Biological pathways** — genes, proteins, interactions
- **Transportation networks** — routes, carriers, schedules
- **Supply chains** — components, suppliers, shipments
- **Medical ontologies** — conditions, procedures, anatomy

See `haskell/schema/src.angle` for the base schema. Extend it for your domain.

*"It's all in the schemas."*

---

## Known Limitations

- **Query performance:** Currently O(n) — scans all stored batches.
  Sufficient for small projects. Composite key storage planned for
  large codebases (50K+ facts).

- **Import indexing:** Import facts are stubbed — coming in a future release.

- **Angle query language:** Full Angle integration is planned. Current
  queries use direct storage access.

- **cabal path warning:** A known warning about `target/release` path
  during `cabal build` is harmless and does not affect functionality.

---

## Project Structure

```
glean-hs/
├── src/
│   ├── rts/           # Rust runtime substrate
│   │   ├── id.rs      # Id, Pid types
│   │   ├── fact.rs    # Fact, Clause, FactRef
│   │   ├── factset.rs # Two-index FactSet
│   │   ├── bytecode/  # VM (opcode, frame, syscall, vm)
│   │   └── inventory.rs
│   └── storage/
│       └── rocksdb.rs # C-ABI functions for Haskell FFI
├── haskell/
│   ├── src/Glean/
│   │   ├── FFI.hs     # Rust FFI bindings
│   │   ├── Storage.hs # Storage typeclass
│   │   ├── RocksDB.hs # RocksDB implementation
│   │   ├── Query.hs   # Direct query layer
│   │   └── Indexer/
│   │       ├── HIE.hs     # GHC HIE file reader
│   │       └── Types.hs   # Fact types
│   ├── app/
│   │   └── Main.hs    # CLI
│   └── schema/
│       ├── src.angle  # Schema definition (DRAFT)
│       └── SCHEMA.md  # Schema documentation
└── glean-hs.cabal
```

---

## Relationship to Meta Glean

glean-hs is inspired by and compatible with
[Meta Glean](https://github.com/facebookincubator/glean).

Meta Glean is a production-grade, battle-tested system running at massive
scale inside Meta. If you are on Linux and comfortable with Docker,
Meta Glean is worth evaluating directly.

glean-hs solves a specific problem Meta Glean has: building natively on
macOS and Windows without Docker. It is not a replacement for Meta Glean —
it is a portable on-ramp to the same ecosystem.

Schema compatibility with Meta Glean's `src.1` is a goal for future releases.

---

## Contributing

glean-hs is open source under the MIT license.
Contributions welcome — especially:

- Testing on Linux and Windows
- Domain schemas (biology, transport, supply chain, medical)
- LMDB backend (30-40% faster per Meta Glean benchmarks)
- Pure-Rust storage backend (Fjall/Redb) for Windows
- Angle query language integration

Open an issue or pull request at
[github.com/XF-Interchange/glean-hs](https://github.com/XF-Interchange/glean-hs).

---

## License

MIT — see [LICENSE](LICENSE)

Built by [XF-Interchange LLC](https://xf-interchange.ai)
