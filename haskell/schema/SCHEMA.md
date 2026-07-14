# glean-hs Schema — Version 1 DRAFT

Status: DRAFT — not yet frozen.
Freeze after integration testing against real HIE files.

## Predicates

- **File** — source file path
- **Pos** — line/col position (1-indexed)
- **Span** — source location range
- **Module** — Haskell module + file
- **Definition** — name defined at location
- **Reference** — name used at location
- **Import** — module import relationship

## Example Queries

```angle
# Where is validateCDTCode defined?
src.Definition { name = "validateCDTCode" }

# What references processRemittance?
src.Reference { name = "processRemittance" }

# What does SeidoClaims.Validation import?
src.Import { from = "SeidoClaims.Validation" }
```

## Extension

Extend src.1 for your own language or domain:

```angle
schema myproject.1 : src.1 {
  predicate MyPredicate : { def : src.Definition, ... }
}
```

## Versioning

src.1 will be frozen after integration testing.
New predicates go in src.2 — never breaking src.1.
