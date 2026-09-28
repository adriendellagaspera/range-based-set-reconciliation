# Research references

The terminology and algorithms implemented by this repository come from the research literature.
The repository provides Rust implementations; it does not claim authorship of the underlying RBSR
protocol or RSOS abstraction.

## Range-Based Set Reconciliation (RBSR)

Aljoscha Meyer. **Range-Based Set Reconciliation.**  
42nd International Symposium on Reliable Distributed Systems (SRDS 2023), pp. 59–69. IEEE, 2023.  
DOI: [10.1109/SRDS60354.2023.00016](https://doi.org/10.1109/SRDS60354.2023.00016)  
Preprint: [arXiv:2212.13567](https://arxiv.org/abs/2212.13567)

The `rbsr` crate implements the transport-independent reconciliation mechanism described by this
work.

## Range-Summarizable Order-Statistics Stores (RSOS)

Elvio G. Amparore. **Range-Based Set Reconciliation via Range-Summarizable Order-Statistics
Stores.** 2026.  
Preprint: [arXiv:2603.19820](https://arxiv.org/abs/2603.19820)

The `rsos` crate implements the RSOS abstraction formalized in this work, with
`FingerprintTreeMap` as an in-memory realization. The `rbsr` crate uses an RSOS-compatible
read-only view as its storage backend contract.

## Relationship

Meyer's work defines and analyzes Range-Based Set Reconciliation. Amparore's work subsequently
formalizes the storage-side RSOS abstraction that supports efficient RBSR operations such as range
summaries, rank/select navigation, and enumeration.

Implementation details, APIs, tests, and engineering choices in this repository are specific to
this Rust implementation unless explicitly attributed otherwise.
