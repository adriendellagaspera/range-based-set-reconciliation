<!-- Imported from adriendellagaspera/rbsr-research PR #96, head eb84a667f804e56f04d350311ce314a45895bc45. Unqualified historical issue references in this document refer to the archived source tracker unless repointed. -->

# `rbsr-research`

> Private research companion to the standalone RSOS/RBSR implementation and the `reconcile`
> runtime, not a published library. It contains comparative benchmarks, transport projections,
> oracle probes, experimental policies, and literature material.
>
> | Topic | Canonical location |
> |---|---|
> | RSOS/RBSR implementation and intrinsic benchmarks | [`range-based-set-reconciliation`](https://github.com/adriendellagaspera/range-based-set-reconciliation) |
> | ReplicatedMap/runtime implementation and benchmarks | [`reconcile-rs`](https://github.com/adriendellagaspera/reconcile-rs) |
> | LWW/HLC primitive | [`lww-register`](https://github.com/adriendellagaspera/lww-register) |
> | Comparative research, transport models, Pareto experiments | this repository |
> | Literature material | survey panels, glossary, and bibliography below |
>
> Implementation facts belong to the repository that ships the code. This repository owns
> cross-algorithm comparisons, experimental variants, transport projections, and research claims.
> Runtime crates may appear only as dev-only harness dependencies for explicit transport experiments
> (for example `actual_*` and selective-reliability tests); they are not part of the research library
> or its comparison core.

---

## 1. Survey panels

Three panels that summarize research areas rather than restating implementation details.

### 1.1 Merkle / anti-entropy structures

Important panel nuance: **FingerprintTreeMap does NOT belong to the Merkle Search Tree (MST) / prolly-tree
family**, and that is a point in its favor. MST (Auvolat & Taïani, SRDS 2019) and prolly-trees
(Dolt/Noms) *need* **insertion-order independence** because they diff by comparing the hashes of the
tree's **internal nodes**. FingerprintTreeMap, by contrast, diffs **value-defined ranges**: the cumulative
256-bit additive fingerprint (per-element BLAKE3, combined mod 2²⁵⁶) over `[a,b)` is identical on two
peers iff the *content* of the range is identical, **regardless of each one's B-tree shape**.
FingerprintTreeMap therefore obtains the convergence guarantee that MST/prolly pay for with
history-independence, **without paying for it** — and, since addition-with-carry is not GF(2)-linear
the way XOR is, also escapes the MST "leading-zeros" attack on firmer ground than a linear combiner
would. The B-tree's order-dependence and history-dependence are not defects for this design.

**The comparison also cuts both ways.** The family this panel distinguishes FingerprintTreeMap
*from* has since been unified — and made cheap — by RBSR's own author: **G-trees** (Meyer 2024, §3.4)
subsume zip-trees, zip-zip-trees, skip-trees, dense skip-trees, MST **and** prolly-trees as one
randomized, history-independent family whose `k`-ary members carry no extra conceptual overhead over
the binary ones, and zip-tree-class per-node metadata rather than a rolling hash. The structural half
of the paragraph above is untouched — value-defined ranges are still not internal nodes — but its
*cost* half was written against prolly-trees' cascading rechunking, and that reference point has
moved twice since (Rawat et al., then this). The cell where both properties hold is **empty**: an HI
tree carrying a composable 256-bit summary *and* subtree counts would be an RSOS and node-diffable at
once, the only way to compare the two diff modes with the store held constant — [#29](https://github.com/adriendellagaspera/rbsr-research/issues/29).

### 1.2 Consistency and conflict resolution

- **Physical-clock LWW**: a documented anti-pattern (Jepsen/Kingsbury "The trouble with
  timestamps"; real NTP incidents). The "winner" is the node with the most-advanced clock, not the
  causally latest write → silent lost update.
- **Minimal SOTA fix**: **Hybrid Logical Clocks (HLC, Kulkarni 2014)** — 64-bit drop-in, monotonic,
  respects causality, divergence bounded by ε; adopted by CockroachDB and MongoDB.
- **Tie-break**: must be a **deterministic total order** (e.g. `(HLC, node_id)`). The current `keep
  local on equal` is non-convergent.
- **Tombstone GC**: the safe criterion is **causal stability** (acknowledgment by all replicas), not
  a wall-clock timer. Even Cassandra's `gc_grace_seconds` (default **10 days**) is safe *only on the
  condition* that a complete repair covers the window — i.e. no fixed duration, short or long, is
  sufficient on its own (ScyllaDB makes this explicit with repair-based GC). The pre-fix **60 s**
  wall-clock purge could not honor that precondition; GC is now gated on causal stability (F4/#109).

### 1.3 The design space: RBSR variants over an RSOS, and alternatives to the RSOS

This survey maps alternatives to the RSOS and the protocol choices that remain open. Active issues own detailed claims, methods, and measurements.

**Fix the RSOS, vary the protocol.** Algorithm 1's degrees of freedom, and which are swept:

| Degree of freedom | State |
|---|---|
| split arity `b` | swept; `b` = 16, a per-node choice rather than a wire contract (upstream #257) |
| enumeration threshold `t` | swept; no `t` beats not having one by default, conditional on value size and RTT (upstream #468/#315) |
| width from a divergence signal | closed, and **not built** — the count is the only admissible signal, and it reads zero exactly where an LWW update lands ([#12](https://github.com/adriendellagaspera/rbsr-research/issues/12)) |
| **split *position*** — exact rank-cut, jittered, uniform, skewed | **open.** Meyer §5.1 makes Def. 3.8's exactness non-load-bearing, Vogel et al. put maximal throughput at any arity *given the right split distribution*, and both founding contention-tree analyses assume fair coins ([#25](https://github.com/adriendellagaspera/rbsr-research/issues/25)) |
| what a SPLIT transmits — sibling subtraction | measurable, and group-only, so it prices what the group buys ([#20](https://github.com/adriendellagaspera/rbsr-research/issues/20)) |
| **round budget** — width from a byte ceiling rather than from `m` | **open, and deployed elsewhere**: Negentropy's `frameSizeLimit` bounds every frame and defers the rest, while the default here advertises a round over the datagram ceiling ([#22](https://github.com/adriendellagaspera/rbsr-research/issues/22), [#26](https://github.com/adriendellagaspera/rbsr-research/issues/26)) |
| **where refinement starts** — the outer range, or a prior over the divergence | **open, and unclaimed in either dialect**: the one lever that shortens the chain without paying for rounds in bytes ([#27](https://github.com/adriendellagaspera/rbsr-research/issues/27)) |
| parties — two, or N | half-closed: the retry claim is refuted by derivation, the constructive N-party family untouched (upstream #354, [#30](https://github.com/adriendellagaspera/rbsr-research/issues/30)) |
| dimension `δ` | no-go on paper; the obstruction is the summary, not the dimension, and the protocol side transports verbatim (upstream #360) |

**Fix RBSR, vary what answers the queries.** Def. 3.9's five queries are an interface; an
aggregate-augmented B-tree is one implementation of it:

| Instead of the RSOS | What it drops, what it buys | State |
|---|---|---|
| HI tree + conventional hash over clamped subtrees | drops the composable monoid, needs clamping-invariance; realized by G-trees | the standing counter-argument (the alternative discussed above), and the (HI × composable) cell is empty ([#29](https://github.com/adriendellagaspera/rbsr-research/issues/29)) |
| AB-tree's concurrent aggregate maintenance, or a per-session snapshot | drops the root-path write every insert pays; the snapshot drops it from the write path entirely, at a staleness cost | **open**, and the recorded verdict rests on a reading under review ([#28](https://github.com/adriendellagaspera/rbsr-research/issues/28), upstream #359) |
| a persistent COW / content-addressed store (LMDB, AELMDB, prolly) | buys structural sharing, then cross-version identity — priced separately | upstream #271, #188 |
| an aggregate-augmented LSM | write-optimized, aggregates falling out of compaction; but a sound SKIP needs the summary of the **merged** view | unexplored in either dialect, no issue |
| a sketch instead of a store (RIBLT, CertainSync, MET-IBLT, PBS) | drops the order: one exchange, `Ω(n)` encoder, no partial-range or prefix sync | hybrid tracked ([#13](https://github.com/adriendellagaspera/rbsr-research/issues/13), [#5](https://github.com/adriendellagaspera/rbsr-research/issues/5), [#6](https://github.com/adriendellagaspera/rbsr-research/issues/6)) |
| approximate or similarity-based (ART; LSH + IBLT) | drops exactness; LSH prices *distance between elements* rather than a count of differing keys | neither cited by any RBSR or PSR work ([#31](https://github.com/adriendellagaspera/rbsr-research/issues/31)) |
| a vector-commitment summary | buys a *proof* that a SKIP is honest | open, no issue: the union bound covers accidental collisions, never a lying peer |

Two readings the map supports and no single source states. **Latency has no lever left inside RBSR** —
every remaining one trades bytes for rounds — so the single-shot hybrid and a prior over the
divergence are the only two candidates, and they are also the two least-explored rows. And
**exactness may not be uniform across an RSOS's two components**: the fingerprint carries the SKIP's
soundness while part of what the count carries is balance, which is the separation [#28](https://github.com/adriendellagaspera/rbsr-research/issues/28) tests
rather than asserts.

---

## 2. Glossary

> Lists **(a)** the competing structures and algorithms cited, **(b)** the acronyms and concepts of
> distributed systems, cryptography, networking and complexity, and **(c)** the Rust tooling — all
> **implementation-agnostic**. The repository's own identifiers, types and constants are intentionally
> not catalogued here (see [§2.1](#g91)). `Fxx` references denote the original audit findings, whose
> resolution record lives in [`ARCHITECTURE.md`](https://github.com/adriendellagaspera/reconcile-rs/blob/main/ARCHITECTURE.md) §8.

<a id="g91"></a>
### 2.1 — Repository identifiers

> **Implementation-agnostic by design.** This literature survey does not catalogue the
> repository's own types, methods and constants. For the code surface, see the crate's API
> documentation (`cargo doc`); for the module map and the target design, see
> [`ARCHITECTURE.md`](https://github.com/adriendellagaspera/reconcile-rs/blob/main/ARCHITECTURE.md) (§2.1); for the audit findings and their resolution
> record, see [`ARCHITECTURE.md`](https://github.com/adriendellagaspera/reconcile-rs/blob/main/ARCHITECTURE.md) §8. The subsections below define the
> **field-agnostic** concepts.

<a id="g92"></a>
### 2.2 — Competing data structures and algorithms

| Term | Definition |
|---|---|
| **RBSR** (*Range-Based Set Reconciliation*) | Algorithm family (Meyer 2023): the peers maintain a family of pairwise disjoint **active ranges**, initially one **outer range**; each **protocol round**, a peer answers every active range it was asked about with **SKIP** (aggregates match → *resolved*), **IDLIST** (send the range's ordered contents outright) or **SPLIT** (replace it by a balanced family of **child ranges**). The result is the **symmetric difference** Δ(X, Y). Vocabulary and Algorithm 1 as formalized in arXiv:2603.19820 §4. The standalone `rbsr` crate implements this family with `FixedFanOut(b = 16)` as its default shipped policy; `SqrtFanOut` is an alternative policy, not the default. At fixed `b`, refinement depth is logarithmic in `n`. |
| **RSOS** (*Range-Summarizable Order-Statistics Store*) | Abstraction (arXiv:2603.19820, 2026): an ordered set offering **composable** range summaries + rank/select navigation. An augmented B+-tree realizes it → **the FingerprintTreeMap is an RSOS**. |
| **AELMDB** | **Persistent** RSOS implementation (LMDB extension, memory-mapped) from the 2026 paper, evaluated with Negentropy. The most direct competitor to the FingerprintTreeMap. Aggregates live in **branch pages** (`[child pgno \| aggregates \| separator key]`); the element summary is a byte slice *extracted* from the record, never hashed by the engine. Not content-addressed. |
| **LMDB** | Lightning Memory-Mapped Database: a copy-on-write, memory-mapped B+-tree with lock-free MVCC readers and a single writer. AELMDB's host engine, and the closest existing thing to what #271 proposes to build. |
| **Counted B-tree** | (Tatham, 2004) A B-tree carrying per-subtree element counts, giving O(log n) rank/select. The order-statistic half of RSOS, as a standalone classic. |
| **AB-tree** | (Zhao et al., VLDB 2022) A page-oriented tree maintaining aggregate metadata **under concurrent updates**; evidence that aggregate augmentation and concurrency compose, and the reference point for root-aggregate write contention. |
| **Embedded Merkle B-tree (EMB-tree)** | (Li et al., SIGMOD 2006) A B-tree caching digests in its nodes for **authenticated query answering** over outsourced databases. Prior art for "digests inside a B-tree", with a different goal (proofs, not range aggregation). |
| **PBS** (*Parity Bitmap Sketch*) | (Gong et al., VLDB 2020) A sketch-based reconciliation scheme targeting low computation together with near-optimal communication — another point on the RIBLT/minisketch Pareto front. |
| **ART** (*Approximate Reconciliation Tree*) | (Byers–Considine–Mitzenmacher, BU CS TR 2002-019) A Bloom-filter-based tree summarizing a set for **approximate** difference recovery — the approximate sibling of exact range refinement, and cited by neither dialect ([§3.1](#31-cross-community-vocabulary)). |
| **LSH-based reconciliation** | (Mitzenmacher–Morgan, PODS 2019) Locality-sensitive hashing over IBLTs, so cost scales with a **distance between the sets' elements** rather than with the number of differing elements — the axis an LWW value update lives on, which every cost column here prices as one full difference. |
| **MST** (*Merkle Search Tree*) | Auvolat & Taïani, SRDS 2019. A B-tree whose key level derives from the **hash of the key** ⇒ history-independent. Diffs **nodes**. Vulnerable to the leading-zeros attack. Usage: Bluesky/atproto. |
| **Prolly tree** (*probabilistic B-tree*) | Noms/Dolt. Content-addressed B-tree, boundaries by **rolling hash**. History-independent + **structural sharing** → versioning (Git-like). SOTA of versioned ordered stores. |
| **G-tree** (*geometric search tree*) | (A. Meyer, 2024) One randomized, **history-independent** search-tree family subsuming zip-trees, zip-zip-trees, skip-trees/dense skip-trees, **MST and prolly-trees**, generalizing zip/unzip to arity `k`. The family FingerprintTreeMap is *not* in ([§1.1](#11-merkle--anti-entropy-structures)) — unified, and made cheap, by RBSR's own author. |
| **Zip tree / zip-zip tree** | (Tarjan–Levy–Timmel 2021; Gila–Goodrich–Tarjan 2023) Randomized BSTs balanced by `Θ(lg lg n)`-bit ranks rather than `Θ(lg n)`-bit ones, **strongly history-independent** (isomorphic to skip lists), rebalanced by "zip"/"unzip" instead of rotations. The binary members of the G-tree family. |
| **Merkle radix / Patricia trie** | A Merkle tree where position depends on the key's **prefix bits**. History-independent. The basis of Ethereum. |
| **SMT** (*Sparse Merkle Tree*) | Merkle tree over a huge, mostly-empty key space; compact inclusion/exclusion proofs. |
| **Merkle tree / Merkle root** | Hash tree where each node hashes its children; the root summarizes everything. Basis of classic anti-entropy. |
| **Merkle-DAG / Merkle-CRDT** | Content-addressed, hash-linked DAG (IPFS); the links encode causal history (Merkle-CRDT, arXiv:2004.00107). |
| **IBLT** (*Invertible Bloom Lookup Table*) | A structure encoding a set into cells (XOR of key/hash + counter); subtracting two IBLTs reveals the symmetric difference via "peeling". Comm. ∝ d. A fixed one-shot capacity normally needs d known; Wu et al.'s pre-peeling statistic instead measures d from a first attempt ([#36](https://github.com/adriendellagaspera/rbsr-research/issues/36)). |
| **Rateless IBLT (RIBLT)** | *Practical Rateless Set Reconciliation*, SIGCOMM 2024. An infinite stream of coded symbols (fountain code); decodes as soon as ~d symbols are received. **No need for d**, linear compute, adversarially robust. **Single-shot SOTA choice.** |
| **minisketch / PinSketch** | Bitcoin Core library implementing PinSketch (BCH formulation of reconciliation). Comm. **optimal ≈ b·d**, O(d²) decoding, capacity to predefine. |
| **CPI / CPISync** (*Characteristic Polynomial Interpolation*) | Encodes the set as the roots of a polynomial; the ratio of the polynomials yields the difference. Minsky-Trachtenberg-Zippel. O(d³) decoding. |
| **BCH codes / Berlekamp-Massey** | Error-correcting codes / decoding algorithm used by PinSketch to reconstruct the characteristic polynomial. |
| **Strata Estimator** | A stack of sampled IBLTs estimating the difference size *d* without a prior round (Eppstein et al. 2011). |
| **CertainSync** | arXiv:2504.08314 (SIGMETRICS 2025): rateless reconciliation with **deterministic success** (no estimator or parametrization). |
| **Bloom filter** | Probabilistic membership filter (false positives, no false negatives); a Graphene component. |
| **Erlay / Graphene / BIP 330** | Bitcoin deployments: Erlay (minisketch + flooding, specified in BIP 330), Graphene (Bloom + IBLT). |
| **Negentropy** | Production RBSR implementation (Nostr/NIP-77, strfry relay). **Abandoned the naive XOR combiner** for an incremental cryptographic hash — directly relevant to F6. |
| **Willow / Earthstar / iroh / iroh-docs** | Decentralized-sync ecosystem: Willow (3D RBSR, whose spec documents XOR-fingerprint insecurity), iroh (encrypted QUIC + `iroh-docs` = persistent CRDT KV — a direct Rust competitor). |
| **Dynamo / Cassandra / ScyllaDB / Riak / Voldemort** | Distributed databases with Merkle-tree anti-entropy. Cassandra: `gc_grace_seconds`, over-streaming. ScyllaDB: repair-based tombstone GC. Reference for F4. |
| **Noms / Dolt / DoltHub** | Prolly-tree ecosystem; Dolt = "the first version-controlled relational database". |
| **content-defined chunking (CDC) / rolling hash** | Placing node boundaries where a rolling hash over the content matches a target pattern (core of prolly-trees). |
| **structural sharing / CAS / CID** | Sharing unchanged substructures across versions; *Content-Addressed Storage*; *Content IDentifier* (hash used as an address). |

<a id="g93"></a>
### 2.3 — Consistency, replication and distributed systems

| Term | Definition |
|---|---|
| **LWW** (*Last-Write-Wins*) | Conflict resolution: the value with the largest timestamp wins. Wired here on a physical clock (F5). |
| **Thomas write rule** | The rule formalizing LWW: ignore a write older than an already-applied state. |
| **eventual consistency** | Weak guarantee: with no new writes, replicas converge *eventually*. |
| **SEC** (*Strong Eventual Consistency*) | *Strong* convergence: replicas that received the same updates have identical state, **regardless of order**. Requires a commutative/associative/idempotent merge. Not reached here (F5). |
| **CRDT** (*Conflict-free Replicated Data Type*) | A type whose merge guarantees SEC. **CvRDT** (state, merge = least upper bound of a lattice) vs **CmRDT** (commutative operations). Shapiro et al. 2011. |
| **LWW-Register / MV-Register / OR-Set** | Classic CRDTs: LWW register (lossy), multi-value register (keeps concurrent values), Observed-Remove Set (add-wins). |
| **join-semilattice** | A lattice where any pair has a least upper bound; the mathematical structure underlying CvRDTs. |
| **commutative / associative / idempotent / monotone** | Properties required of a CRDT merge. reconcile-rs's merge is **not commutative** on equal timestamps (F5). |
| **Lamport clock** | A scalar logical clock respecting *happens-before*; does not detect concurrency. |
| **vector clock / version vector** | A vector of one counter per node; **detects** concurrency (incomparable vectors). O(N) cost, delicate pruning. |
| **DVV** (*Dotted Version Vector*) | A refined version vector (Preguiça et al.): O(1) causality, metadata bounded by the replication degree. Adopted by Riak. |
| **HLC** (*Hybrid Logical Clock*) | Kulkarni 2014. 64-bit timestamp = physical + logical counter: monotonic, respects causality, bounded divergence. **Recommended minimal fix** for F5 (CockroachDB, MongoDB). |
| **TrueTime / commit-wait** | Spanner approach: bounded clock-uncertainty interval (GPS+atomic) + commit wait → external consistency/linearizability. |
| **happens-before / causality** | Partial order of events (Lamport 1978). A causally later write must not be overwritten by the one it derives from. |
| **causal consistency / causal+** | Consistency respecting *happens-before*; *causal+* (COPS) = causal + convergent conflict resolution. |
| **causal stability** | A safe-GC condition: an event is purgeable only when **no concurrent operation can still arrive** (all replicas have seen it). The basis of the F4 fix. |
| **session guarantees** | (Bayou) Read-Your-Writes, Monotonic Reads, Monotonic Writes, Writes-Follow-Reads. None provided by multi-master physical LWW. |
| **resurrection / zombie** | Reappearance of deleted data when a tombstone is purged before all have seen it (F4). |
| **`gc_grace_seconds`** | Cassandra window before purging a tombstone (default **10 days**), safe *only if* a complete repair covers it — a heuristic, not a guarantee. The pre-#109 design here used a **60 s** wall-clock purge (F4); GC is now gated on causal stability instead. |
| **CAP / PACELC** | CAP: under a Partition, choose Consistency or Availability. PACELC: *Else* (normal operation), choose Latency or Consistency. reconcile-rs is **PA/EL**. |
| **clock skew / NTP / PTP** | Drift between physical clocks; synchronization protocols (NTP ~sub-second, PTP more precise). Cause of LWW losses (F5). |
| **quorum / read repair / hinted handoff** | Dynamo-like mechanisms (absent here): majority of replicas, repair at read time, buffer for an unreachable peer. |
| **split-brain / partition** | A cluster split into sub-groups that no longer communicate; each diverges. |
| **anti-entropy (push / pull)** | Periodic pairwise reconciliation. Push = push hot updates; pull = query a peer. Demers et al. 1987. |
| **gossip / epidemic / rumor mongering** | Epidemic dissemination of updates to random peers. |
| **SWIM / HyParView / memberlist / Vivaldi** | **Membership** and failure-detection protocols (≠ data sync). SWIM/`memberlist` (HashiCorp): bounded fan-out, log N convergence — recommended for F10. |

<a id="g94"></a>
### 2.4 — Cryptography, hashing and networking

| Term | Definition |
|---|---|
| **XOR** | Exclusive-OR. Commutative, associative, **self-inverse**, GF(2)-linear. Convenient for range subtraction but weak as a fingerprint (F6). |
| **GF(2)-linear** | Linear over the two-element field → an attacker *solves* (Gaussian elimination) for collision elements instead of brute-forcing them (F6). |
| **collision / second-preimage / birthday bound** | Two inputs → same hash; finding a 2nd input colliding given data; probabilistic collision threshold (~2^(b/2), i.e. ~2³² for 64-bit). All relevant to F6. |
| **SipHash** | A fast keyed PRF, 64-bit output; the `DefaultHasher` algorithm. **Not** collision-resistant in the cryptographic sense. |
| **`DefaultHasher`** | The std hasher (`std::collections::hash_map`), **not stable** across Rust versions/platforms → cross-version non-convergence (F8). |
| **BLAKE3 / xxHash** | Fast and **stable** hashes recommended as replacements (F8). |
| **incremental / homomorphic hash** | A set hash updated incrementally and composable. **MSet-XOR-Hash** (weak, self-inverse and GF(2)-linear), **MSet-Mu-Hash** (finite field), **LtHash** (lattice/vector addition, closest in spirit to reconcile-rs's hash-then-add-mod-2²⁵⁶ combiner) — the F6 fix moved off MSet-XOR-Hash onto this family. |
| **transitive group** | The minimal algebraic structure required of an RBSR fingerprint (associativity, identity, inverses, transitivity) — XOR satisfies it, hence its convenience *and* its fragility. |
| **MAC / HMAC / AEAD** | Message Authentication Code; HMAC (hash-based); Authenticated Encryption with Associated Data. The F3 fix. |
| **TLS / DTLS / Noise / QUIC** | Secure transport layers (DTLS = TLS over datagrams; Noise = a handshake framework; QUIC = encrypted transport over UDP). Options for F3; cf. issue #96. |
| **spoofing / amplification / reflection / DRDoS** | Forging the source IP (trivial in UDP); a response larger than the request toward a victim; distributed reflection denial of service. The F9 surface. |
| **bincode allocation bomb** | Deserialization where an attacker-controlled length prefix forces a massive pre-allocation (F18). |
| **UDP / datagram / MTU** | Connectionless, unreliable protocol with a spoofable source; bounded datagram; *Maximum Transmission Unit*. |

<a id="g95"></a>
### 2.5 — Complexity, theory and notation

| Term | Definition |
|---|---|
| **B-tree / B+-tree** | A balanced multi-way search tree. B+-tree: values only in the leaves. |
| **order statistics (rank / select)** | "Rank of a key" / "key at rank i" operations in O(log n) thanks to subtree counters (`tree_size`). |
| **monoid** | A set with an associative operation and an identity element; the ideal structure of a generic composable summary (a composable-summary criterion). |
| **fan-out** | Number of sub-ranges per recursion round; trades RTT vs message size. |
| **n / d / U / b** | SOTA notation: set size *n*, symmetric-difference size *d*, key universe *U*, element bit-width *b*. |
| **O(log n) / O(d log n)** | Target costs: hash-range query and per-mutation operations in O(log n); diff message volume in O(d log n). |

<a id="g96"></a>
### 2.6 — Rust tooling and ecosystem

| Term | Definition |
|---|---|
| **MSRV** (*Minimum Supported Rust Version*) | The minimum supported Rust version; absent from `Cargo.toml` (F17). |
| **clippy / `-Dwarnings`** | The Rust linter; CI treating warnings as errors. The `mismatched_lifetime_syntaxes` warning (`fingerprint_tree_map_iter.rs:177`) would break CI (F17). |
| **miri** | An interpreter detecting UB (*Undefined Behavior*); not applicable here — the crate is `#![forbid(unsafe_code)]` and all iterators are safe Rust (since `d030c15`). The CI gap for F17 is now the undeclared MSRV ([#189](https://github.com/Akvize/reconcile-rs/issues/189)). |
| **proptest / quickcheck / fuzzing** | Property-based / generative / random-input testing. **Entirely absent** (F11). |
| **`cargo audit` / `cargo deny`** | Vulnerability audit / dependency policies. Absent from CI (F19). |
| **bincode / serde / tokio / parking_lot / arrayvec / ipnet / range-cmp / chrono / rand / once_cell / tracing** | Dependencies: binary serialization; (de)serialization; async runtime; non-poisoning locks; `ArrayVec` (inline vector, B-tree nodes); network/CIDR types; key↔range comparison (`RangeOrdering`); `DateTime<Utc>` (LWW timestamps); randomness; lazy init; structured logs. |
| **`Arc` / `RwLock` / `unwrap` / `panic=abort` / `overflow-checks`** | Atomic shared pointer; reader-writer lock; panicking unwrap; panic strategy; arithmetic-overflow checking (disabled in release → F7). |
| **`ExactSizeIterator` / `FusedIterator` / `DoubleEndedIterator`** | Rust iterator traits targeted by issue #92 (full RSOS contract). |

---

## 3. Bibliography

### 3.1 Cross-community vocabulary

> Two literatures work on the same recursive-partition skeleton under two names. This document
> covers both research dialects. The map is the instrument that catches the other; it is not
> a claim that the two are interchangeable — the rows marked **No** are where conflating them
> produces a wrong statement.

| Here (`cs.DC` / `cs.CR`) | There (`cs.IT` / `cs.NI`) | Same? |
|---|---|---|
| **RBSR** — range-based set reconciliation | **PSR** — partitioned set reconciliation | **Cousins.** Same skeleton; the per-partition primitive differs — next row |
| `Fingerprint` / `RangeAggregate` / comparison value `f_p` | **SR** — set representation data structure `Z` | **No.** `Z.recovery` restores the differing *elements* (CPI / IBLT / BCH); a fingerprint only decides equality |
| enumeration threshold `t`, on `\|X ∩ [l,u)\|` — **range size** | `m̄`, on `δ` — **number of differences** (= sketch capacity) | **No.** Analogous role, different quantity |
| fan-out `b`, `FixedFanOut`, balanced `b`-partition (Def. 3.8) | partition arity; **`Q`-ary** / **`d`-ary** splitting | ≈ |
| difference size `d` | `δ` | Yes |
| store size `n` | — (PSR bounds are stated over `δ` alone) | No counterpart |
| refinement round / one-way message | communication round | ≈ |
| `T_loc` | time complexity | ≈ |

**The two lines share one ancestor and have not read each other since.** Verified from both reference
lists: Meyer (SRDS 2023) and arXiv:2603.19820 [16] both cite Minsky & Trachtenberg (Allerton 2002);
arXiv:2603.19820's 27 references carry **no** tree-algorithm, PSR-as-named or benchmarking-framework
work, and arXiv:2509.02373's 25 references carry **no** Meyer, Amparore, Negentropy or Willow.

```
                 Minsky & Trachtenberg, Allerton 2002
                        (divide-and-conquer root)
                    ┌──────────────┴──────────────┐
      fingerprint-based                      sketch-based
      RBSR  (cs.DC/cs.CR)                    PSR  (cs.IT/cs.NI)
      Meyer 2023 → Amparore 2026             Lázaro & Stefanović 2025 (EPSR)
      Negentropy, Willow                     CPI, IBLT, GenSync, tree algorithms
                    └──────── no citations either way ────────┘
```

*Bibliographic note:* both forms name the same
work — the BU technical report is titled **Practical Set Reconciliation**, and its own title page
states that a version appeared as **Scalable Set Reconciliation** at Allerton 2002. Cite
"Scalable" for the Allerton version, "Practical" for the TR.

| Community | Venues | Terms to search |
|---|---|---|
| Distributed systems / P2P | SRDS, ICDCS, EuroSys + PaPoC, arXiv preprints, protocol specs | range-based set reconciliation, anti-entropy, Merkle diff, range fingerprint, prolly/MST |
| Information theory / networking | IEEE Trans. Inf. Theory, **IEEE TNSM**, IEEE Trans. Commun., SIGCOMM, ISIT | partitioned set reconciliation, characteristic polynomial interpolation, PinSketch/BCH, IBLT, MET-IBLT, rateless |
| Random access / MAC — *EPSR's source* | IEEE Trans. Inf. Theory, IEEE Trans. Commun., ISIT, GLOBECOM | tree algorithms, collision resolution, splitting algorithms, **`Q`-ary** / **`d`-ary**, Capetanakis, Tsybakov–Mikhailov |

### 3.2 Datastore workload provenance — #49

Evidence rows use `evidence_id`, `source_version_section`, `artifact_repo_commit`, `dataset_origin`,
`access_license`, `evidence_class`, `task_boundary`, `logical_identity`, `view_semantics`,
`backend_version`, `capabilities_used`, `n`, `logical_mutations`, `symmetric_difference_elements`,
`record_shape`, `difference_locality`, `session_or_mutation_rate`, `network_budget`, `prep_costs`,
`session_costs`, `maintenance_costs`, `completion_boundary`, `independent_verification`,
`reproduction_class`, `known_unknowns`, and `external_validity`. A field not established below is
`unknown`, never an implicit zero. Reproduction classes are only `exact`, `adapted`, `observed`, or
`hypothesis`.

| evidence_id | provenance | evidence / boundary | reproduction | known unknowns / external validity |
|---|---|---|---|---|
| `selfsizing.prod90d` | `arXiv:2608.26537v1` §5.2; `whitewum/self-sizing@4ba615ca76978564d5d7d4b75424a680cae6f21d` is an observed public revision that postdates v1 | 90-day production difference profile; workload characterization only, not an online self-sizing run | `observed` | raw production records and exact paper build unavailable; production workload/lifecycle evidence, not a pilot |
| `selfsizing.relational_replay` | `arXiv:2608.26537v1` §5.3; same post-v1 public revision caveat | cross-engine replay of production-shaped relational tables; exact difference discovery boundary | `adapted` | source tables/identities are not public; relational replay evidence, not a pilot |
| `selfsizing.redis_kv` | `arXiv:2608.26537v1` §5.4 / Appendix R; same post-v1 public revision caveat | production infrastructure with production-shape KV data; frozen Redis 7.2.10 scan case, exact difference / ID resolution; source scale ≈4,890,077 keys; G1 and G2 only | `adapted` | G2 insert/delete/update composition is unknown; production snapshots/keys are not public; selected as `scan.redis72.g1g2` |
| `riblt.ethereum_snapshots` | Rateless IBLT v3; `yangl1996/riblt@297bf35be8029cd028772ab29f05962bd7eb005e` | Ethereum snapshot evidence and lifecycle context; maintained universal coded-symbol cache is optional capability, not this issue's pilot | `observed` | datastore maintenance economics are not established here; re-evaluate maintained cache in #53 |
| `rsos.aelmdb_synthetic` | `arXiv:2603.19820`; `amparore/bench-aelmdb@e74b695328a60370f89887915c428747b19ccec8` | synthetic `base_dense_i`, `i=1..8`; BTreeLMDB / NoWndAELMDB / AELMDB with source Negentropy parameters and cost accounting | `exact` | paper-unspecified set-difference counterparts stay unknown; selected as `ordered.aelmdb.base_dense` |
| `adaptive.synthetic_mininet` | AdaptiveIBLT / `arXiv:2608.15921`; exact public paper-implementation revision not established/localized | synthetic Mininet evaluation context | `observed` | no reproducible artifact pinned by #39; simulation evidence only, not a pilot |
| `adaptive.lightning_snapshots` | AdaptiveIBLT / `arXiv:2608.15921`; same revision boundary | Lightning snapshot evaluation context | `observed` | no reproducible artifact pinned by #39; Lightning evidence only, not a pilot |

Exactly two pilots are frozen under `rbsr-research/benches/fixtures/datastore-workloads/`:
`scan.redis72.g1g2.json` (Redis 7.2.10 G1/G2, `adapted`) and
`ordered.aelmdb.base_dense.json` (`base_dense_i`, `i=1..8`, `exact`). Their manifests are the
measurement contract; implementation and tuning are deliberately deferred.

**Deferred.** Rateless maintained cache → wake when #53 treats lifecycle. AdaptiveIBLT → wake when
#39 has a reproducible artifact. Stop at seven evidence rows and exactly two manifests: Pika,
Ethereum and Lightning are not pilots; ConflictSync, Rateless Bloom Filters, new datastore
integrations and broad benchmark variants remain out of scope.

### 3.3 Entries

**Set reconciliation — range-based (`cs.DC` / `cs.CR`)**
- A. Meyer, *Range-Based Set Reconciliation*, arXiv:2212.13567 (IEEE SRDS 2023) — https://arxiv.org/abs/2212.13567 ; primer: https://logperiodic.com/rbsr.html
- L. Yang, Y. Gilad, M. Alizadeh, *Practical Rateless Set Reconciliation*, SIGCOMM 2024, arXiv:2402.02668 — https://arxiv.org/abs/2402.02668 ; impl. https://github.com/yangl1996/riblt
- minisketch (Bitcoin Core), an optimized PinSketch/BCH-syndrome implementation — https://github.com/bitcoin-core/minisketch ; protocol design notes: https://github.com/bitcoin-core/minisketch/blob/master/doc/protocoltips.md ; BIP 330 — https://bips.dev/330/
  **Bears on:** a communication-first comparator with exactly `b·c` sketch bits for `b`-bit elements and capacity `c`; the implementation documents incremental extension and adaptive subdivision for unknown differences. Practical qualification is tracked in [#92](https://github.com/adriendellagaspera/rbsr-research/issues/92).
- Erlay (Naumenko et al., CCS 2019) — https://arxiv.org/abs/1905.10518
- E. G. Amparore, *RBSR via Range-Summarizable Order-Statistics Stores* (RSOS / AELMDB), arXiv:2603.19820 (2026) — https://arxiv.org/html/2603.19820 ; software: AELMDB https://github.com/amparore/aelmdb, Negentropy integration https://github.com/amparore/negentropy-aelmdb, benchmark harness https://github.com/amparore/bench-aelmdb
- A. Meyer, K. Scherer, *Range-Based Set Reconciliation without Homomorphic Hashing*, preprint 2024 —
  https://aljoscha-meyer.de/assets/landing/rbsr_nonhomomorphic.pdf — RBSR over history-independent,
  clamping-invariant trees using conventional hashes. **A direct counter-argument to the composable-summary approach**:
  the composable-monoid summary is one design point, not a requirement of the algorithm.
  **Two further observations in the primary source** bear on this comparison. §IV.B states the
  matched-range property as the *design requirement* — "if they do store the same set in a range,
  their clamped subtrees will be equal, and hence have equal root hashes" — so the replay result
  rests on what makes a range fingerprint work at all, not on the additive combiner, and survives
  this paper's own construction. §II states RBSR's distinguishing claim against the field: MST
  "can protect against malicious input only by randomizing the tree construction for each
  reconciliation session", CPI/IBLT/RIBLT likewise, leaving RBSR "the only algorithm to handle
  adversarial inputs without resorting to per-session randomization". That is the same property
  A protocol without per-session randomization also has no independent trial per peer. → [#354](https://github.com/Akvize/reconcile-rs/issues/354)
- L. Gong, Z. Liu, L. Liu, J. Xu, M. Ogihara, T. Yang, *Space- and computationally-efficient set
  reconciliation via Parity Bitmap Sketch (PBS)*, VLDB 14(4), 2020 — a further point on the
  communication/computation Pareto front, alongside RIBLT and minisketch.
- Y. Minsky, A. Trachtenberg, *Scalable set reconciliation*, Allerton 2002 (= BU TR 2002-01,
  *Practical Set Reconciliation* — same work, two titles; §3.1) — the divide-and-conquer
  ancestry of range-based refinement, predating the RBSR framing. **Also the root of the PSR line**
  ([§3.1](#31-cross-community-vocabulary)): the shared ancestor of both dialects.
- **T. Keniagin, E. Yaakobi, O. Rottenstreich**, *CertainSync: Rateless Set Reconciliation with
  Certainty*, `arXiv:2504.08314v1` (2025) — https://arxiv.org/abs/2504.08314
  **Bears on:** rateless reconciliation with a deterministic listing guarantee once its communication
  threshold is reached, without a prior difference-size estimator. Its finite-size/runtime relevance
  beside RIBLT and MET-IBLT is tracked in [#93](https://github.com/adriendellagaspera/rbsr-research/issues/93).
- *ConflictSync: Bandwidth Efficient Synchronization of Divergent State*, arXiv:2505.01144v1 (2025,
  Baquero group; published PaPoC 2026 — 13th Workshop on Principles and Practice of Consistency for
  Distributed Data, April 2026) — the first digest-driven synchronisation algorithm for state-based
  CRDTs, cutting transfer up to 18× — https://arxiv.org/abs/2505.01144
  **Bears on:** state-based-CRDT sync converging toward digest-driven sync, i.e. toward what this
  crate already does — same read as CertainSync, orthogonal axis (conflict resolution, not the
  refinement algorithm).
- *Rateless Bloom Filters*, arXiv:2510.27614 (2025, Baquero group) — https://arxiv.org/abs/2510.27614
  **Bears on:** Rateless Bloom Filters provide another sketch option for hybrid reconciliation and
  support delta-CRDT synchronization with digest-driven exchanges, as this crate does.

**Set reconciliation — partitioned and sketch-based (`cs.IT` / `cs.NI`)** *(§3.1's other dialect,
arXiv:2509.02373 and arXiv:2603.19820 are primary sources; other entries are summary-sourced, so check them before quoting numerical claims.)*

- **F. Lázaro, Č. Stefanović**, *Tree algorithms for set reconciliation*, `arXiv:2509.02373v1`
  (submitted to IEEE, 2025) — https://arxiv.org/abs/2509.02373
  **Bears on:** EPSR transmits one child's SR per split and derives the sibling's by subtracting from
  the parent's, so a **group**-valued summary saves one transmission per split where a monoid or a
  conventional hash cannot — `Fingerprint` (add/sub mod 2²⁵⁶) qualifies. Their near-halving is
  specific to **binary** partitioning; at fan-out `b` the saving is `1/b`, ~6 % at `b` = 16.
  → [#298](https://github.com/Akvize/reconcile-rs/issues/298), [#45](https://github.com/adriendellagaspera/reconcile-rs/issues/45).
- **N. Boškov, A. Trachtenberg, D. Starobinski**, *GenSync: A New Framework for Benchmarking and
  Optimizing Reconciliation of Data*, `doi:10.1109/TNSM.2022.3164369` (IEEE TNSM 19(4), 2022) —
  https://github.com/nislab/gensync
  **Bears on:** an open-source testbed for set-reconciliation *families* with a cgroup-based
  latency/bandwidth/loss lane, reporting no universally dominant protocol; **carries no RBSR**, so a
  harness claim here scopes to refinement policies inside RBSR, and its injection lane is the prior
  art #280 weighed before building its own. → [#280](https://github.com/Akvize/reconcile-rs/issues/280), [#174](https://github.com/Akvize/reconcile-rs/issues/174).
- **J. Capetanakis**, *Tree algorithms for packet broadcast channels*, `doi:10.1109/TCOM.1979.1094661`
  (IEEE Trans. Commun. 25(5), 1979) · **P. Mathys, P. Flajolet**, *Q-ary collision resolution
  algorithms in random-access systems with free or blocked channel access*,
  `doi:10.1109/TIT.1985.1057013` (IEEE Trans. Inf. Theory 31(2), 1985)
  **Bears on:** the founding and the `Q`-ary analyses of splitting when conflict locations are
  unknown — free-access throughput peaks at `Q` = 3 and falls only gradually beyond it. Fair coins
  remain throughout, near where this repo's measured `b`/ln `b` optimum also lands, so the split
  *distribution* is never optimised. Different objective (channel
  throughput), so a convergence to investigate, not a transferable bound.
  → [#257](https://github.com/Akvize/reconcile-rs/issues/257).
- **Q. Vogel, Y. Deshpande, Č. Stefanović, W. Kellerer**, *Analysis of d-ary tree algorithms with
  successive interference cancellation*, `doi:10.1017/jpr.2023.107` (J. Applied Prob. 61(3), 2024;
  preprint `arXiv:2302.08145`) — https://arxiv.org/abs/2302.08145
  **Bears on:** disproves binary-optimality — maximal throughput is reachable at any `d` ≥ 2 **given
  suitable splitting probabilities** — so the binding axis is plausibly the split *distribution*
  rather than the arity, which is what Def. 3.8's balanced equal-rank partition fixes and what
  [#25](https://github.com/adriendellagaspera/reconcile-rs/issues/25) proposes to vary. → [#25](https://github.com/adriendellagaspera/reconcile-rs/issues/25).
- **A. J. E. M. Janssen, M. J. de Jong**, *Analysis of contention tree algorithms*,
  `doi:10.1109/18.868486` (IEEE Trans. Inf. Theory 46(6), 2000)
  **Bears on:** levels-to-resolution statistics for arbitrary node degree — the analytical form of
  the round-count column `benches/protocol.rs` reports empirically. → [#257](https://github.com/Akvize/reconcile-rs/issues/257)
- **Y. Minsky, A. Trachtenberg, R. Zippel**, *Set reconciliation with nearly optimal communication
  complexity*, `doi:10.1109/TIT.2003.815784` (IEEE Trans. Inf. Theory 49(9), 2003)
  **Bears on:** CPI, the primitive PSR partitions down to; the `≈ b·d` communication optimum is the comparison with minisketch. Mechanized: the AFP entry *A Set Reconciliation Algorithm* (Hofmeier &
  Karayel — https://www.isa-afp.org/entries/Set_Reconciliation.html) proves CPI's decode∘encode
  round-trip in Isabelle/HOL (`decode_encode_correct`, its single top-level theorem). Functional
  correctness only: the abstract's "nearly optimal communication complexity" quotes this paper's
  title, no cost theorem is formalized, and nothing in it touches range-based splitting — a
  mechanized-RBSR effort would start from zero above the polynomial libraries.
- **J. Byers, J. Considine, M. Mitzenmacher**, *Fast Approximate Reconciliation of Set Differences*,
  Boston University CS TR 2002-019 (July 2002) —
  https://www.semanticscholar.org/paper/00e3a72ad7e77efae4355b19eff136ae7a509676
  **Bears on** *(no doi pinnable offline; BU TR id pinned — [§3](#3-bibliography) exception, declared)*:
  Approximate Reconciliation Trees trade exactness for cost, the approximate sibling of the exact
  refinement this repository measures — and from the same BU technical-report series
  [§3.1](#31-cross-community-vocabulary) traces both dialects' ancestor to, which is what makes its
  absence from both reference lists surprising rather than merely notable.
  → [§1.3](#13-the-design-space-rbsr-variants-over-an-rsos-and-alternatives-to-the-rsos), [#31](https://github.com/adriendellagaspera/rbsr-research/issues/31)
- **M. Mitzenmacher, T. Morgan**, *Robust Set Reconciliation via Locality Sensitive Hashing*,
  `doi:10.1145/3294052.3319690` (ACM PODS 2019; preprint `arXiv:1807.09694`) —
  https://arxiv.org/abs/1807.09694
  **Bears on:** LSH over IBLTs makes the cost scale with a distance between the sets' *elements*
  instead of a count of differing ones. Every column in this workspace counts differing `(key, value)`
  pairs, so an LWW update to an existing key — the divergence a KV store actually accumulates, and
  where the count signal reads zero ([#12](https://github.com/adriendellagaspera/rbsr-research/issues/12)) — is priced as a full difference. A
  `lift`-then-add summary is deliberately distance-destroying, so this is the argument that a different summary prices a different workload, not an adaptation of this one.
  → [§1.3](#13-the-design-space-rbsr-variants-over-an-rsos-and-alternatives-to-the-rsos), [#31](https://github.com/adriendellagaspera/rbsr-research/issues/31)
- **M. Mitzenmacher, R. Pagh**, *Simple multi-party set reconciliation*,
  `doi:10.1007/s00446-017-0316-0` (Distributed Computing 31(6), 2018; preprint `arXiv:1311.2037`) —
  https://arxiv.org/abs/1311.2037
  **Bears on:** the only entry here that is not two-party. Every cost model on this page is stated
  for one pair while `ReplicatedMap` runs an N-node cluster at O(N) write amplification — the
  fleet-level benchmark evidence the research questions need to explain. → [#174](https://github.com/Akvize/reconcile-rs/issues/174), [#354](https://github.com/Akvize/reconcile-rs/issues/354)
- **F. Lázaro, B. Matuz**, *A rate-compatible solution to the set reconciliation problem*,
  `arXiv:2211.05472v2` (IEEE Trans. Commun. 71(10), 2023 — v2 is the accepted revision) —
  https://arxiv.org/abs/2211.05472
  **Bears on:** MET-IBLTs reconcile without a prior estimate of `|d|` and avoid committing to one
  worst-case table size. They are a distinct rate-compatible comparator beside RIBLT and self-sizing
  IBLT; finite-size/runtime qualification is tracked in [#93](https://github.com/adriendellagaspera/rbsr-research/issues/93).
- **M. Wu, J. Qi, C. Luo, S. Lu, Z. Ye, Z. Wei**, *IBLTs Measure Before They Decode:
  Self-Sizing Set Reconciliation from Pre-Peeling Counts*, `arXiv:2608.26537v1` (2026) —
  https://arxiv.org/abs/2608.26537 ; reference artifact:
  https://github.com/whitewum/self-sizing/tree/4ba615ca76978564d5d7d4b75424a680cae6f21d
  **Bears on:** a failed first IBLT estimates `d` from pre-peeling counts with no extra estimator
  payload, so unknown `d` is no longer a clean discriminator between range refinement and classical
  IBLTs. Core qualification and cumulative M1+M2 accounting live in [#36](https://github.com/adriendellagaspera/rbsr-research/issues/36).
- **X. Chen, A. Sinha, D. Starobinski, A. Trachtenberg**, *Scaling the Lightning Network with
  Practical Set Reconciliation*, `arXiv:2608.15921` (IEEE ICBC 2026) —
  https://arxiv.org/abs/2608.15921
  **Bears on:** ADAPTIVEIBLT adapts IBLT reconciliation and adds partial-decoding reuse, making a
  failed/undersized attempt potentially useful progress rather than pure sunk cost. Reproduction and
  composition with self-sizing are tracked in [#39](https://github.com/adriendellagaspera/rbsr-research/issues/39)
  and [#40](https://github.com/adriendellagaspera/rbsr-research/issues/40).
- **R. Xu, K. Zhou, J. Xu, J. Guo, B. Xian, K. Yang, T. Yang, Y. Cui**, *Toward Optimal Time-Space
  Tradeoffs for Set Reconciliation*, `arXiv:2609.14442` (2026) —
  https://arxiv.org/abs/2609.14442 ; implementation: https://github.com/djwj233/XYZ-Sketch
  **Bears on:** XYZ-Sketch claims, for sufficiently large `d`, near-minimal communication together
  with O(1) insertion and O(d log V) decoding under its model. The theorem assumptions, finite-size
  regime and datastore adaptation are tracked in [#61](https://github.com/adriendellagaspera/rbsr-research/issues/61).
- **J. Klausen, R. Pagh, S. Walzer**, *Stuffed IBLTs: Optimal Linear Multiset Sketches*,
  `arXiv:2609.17487` (2026) — https://arxiv.org/abs/2609.17487
  **Bears on:** near-information-theoretic space for bounded-support/multiplicity linear sketches,
  with constant-time updates and linear decoding in the stated asymptotic regime. Practical constants
  and set-reconciliation relevance are tracked in [#62](https://github.com/adriendellagaspera/rbsr-research/issues/62);
  theorem-level consequences also feed the fundamental-research roadmap [#95](https://github.com/adriendellagaspera/rbsr-research/issues/95).
- **M. Goodrich, M. Mitzenmacher**, *Invertible Bloom lookup tables*, Allerton 2011 ·
  **D. Eppstein, M. Goodrich, F. Uyeda, G. Varghese**, *What's the difference? Efficient set
  reconciliation without prior context*, `doi:10.1145/2043164.2018462` (SIGCOMM 2011) ·
  **P. Ozisik et al.**, *Graphene*, `doi:10.1145/3341302.3342082` (SIGCOMM 2019)
  **Bears on:** the IBLT origin and difference-digest framing behind the hybrid comparison, and the
  Bloom-prefilter-plus-IBLT design that prefigures ConflictSync's two-stage approach.
  → [#45](https://github.com/adriendellagaspera/reconcile-rs/issues/45).

**Merkle / anti-entropy structures**
- A. Auvolat, F. Taïani, *Merkle Search Trees*, SRDS 2019 — https://inria.hal.science/hal-02303490 ; crate https://github.com/domodwyer/merkle-search-tree ; Bluesky/atproto usage — https://atproto.com/specs/repository
- Prolly trees (Dolt/Noms) — https://docs.dolthub.com/architecture/storage-engine/prolly-tree ; https://www.dolthub.com/blog/2025-06-03-people-keep-inventing-prolly-trees/
- **A. Meyer**, *Geometric Search Trees* (G-trees), 2024 — https://g-trees.github.io/g_trees/ ; code
  https://github.com/g-trees/g_trees
  **Bears on** *(web-published, no arXiv/DOI located; this entry is summary-sourced)*:
  one randomized, history-independent family subsuming zip-trees, zip-zip-trees, skip-trees, dense
  skip-trees, MST **and** prolly-trees, with `k`-ary members for block storage — by the author of
  `arXiv:2212.13567`. It is the realization Meyer–Scherer's non-homomorphic RBSR runs over, and it
  moves the reference point [§1.1](#11-merkle--anti-entropy-structures)'s cost claim was written
  against. → [§1.1](#11-merkle--anti-entropy-structures), [§1.3](#13-the-design-space-rbsr-variants-over-an-rsos-and-alternatives-to-the-rsos), [#29](https://github.com/adriendellagaspera/rbsr-research/issues/29)
- **R. E. Tarjan, C. Levy, S. Timmel**, *Zip Trees*, `arXiv:1806.06726` · `doi:10.1145/3476830` (ACM
  TALG 17(4), 2021) — https://arxiv.org/abs/1806.06726 · **O. Gila, M. T. Goodrich, R. E. Tarjan**,
  *Zip-zip Trees*, `arXiv:2307.07660` (WADS 2023, `doi:10.1007/978-3-031-38906-1_31`; Algorithmica,
  `doi:10.1007/s00453-025-01364-2`, 2025) — https://arxiv.org/abs/2307.07660
  **Bears on:** the binary members of the G-tree family and the reason history-independence is now
  cheap — `Θ(lg lg n)` bits of rank metadata per node, strong history-independence by isomorphism
  with skip lists, and no rolling hash. What [#29](https://github.com/adriendellagaspera/rbsr-research/issues/29)'s
  empty cell would have to carry the `Rsos<K>` augmentations on top of.
  → [§1.1](#11-merkle--anti-entropy-structures), [#29](https://github.com/adriendellagaspera/rbsr-research/issues/29)
- J. Gustafson, *Merklizing the key/value store* (Merkle radix / SMT) — https://joelgustafson.com/posts/2023-05-04/merklizing-the-key-value-store-for-fun-and-profit/
- Merkle-CRDTs, arXiv:2004.00107 — https://arxiv.org/abs/2004.00107
- Dynamo (DeCandia et al., SOSP 2007) — https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf
- Cassandra repair / over-streaming — https://www.pythian.com/blog/effective-anti-entropy-repair-cassandra
- Willow 3d-RBSR (fingerprint security) — https://willowprotocol.org/specs/3d-range-based-set-reconciliation/index.html ; Negentropy — https://github.com/hoytech/negentropy
- Demers et al., *Epidemic Algorithms*, PODC 1987 ; SWIM — https://www.cs.cornell.edu/projects/Quicksilver/public_pdfs/SWIM.pdf ; memberlist — https://github.com/hashicorp/memberlist
- **B. Doerr, A. Kostrygin**, *Randomized Rumor Spreading Revisited*, `arXiv:2303.11150v1` (full
  version of ICALP 2017) — https://arxiv.org/abs/2303.11150
  **Bears on:** the sharpest gossip round counts to additive constants (push-pull:
  `log₃n + log₂ln n ± O(1)`), and the theorem that constant per-message loss destroys the
  double-exponential end phase — message complexity degrades from `Θ(n log log n)` to
  `Θ(n log n)` — a published anchor for loss, not RTT, being the binding term on an unreliable
  transport. → [#23](https://github.com/adriendellagaspera/reconcile-rs/issues/23).
- A. Rawat, T. K. Vangani, H. Cornelius, V. Daza, *Accelerating Prolly Trees: Simplified Chunking for
  Rapid Updates*, DLT 2024 workshop (CEUR-WS Vol-3791, paper 8) — https://ceur-ws.org/Vol-3791/paper8.pdf
  ; journal version `doi:10.1145/3785142` (ACM Distributed Ledger Technologies: Research and
  Practice, online 2026-01-06)
  **Bears on:** replaces the classic rolling-hash chunker's O(N)-worst-case cascading rechunking
  with an anchor-node design bounding each insertion to one chunk plus an O(H) anchor-path update
  (≤2H hashes), height staying O(log n) — narrows but does not remove the "heavy machinery /
  higher latency" ❌ against FingerprintTreeMap.

**Aggregate-augmented and page-oriented trees** *(the structural ancestry of `FingerprintTreeMap`,
surfaced by arXiv:2603.19820's related work — the project questions in issues #257/#271 remain active here)*
- S. Tatham, *Counted B-Trees* (2004) — https://www.chiark.greenend.org.uk/~sgtatham/algorithms/cbtree.html
  — the subtree-count augmentation giving O(log n) rank/select. Direct prior art for `tree_size`:
  the order-statistic half of RSOS is a documented classic, not a 2026 result.
- Z. Zhao, D. Xie, F. Li, *AB-tree: Index for Concurrent Random Sampling and Updates*,
  `doi:10.14778/3538598.3538606` (VLDB 15(9), 2022) — https://vldb.org/pvldb/vol15/p1835-zhao.pdf ;
  code (primary source read for this entry — `vldb.org` was egress-blocked) via
  https://github.com/zzy7896321/abtree_public.
  **Bears on:** the mechanism is not root-path locking — writers update the in-page aggregate in
  place with atomic Fetch-And-Add (weight updates commute, §3.1) *and* prepend immutable delta
  records (tagged by inserting xmin) to a lock-free per-child-page version chain, which snapshot
  readers use to **subtract invisible deltas** from the in-place value (§4.4); stored weights are
  deliberately inexact upper bounds corrected by rejection sampling (Def. 1) — AB-tree buys
  concurrency by relaxing the exactness a sound SKIP cannot relax. An epoch-based GC reclaims dead
  chain nodes; the root's entry is deliberately not removed on the hot path — that detail is in the
  code alone (`_abt_install_version_chain`'s comment; vacuum collects it when the tree quiesces),
  not in the paper (paper and code checked).
  [#359](https://github.com/Akvize/reconcile-rs/issues/359).
- F. Li, M. Hadjieleftheriou, G. Kollios, L. Reyzin, *Dynamic authenticated index structures for
  outsourced databases* (Embedded Merkle B-tree), SIGMOD 2006 — twenty years of prior art on
  caching digests inside a B-tree. Different goal (verifiable query answering, not range
  aggregation), but it bounds any novelty claim and supplies the vocabulary if inclusion proofs
  ever become a requirement.
- S. Roura, *A new method for balancing binary search trees*, ICALP 2001 — balancing by subtree
  weight; background for the untuned interaction between the tree order (6) and the protocol
  fan-out ([#257](https://github.com/Akvize/reconcile-rs/issues/257)).
- H. Chu et al., *LMDB* — https://github.com/LMDB — a copy-on-write, memory-mapped B+-tree with
  lock-free MVCC readers and a single writer. Named here because that *is* the property epic
  [#36](https://github.com/adriendellagaspera/reconcile-rs/issues/36) sets out to build in safe Rust: the
  build-vs-adopt comparison should be made against it explicitly rather than by default.

**Range selection and orthogonal range searching (`cs.DS` / `cs.CG`)** *(a third dialect, opened
for [#360](https://github.com/Akvize/reconcile-rs/issues/360): the operation a `δ > 1` RSOS
needs beyond Def. 3.9 is this literature's central object, and its bounds are settled. **`δ` is the
dimension throughout this group and [`ARCHITECTURE.md`](https://github.com/adriendellagaspera/reconcile-rs/blob/main/ARCHITECTURE.md) §7, not [§3.1](#31-cross-community-vocabulary)'s
PSR difference size** — the one symbol the three dialects genuinely collide on. Umbrella survey:
P. K. Agarwal, *Range searching*, Handbook of Discrete and Computational Geometry 3rd ed. ch. 41 —
https://users.cs.duke.edu/~pankaj/publications/surveys/rs3ed.pdf . Sourced from search summaries.)*

- **M. He, J. I. Munro, P. K. Nicholson**, *Dynamic range selection in linear space*, `arXiv:1106.5076`
  · `doi:10.1007/978-3-642-25591-5_18` (ISAAC 2011, LNCS 7074, pp. 160–169) —
  https://arxiv.org/abs/1106.5076
  **Bears on:** its problem statement *is* the missing operation — the `k`-th smallest `y` among the
  points whose `x` lies in a query range — at `O((lg n/lg lg n)²)` query and amortized update in
  **linear** space. The primitive #360 expected to be the blocker is the affordable half.
  → [#360](https://github.com/Akvize/reconcile-rs/issues/360), [`ARCHITECTURE.md`](https://github.com/adriendellagaspera/reconcile-rs/blob/main/ARCHITECTURE.md) §7
- **A. G. Jørgensen, K. G. Larsen**, *Range selection and median: tight cell probe lower bounds and
  adaptive data structures*, `doi:10.1137/1.9781611973082.63` (SODA 2011, pp. 805–813) —
  https://cs.au.dk/~larsen/papers/range_median.pdf
  **Bears on:** `Ω(lg n/lg lg n)` for *static* range selection in `n·lg^O(1) n` bits, matched by
  Brodal & Jørgensen (ISAAC 2009, https://users-cs.au.dk/gerth/papers/isaac09median.pdf) — so the
  primitive's price is **tight**, not merely unimproved.
  → [#360](https://github.com/Akvize/reconcile-rs/issues/360)
- **K. G. Larsen**, *The cell probe complexity of dynamic range counting*, `arXiv:1105.5933` ·
  `doi:10.1145/2213977.2213987` (STOC 2012, pp. 85–94) — https://arxiv.org/abs/1105.5933 ;
  strengthening **M. Pătraşcu**, *Lower bounds for 2-dimensional range counting*,
  `doi:10.1145/1250790.1250797` (STOC 2007, pp. 40–46)
  **Bears on:** the load-bearing citation of the no-go. `t_q = Ω((lg n/lg(w·t_u))²)`, i.e.
  `Ω((lg n/lg lg n)²)` at cell size `w = Θ(lg n)` under any polylog update, for **weighted** 2D range
  counting — and `Aggregate` carries a 256-bit `Fingerprint`. It therefore binds the operation Def. 3.9
  **already has**, which is why `δ > 1` is priced by the dimension and not by the new primitive.
  → [#360](https://github.com/Akvize/reconcile-rs/issues/360).
- **O. Weinstein, H. Yu**, *Amortized dynamic cell-probe lower bounds from four-party communication*,
  `arXiv:1604.03030v1` (FOCS 2016) — https://arxiv.org/abs/1604.03030
  **Bears on:** closes the escape hatch the row above leaves open. Larsen's `t_u` is **worst case**, so an
  amortized structure was the one way a `δ = 2` box aggregate could still have been cheap; its Thm 1 gives
  the same `Ω((lg n/lg lg n)²)` **amortized and randomized**, for dynamic weighted 2-D orthogonal range
  counting (weights in `[n]`, `w = Θ(lg n)`). The `δ = 1` baseline two rows down already allows
  amortization, so the two ends of §7's comparison are now quantified alike. See the primary sources listed in this bibliography entry.
  → [#360](https://github.com/Akvize/reconcile-rs/issues/360), [`ARCHITECTURE.md`](https://github.com/adriendellagaspera/reconcile-rs/blob/main/ARCHITECTURE.md) §7
- **T. M. Chan, B. T. Wilkinson**, *Adaptive and Approximate Orthogonal Range Counting*,
  SODA 2013 — http://tmc.web.engr.illinois.edu/orcount_soda.pdf (the September 2012 preprint, the
  document read for this page; its "last year's SODA" designates [JL11] = SODA 2011)
  **Bears on:** the primary paper gives linear-space range selection at `O(1 + lg_w k)`, matching
  [JL11]'s lower bound, and states that range selection is closely related to 2-D 3-sided
  orthogonal range counting. Its static linear-space `O(lg_w n)` box counting motivates #360's
  open question about per-session snapshots.
  → [#360](https://github.com/Akvize/reconcile-rs/issues/360)
- **M. Pătraşcu, E. D. Demaine**, *Tight bounds for the partial-sums problem*,
  `doi:10.5555/982792.982796` (SODA 2004; journal version *Logarithmic lower bounds in the cell-probe
  model*, SIAM J. Comput. 35(4), 2006, pp. 932–963)
  **Bears on:** the one-dimensional baseline the row above is measured against — dynamic partial sums cost
  `Θ(1 + lg n/lg(w/s))` for cell size `w` and summary width `s`, hence `Θ(lg n)` once the summary is a
  word wide. `FingerprintTreeMap`'s `O(lg n)` aggregate is **optimal**, not merely adequate, so the
  `δ > 1` comparison is tight on both sides. → [#360](https://github.com/Akvize/reconcile-rs/issues/360).

**Consistency & conflict resolution**
- Kingsbury (Jepsen), *The trouble with timestamps* — https://aphyr.com/posts/299-the-trouble-with-timestamps ; *Jepsen: Cassandra* — https://aphyr.com/posts/294-jepsen-cassandra
- S. Kulkarni et al., *Hybrid Logical Clocks*, 2014 — https://cse.buffalo.edu/tech-reports/2014-04.pdf
- Shapiro et al., *CRDTs*, INRIA RR-7506 / SSS 2011 — https://inria.hal.science/inria-00555588/en/
- Preguiça et al., *Dotted Version Vectors*, arXiv:1011.5808 — https://arxiv.org/abs/1011.5808
- Clarke et al., *Incremental Multiset Hash Functions*, ASIACRYPT 2003 — https://people.csail.mit.edu/devadas/pubs/mhashes.pdf
  **Bears on** *(no arXiv/doi pinnable offline — supplied-PDF read; pin when next touched, §3.2)*:
  MSet-Add-Hash is the one short-output additive construction with a security proof —
  keyed by a PRF whose key is **secret to the verifier** (bound `u²/2^m + (d/n)^l`); with the key
  known and the nonce fixed, security degrades to exactly the weighted-knapsack problem Wagner's
  k-tree attacks — so a cluster-shared key protects against outsiders only, never key holders.
  → [#19](https://github.com/adriendellagaspera/reconcile-rs/issues/19), [#471](https://github.com/Akvize/reconcile-rs/issues/471)
- **M. Bellare, D. Micciancio**, *A New Paradigm for Collision-free Hashing: Incrementality at
  Reduced Cost*, EUROCRYPT 1997 — https://cseweb.ucsd.edu/~mihir/papers.html
  **Bears on** *(no arXiv/doi pinnable offline — supplied-PDF read; pin when next touched, §3.2)*:
  defines the randomize-then-combine paradigm (AdHASH/MuHASH/LtHASH) and reduces
  AdHASH collisions to the weighted-knapsack problem — the security framework a keyed lift argues
  in; its pre-Wagner "a few hundred bits" sizing is exactly what Wagner broke.
  → [#19](https://github.com/adriendellagaspera/reconcile-rs/issues/19).
- **K. Lewi, W. Kim, I. Maykov, S. Weis**, *Securing Update Propagation with Homomorphic Hashing*
  (LtHash), IACR ePrint 2019/227 — https://eprint.iacr.org/2019/227.pdf
  **Bears on** *(ePrint carries no doi; id pinned as 2019/227 — §3.2 exception, declared)*:
  the deployed post-Wagner unkeyed additive hash pays 16384 bits for ~200-bit
  security (lthash16 = 1024 × 16-bit lanes), and its §1.3 disputes [MGS15]'s smaller sizings —
  the quantified case that an unkeyed lift cannot stay at 256 bits, keying can.
  → [#19](https://github.com/adriendellagaspera/reconcile-rs/issues/19).
- **D. Wagner**, *A Generalized Birthday Problem*, `doi:10.1007/3-540-45708-9_19` (CRYPTO 2002,
  LNCS 2442, pp. 288–303) — https://www.iacr.org/archive/crypto2002/24420288/24420288.pdf
  **Bears on:** the k-tree solves the balance problem over `ℤ/2^w` in subexponential time, so a wide
  non-GF(2)-linear combiner is necessary but **not sufficient** — the source for
  "Wagner-breakable", and for why more width is not the remedy.
  → [#19](https://github.com/adriendellagaspera/reconcile-rs/issues/19).
- Abadi, *PACELC* — https://en.wikipedia.org/wiki/PACELC_design_principle ; ScyllaDB repair-based tombstone GC — https://www.scylladb.com/2022/06/30/preventing-data-resurrection-with-repair-based-tombstone-garbage-collection/

**Related projects**
- Pekko Distributed Data — https://pekko.apache.org/docs/pekko/current/typed/distributed-data.html
- Hazelcast Replicated Map — https://docs.hazelcast.com/hazelcast/5.6/data-structures/replicated-map
- iroh / iroh-docs — https://github.com/n0-computer/iroh ; automerge — https://github.com/automerge/automerge

---

## 4. Alphabetical index

> Index of the [glossary (§2)](#2-glossary) terms. Each entry links to the subsection where the term
> is defined: [2.1 repo → code](#g91) · [2.2 structures/algos](#g92) · [2.3 distributed](#g93) ·
> [2.4 crypto/network](#g94) · [2.5 complexity](#g95) · [2.6 Rust](#g96).

**A** — AB-tree [2.2](#g92) · AEAD [2.4](#g94) · ART (Approximate Reconciliation Tree) [2.2](#g92) · AELMDB [2.2](#g92) · amplification [2.4](#g94) · anti-entropy [2.3](#g93) · `Arc` [2.6](#g96) · `ArrayVec` [2.6](#g96) · associative [2.3](#g93)

**B** — B-tree / B+-tree [2.5](#g95) · BCH codes [2.2](#g92) · Berlekamp-Massey [2.2](#g92) · bincode [2.6](#g96) · bincode allocation bomb [2.4](#g94) · BIP 330 [2.2](#g92) · birthday bound [2.4](#g94) · BLAKE3 [2.4](#g94) · Bloom filter [2.2](#g92)

**C** — CAP [2.3](#g93) · CAS [2.2](#g92) · Cassandra [2.2](#g92) · Counted B-tree [2.2](#g92) · causal consistency / causal+ [2.3](#g93) · causal stability [2.3](#g93) · CDC (content-defined chunking) [2.2](#g92) · CertainSync [2.2](#g92) · chrono [2.6](#g96) · CID [2.2](#g92) · clippy [2.6](#g96) · clock skew [2.3](#g93) · CmRDT [2.3](#g93) · collision [2.4](#g94) · commit-wait [2.3](#g93) · commutative [2.3](#g93) · content-addressing [2.2](#g92) · CPI / CPISync [2.2](#g92) · CRDT [2.3](#g93) · CvRDT [2.3](#g93)

**D** — datagram [2.4](#g94) · `DateTime<Utc>` [2.6](#g96) · `DefaultHasher` [2.4](#g94) · Dolt / DoltHub [2.2](#g92) · `DoubleEndedIterator` [2.6](#g96) · DRDoS [2.4](#g94) · DTLS [2.4](#g94) · DVV (Dotted Version Vector) [2.3](#g93) · Dynamo [2.2](#g92)

**E** — Earthstar [2.2](#g92) · EMB-tree (Embedded Merkle B-tree) [2.2](#g92) · epidemic [2.3](#g93) · Erlay [2.2](#g92) · eventual consistency [2.3](#g93) · `ExactSizeIterator` [2.6](#g96)

**F** — fan-out [2.5](#g95) · `FusedIterator` [2.6](#g96) · fuzzing [2.6](#g96)

**G** — `gc_grace_seconds` [2.3](#g93) · G-tree [2.2](#g92) · GF(2)-linear [2.4](#g94) · gossip [2.3](#g93) · Graphene [2.2](#g92)

**H** — happens-before [2.3](#g93) · Hazelcast [2.2](#g92) · hinted handoff [2.3](#g93) · history-independence [2.2](#g92) · HLC (Hybrid Logical Clock) [2.3](#g93) · HMAC [2.4](#g94) · homomorphic hash [2.4](#g94) · HyParView [2.3](#g93)

**I** — IBLT [2.2](#g92) · idempotent [2.3](#g93) · incremental hash [2.4](#g94) · ipnet [2.6](#g96) · iroh / iroh-docs [2.2](#g92)

**J** — join-semilattice [2.3](#g93)

**L** — Lamport clock [2.3](#g93) · LMDB [2.2](#g92) · leading-zeros (attack) [2.2](#g92) · LSH-based reconciliation [2.2](#g92) · LtHash [2.4](#g94) · LWW (Last-Write-Wins) [2.3](#g93) · LWW-Register [2.3](#g93)

**M** — MAC [2.4](#g94) · memberlist [2.3](#g93) · Merkle-CRDT [2.2](#g92) · Merkle-DAG [2.2](#g92) · Merkle radix / Patricia [2.2](#g92) · Merkle tree / root [2.2](#g92) · minisketch [2.2](#g92) · miri [2.6](#g96) · monoid [2.5](#g95) · monotone [2.3](#g93) · MSet-Mu-Hash / MSet-XOR-Hash [2.4](#g94) · MSRV [2.6](#g96) · MST (Merkle Search Tree) [2.2](#g92) · MTU [2.4](#g94) · MV-Register [2.3](#g93)

**N** — *n / d / U / b* (notation) [2.5](#g95) · Negentropy [2.2](#g92) · Noise [2.4](#g94) · Noms [2.2](#g92) · NTP [2.3](#g93)

**O** — O(log n) / O(d log n) [2.5](#g95) · once_cell [2.6](#g96) · order statistics (rank/select) [2.5](#g95) · OR-Set [2.3](#g93) · over-streaming [2.2](#g92)

**P** — PACELC [2.3](#g93) · PBS (Parity Bitmap Sketch) [2.2](#g92) · `panic=abort` [2.6](#g96) · parking_lot [2.6](#g96) · partition [2.3](#g93) · Patricia trie [2.2](#g92) · Pekko Distributed Data [2.2](#g92) · PinSketch [2.2](#g92) · prolly tree [2.2](#g92) · proptest [2.6](#g96) · PTP [2.3](#g93) · push / pull [2.3](#g93)

**Q** — QUIC [2.4](#g94) · quickcheck [2.6](#g96) · quorum [2.3](#g93)

**R** — rand [2.6](#g96) · range-cmp / `RangeOrdering` [2.6](#g96) · rank / select [2.5](#g95) · Rateless IBLT (RIBLT) [2.2](#g92) · RBSR [2.2](#g92) · read repair [2.3](#g93) · resurrection / zombie [2.3](#g93) · Riak [2.2](#g92) · rolling hash [2.2](#g92) · rumor mongering [2.3](#g93) · `RwLock` [2.6](#g96) · RSOS [2.2](#g92)

**S** — ScyllaDB [2.2](#g92) · second-preimage [2.4](#g94) · SEC (Strong Eventual Consistency) [2.3](#g93) · serde [2.6](#g96) · session guarantees [2.3](#g93) · SipHash [2.4](#g94) · SMT (Sparse Merkle Tree) [2.2](#g92) · spoofing [2.4](#g94) · split-brain [2.3](#g93) · Strata Estimator [2.2](#g92) · structural sharing [2.2](#g92) · SWIM [2.3](#g93)

**T** — Thomas write rule [2.3](#g93) · TLS [2.4](#g94) · tokio [2.6](#g96) · tracing [2.6](#g96) · transitive group [2.4](#g94) · TrueTime [2.3](#g93)

**U** — UDP [2.4](#g94) · `unwrap` [2.6](#g96) · `overflow-checks` [2.6](#g96)

**V** — vector clock / version vector [2.3](#g93) · Vivaldi [2.3](#g93) · Voldemort [2.2](#g92)

**W** — Willow [2.2](#g92) · Writes-Follow-Reads [2.3](#g93)

**X** — XOR [2.4](#g94) · xxHash [2.4](#g94)

**Z** — zip tree / zip-zip tree [2.2](#g92)

---

*The bibliography records the literature behind the survey panels. Current implementation invariants live in [`reconcile-rs` architecture](https://github.com/adriendellagaspera/reconcile-rs/blob/main/ARCHITECTURE.md).*
