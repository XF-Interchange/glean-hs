# glean-hs

> Docker-free Haskell code indexing — a Rust-native reimplementation of Meta Glean's dependency stack for the Haskell ecosystem.

---

## The Problem

[Meta Glean](https://github.com/facebookincubant/glean) is a powerful code indexing system with exceptional support for Haskell. It enables cross-module navigation, semantic search, dead code detection, and dependency analysis across large codebases.

There is one significant barrier to adoption: **Glean requires Docker**.

This requirement exists because Glean's C++ dependency chain — `folly`, `RocksDB`, and `Thrift` — is difficult to build natively on macOS and Windows. The Docker container pre-packages these dependencies in a Linux environment, bypassing the build complexity entirely.

For Haskell developers who prefer native tooling, this is a meaningful friction point.

---

## The Vision

**glean-hs** aims to eliminate the Docker requirement by reimplementing Glean's C++ dependency stack in Rust, using existing, well-maintained crates:

```
C++ dependency     →  Rust equivalent
──────────────────────────────────────
RocksDB            →  rust-rocksdb
Thrift             →  rust Thrift implementations
folly utilities    →  Rust standard library + crates
```

The result: a native Haskell code indexer that runs on macOS, Linux, and Windows — no Docker required.

---

## Why Rust

Rust is the natural choice for this reimplementation:

- **Memory safety without GC** — same guarantee as C++, without the footguns
- **Mature ecosystem** — `rust-rocksdb`, async runtimes, FFI tooling all exist
- **Haskell FFI** — Rust exposes a clean C ABI that Haskell can call via `ccall`
- **Cross-platform** — builds natively on macOS, Linux, and Windows without containers
- **Philosophy alignment** — precision infrastructure deserves a precise implementation language

---

## Why This Matters for Haskell

Large Haskell codebases are genuinely difficult to navigate without tooling. As projects grow beyond 20-30 modules, questions like these become expensive to answer manually:

- What calls `validateCDTCode`?
- Which modules import `Types.LLM`?
- What is the full dependency graph of `Pipeline.AutoCorrect`?
- Is there dead code after this refactor?

Haskell Language Server (HLS) addresses some of this, but Glean's indexed, queryable approach is fundamentally more powerful for large-scale analysis.

The missing piece has always been native installation. **glean-hs** fills that gap.

---

## Intended Use Cases

**1. Development tooling (primary)**
Navigate large Haskell codebases without Docker. Cross-module semantic search, call graph analysis, dead code detection — natively on your development machine.

**2. CI/CD integration**
Run code analysis in CI pipelines without managing Docker images or container runtimes.

**3. Domain intelligence layer (future)**
Index domain-specific document corpora (e.g., payer companion guides, regulatory documentation) alongside code, enabling unified search across code and knowledge.

---

## Project Status

**Early stage — vision and architecture phase.**

This project is being built in parallel with other XF-Interchange infrastructure work. Contributions, discussion, and interest are welcome.

```
Phase 1:  Architecture design
          Rust crate selection and evaluation
          Haskell FFI boundary design

Phase 2:  Rust core implementation
          RocksDB indexing layer
          Thrift serialization layer

Phase 3:  Haskell bindings
          FFI wrappers
          Query API

Phase 4:  Native installer
          macOS, Linux, Windows
          No Docker required

Phase 5:  Glean compatibility layer
          Drop-in replacement for existing
          Glean Haskell workflows
```

---

## Relationship to Meta Glean

glean-hs is not a fork of Meta Glean. It is an independent reimplementation of the infrastructure layer that Glean depends on, with the goal of enabling native installation of Glean-compatible tooling for Haskell developers.

Meta Glean's query language, schema design, and indexing approach are referenced as prior art and inspiration. All implementation in this repository is original.

---

## Contributing

This project is in its early stages. If you are interested in:

- Rust systems programming
- Haskell FFI
- Developer tooling for functional languages
- Eliminating unnecessary Docker dependencies from the Haskell ecosystem

...your contributions are welcome. Open an issue to discuss ideas before submitting a PR.

---

## License

MIT — see [LICENSE](LICENSE)

---

## About XF-Interchange LLC

XF-Interchange LLC identifies critical gaps in industries where data integrity is not optional — and builds the precision infrastructure to fill them.

[xf-interchange.ai](https://xf-interchange.ai)
