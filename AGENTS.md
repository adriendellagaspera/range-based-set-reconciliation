# AGENTS.md

## Scope

This repository owns set-reconciliation algorithms, data structures, comparators, experiments, formal models, reproducible workloads, and literature. `reconcile-rs` owns the distributed/application runtime.

## Ownership boundaries

- `rsos/`: stable, publishable RSOS crate.
- `rbsr/`: stable, publishable transport-independent RBSR crate.
- `comparators/`: reproductions or adapters of existing algorithms from the literature. Preserve attribution and provenance.
- `constructions/`: genuinely new mechanisms proposed in this project. Do not move literature implementations here.
- `experiments/`: harnesses, simulations, instrumentation, transport projections, reporting, and measurement-only support.
- `models/`: formal problem definitions, cost/interaction/locality models, asymptotic analysis, lower-bound questions, conjectures, and adversarial families.
- `proofs/`: only developed proof/formalization artifacts; do not create it for notes.
- `workloads/`: reproducible inputs/manifests/fixtures, not large generated outputs.
- `literature/`: external work, terminology, evidence status, bibliography, and provenance.

A project may span several surfaces, but each artifact lives according to its function.

## Stable Cargo boundary

The root Cargo workspace contains only `rsos` and `rbsr`. Both must compile and test independently, preserve their MSRV, remain publishable on crates.io, and stay free of experimental/runtime dependencies. Criterion is allowed only as a dev dependency.

Experiments and comparators use separate manifests/workspaces and may depend on `../../rsos` and `../../rbsr` by path. A repository commit is the common reproducibility pin.

## Evidence and attribution

Do not present an external algorithm as original work. Literature claims must distinguish theorem/proved bound, conjecture, empirical paper result, implementation artifact, local reproduction, and local hypothesis. Prefer primary sources for algorithmic guarantees.

## CI

Stable crate CI and experimental CI are independent. Large statistical sweeps never run in CI; CI may compile them and run deterministic small fixtures.

## Migration provenance

The private `rbsr-research` repository remains the provenance archive for its original Git history. The public import records exact source SHAs because the available GitHub integration cannot attach private Git objects directly to this public repository without rewriting/copying them.
