# Range-Based Set Reconciliation

Rust implementations of the data structures and protocol primitives behind range-based set reconciliation.

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

## Development

The workspace targets Rust 1.85+.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

Licensed under either MIT or Apache-2.0, at your option.
