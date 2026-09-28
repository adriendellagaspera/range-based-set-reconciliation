# rsos

Range-Summarizable Order-Statistics Store primitives and `FingerprintTreeMap`.

## Origin

**RSOS is not a concept introduced by this crate.** The Range-Summarizable Order-Statistics Store
abstraction is formalized by Elvio G. Amparore in:

> Elvio G. Amparore, *Range-Based Set Reconciliation via Range-Summarizable Order-Statistics
> Stores*, 2026, [arXiv:2603.19820](https://arxiv.org/abs/2603.19820).

This crate is an independent Rust implementation of that storage abstraction. In particular,
`Rsos` represents the range-summary/order-statistics contract and `FingerprintTreeMap` is this
crate's in-memory realization.

The abstraction is motivated by the Range-Based Set Reconciliation protocol introduced by
Aljoscha Meyer at IEEE SRDS 2023; see the repository
[references](../REFERENCES.md).

See the crate rustdoc for the `Rsos` contract, range aggregates, canonical encoding,
fingerprints, and map API.

Licensed under MIT OR Apache-2.0.
