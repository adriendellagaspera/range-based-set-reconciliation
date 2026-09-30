# Set Reconciliation

Exact and practical set reconciliation algorithms, data structures, experiments, and theory.

This repository is the canonical home for set-reconciliation work maintained here. It is broader than Range-Based Set Reconciliation: RBSR and RSOS are shipping implementations alongside reproducible comparators, experiments, new constructions, formal models, workloads, and a versioned literature survey.

## Repository surfaces

1. **Shipping implementations** — `rsos/` and `rbsr/` are the stable, published Rust crates. They form the root Cargo workspace and keep their independent MSRV, packaging, and release lifecycle.
2. **Comparators & experiments** — `comparators/` contains reproductions/adapters for algorithms from the literature; `experiments/` contains measurement harnesses, simulations, transport projections, instrumentation, and reporting. These are unpublished and isolated from the stable workspace.
3. **New constructions & formal models** — new mechanisms belong in `constructions/`; formal problem definitions, cost models, conjectures, and lower-bound questions belong in `models/`. Directories are created only when a real artifact exists.
4. **Literature** — `literature/` holds the versioned survey, bibliography, terminology map, evidence catalog, and provenance notes. External algorithms are attributed to their authors; this repository does not claim authorship of work from the literature.

Reproducible benchmark inputs live in `workloads/`. Generated large outputs do not.

## Stable crates

- `rsos` — Range-Summarizable Order-Statistics Store primitives and `FingerprintTreeMap`.
- `rbsr` — transport-independent Range-Based Set Reconciliation over an RSOS-compatible backend.

```text
rsos
  ↑
rbsr
  ↑
application/runtime
```

The stable crates do not depend on an async runtime, transport, wire codec, `reconcile-rs`, or experimental code. Intrinsic implementation benchmarks remain with those crates.

`reconcile-rs` is a separate downstream runtime/consumer. Runtime concerns such as membership, persistence, authentication, transport, and application-level replication remain there.

## Attribution

RBSR follows Aljoscha Meyer's *Range-Based Set Reconciliation* (SRDS 2023, DOI 10.1109/SRDS60354.2023.00016; arXiv:2212.13567). RSOS follows Elvio G. Amparore's *Range-Based Set Reconciliation via Range-Summarizable Order-Statistics Stores* (2026, arXiv:2603.19820).

See `literature/` for the broader survey and evidence taxonomy.

## Development

Stable workspace:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
cargo package --workspace
```

Experimental/comparator checks are deliberately separate; see `experiments/` and CI.

Licensed under either MIT or Apache-2.0, at your option.
