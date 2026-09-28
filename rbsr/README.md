# rbsr

Transport-independent Range-Based Set Reconciliation over an RSOS backend.

## Origin

**RBSR is not a protocol introduced by this crate.** This implementation follows:

> Aljoscha Meyer, *Range-Based Set Reconciliation*, 42nd IEEE International Symposium on
> Reliable Distributed Systems (SRDS), 2023, pp. 59–69,
> DOI: [10.1109/SRDS60354.2023.00016](https://doi.org/10.1109/SRDS60354.2023.00016),
> [arXiv:2212.13567](https://arxiv.org/abs/2212.13567).

The `RsosView` backend used by this crate follows the Range-Summarizable Order-Statistics Store
abstraction formalized by Elvio G. Amparore in *Range-Based Set Reconciliation via
Range-Summarizable Order-Statistics Stores* (2026,
[arXiv:2603.19820](https://arxiv.org/abs/2603.19820)).

This crate is an independent Rust implementation of those ideas; it does not claim authorship of
the underlying RBSR protocol or RSOS abstraction. See the repository
[references](../REFERENCES.md) for the full citations.

See the crate rustdoc for the protocol driver, range messages, and refinement policies.

Licensed under MIT OR Apache-2.0.
