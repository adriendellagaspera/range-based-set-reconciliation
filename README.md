# Range-Based Set Reconciliation

Rust implementations of the data structures and protocol primitives behind range-based set reconciliation.

## Origins and attribution

The concepts implemented here come from the research literature; this repository does **not**
claim to originate RBSR or RSOS.

- **Range-Based Set Reconciliation (RBSR)** follows Aljoscha Meyer's *Range-Based Set
  Reconciliation*, published at IEEE SRDS 2023 (pp. 59–69,
  DOI: [10.1109/SRDS60354.2023.00016](https://doi.org/10.1109/SRDS60354.2023.00016);
  preprint: [arXiv:2212.13567](https://arxiv.org/abs/2212.13567)).
- **Range-Summarizable Order-Statistics Store (RSOS)** follows the abstraction formalized by
  Elvio G. Amparore in *Range-Based Set Reconciliation via Range-Summarizable Order-Statistics
  Stores* (2026, [arXiv:2603.19820](https://arxiv.org/abs/2603.19820)).

The Rust crates in this repository are independent implementations of those ideas. See
[REFERENCES.md](REFERENCES.md) for full references and the mapping from the papers to the crates.

This repository contains two crates:

- `rsos`: Range-Summarizable Order-Statistics Store primitives and `FingerprintTreeMap`.
- `rbsr`: transport-independent Range-Based Set Reconciliation over any compatible RSOS backend.

The protocol crate depends on the store abstraction, but neither crate depends on an async runtime, transport, wire codec, wall clock, persistence layer, or application-specific replication runtime.

```text
rsos
  ↑
rbsr
  ↑
application/runtime
```

The code was extracted, with history preserved, from [`reconcile-rs`](https://github.com/adriendellagaspera/reconcile-rs). Product/runtime concerns such as `ReplicatedMap`, membership, tombstone stability, persistence, discovery, authentication, and UDP transport remain there.

## Crates

### `rsos`

```sh
cargo add rsos
```

Provides the `Rsos` contract, range aggregates, canonical encoding, fingerprints, and the persistent `FingerprintTreeMap` implementation.

### `rbsr`

```sh
cargo add rbsr
```

Provides `initial_ranges`, `protocol_round`, refinement policies, and the read-only `RsosView` contract used by the reconciliation driver.

## Benchmarks

Benchmarks live with the crate whose behavior they measure:

- `cargo bench -p rsos --bench contention` measures `FingerprintTreeMap` write contention against a `BTreeMap` control behind the same lock.
- `cargo bench -p rbsr --bench history_independence` verifies that superseded mutation history does not change the RBSR trace once current states are identical, and measures reconciliation CPU cost.

These are implementation benchmarks for the shipped RSOS/RBSR crates. Comparative algorithm research, transport projections, and Pareto-frontier experiments belong in `rbsr-research`; `ReplicatedMap`, membership, persistence, and network-runtime benchmarks belong in `reconcile-rs`.

## Development


The workspace targets Rust 1.85+.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

Licensed under either MIT or Apache-2.0, at your option.
