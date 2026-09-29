# Literature

This directory is the versioned literature surface for set reconciliation.

`SURVEY.md` is the imported survey/bibliography, including the September 2026 SOTA refresh. Claims should be tagged or written so their evidence level is explicit:

- **theorem / proved bound** — a proved statement in a cited source;
- **conjecture** — an unproved statement explicitly presented as such;
- **empirical paper result** — a measurement reported by external authors;
- **implementation artifact** — code or a reproducible artifact released by external authors;
- **our reproduction** — a result reproduced by this repository's comparator/experiment code;
- **our hypothesis** — a project hypothesis not established by cited evidence.

The survey covers RBSR/RSOS, Merkle/MST/G-tree/prolly structures, characteristic-polynomial reconciliation/CPISync, IBLT variants (including MET-IBLT, RIBLT, self-sizing IBLT, ADAPTIVEIBLT, Stuffed IBLTs), Minisketch/PinSketch, PBS, CertainSync, XYZ-Sketch, Graphene/Difference Digest where relevant, and related sparse-recovery, streaming, and communication-complexity work.

External algorithms remain attributed to their original authors. Reproduction code belongs under `comparators/`; comparative measurements belong under `experiments/`.
