# Benchmarks

This repository owns only comparative/experimental targets:

| Target | What it measures |
|---|---|
| `protocol` | Comparative RBSR policy cost, including wire/transport projections used by research sweeps. |
| `reconciliation` | Structured comparison/report generation across research candidates and fixtures. |

CI compile-checks these targets but does not run benchmark workloads.

Canonical implementation benchmarks live with the code they measure:

- RSOS microbenchmarks and contention: `range-based-set-reconciliation` (`rsos --bench micro`, `rsos --bench contention`).
- Intrinsic RBSR protocol/history behavior: `range-based-set-reconciliation` (`rbsr --bench protocol`, `rbsr --bench history_independence`).
- ReplicatedMap/runtime/system behavior: `reconcile-rs` (`replicated_map`, `system`, membership, persistence, and runtime-history targets).

The result sections below are retained as research evidence. Where their original harness moved to a
canonical implementation repository, the reproduction command now names that repository's target.

## Canonical runtime lanes

## The injected-RTT / loss lane

```sh
# Run from reconcile-rs:
cargo bench --bench system -- netem                    # calibration report only
cargo bench --bench system -- cold_sync_rtt
cargo bench --bench system -- gossip_propagation_rtt
cargo bench --bench system -- gossip_fanout_rtt
```

Every other benchmark in this repository runs at RTT ≈ 0 and loss = 0, which prices the axis RBSR is
good at (bytes) and zeroes the one it is worst at (`POSITIONING.md` §1.3: sequential round-trips). These
four lanes are the instrument that answer [#280](https://github.com/Akvize/reconcile-rs/issues/280)
and, for `gossip_fanout_rtt`, the RTT-sweep half of
[#187](https://github.com/Akvize/reconcile-rs/issues/187)'s measurement arm (#280 shipped
`cold_sync_rtt`/`gossip_propagation_rtt`; `gossip_fanout` itself had no RTT-swept counterpart until
now).

The canonical injected-network instrument now lives in `reconcile-rs` as `gossip::netem::NetemTransport`: a seeded `Transport` decorator providing one-way delay, jitter, loss, and reordering per **directed** link over `InMemoryNetwork`. The runtime benchmark target owns its calibration and convergence checks. The result tables below are retained here as research evidence because they price the transport axis used by the comparative models.

**Both `rtt=0ms` columns below are the same harness with a perfect link, not the loopback
benchmarks** — the decorator, the pump and the in-memory fabric are in every lane, so a delta is the
injected network and nothing else. Cross-harness it is not comparable: `cold_sync/1000` on real
loopback UDP is 2.18 ms against `cold_sync_rtt/n=1000/rtt=0ms`'s 0.98 ms.

### Calibration

`netem_calibration` prints this before every run; read it first, because a delay lane that silently
quantized to tokio's 1 ms timer resolution would look like a protocol result. Measured on a 4-core
Xeon @ 2.10 GHz, seed `0x5eed0280`:

| lane | injected one-way | observed mean | lane | configured | realized |
|---|---:|---:|---|---:|---:|
| `rtt=0ms` | 0 | 39 µs | `loss=0.1%` | 0.100 % | 0.110 % |
| `rtt=0.1ms` | 50 µs | 70 µs | `loss=1%` | 1.000 % | 0.940 % |
| `rtt=1ms` | 500 µs | 536 µs | | | |
| `rtt=10ms` | 5 ms | 5.22 ms | | | |
| `rtt=50ms` | 25 ms | 25.30 ms | | | |

So the harness floor is ≈ 39 µs per one-way delivery and the injected delay is faithful from 50 µs
up. That floor is the resolution of every number below.

### Results: what RTT ≈ 0 was hiding

| benchmark | rtt=0ms | 0.1 ms | 1 ms | 10 ms | 50 ms | delta vs rtt=0 |
|---|---:|---:|---:|---:|---:|---|
| `cold_sync_rtt/n=1000` | 984 µs | 1.07 ms | 2.05 ms | 11.50 ms | 51.50 ms | **+1.01 × RTT** (52× at 50 ms) |
| `cold_sync_rtt/n=10000` | 11.53 ms | 11.48 ms | 12.48 ms | 22.38 ms | 62.48 ms | **+1.02 × RTT** (5.4× at 50 ms) |
| `gossip_propagation_rtt/N=8` | 103 µs | 140 µs | 595 µs | 5.33 ms | 25.69 ms | **+0.50 × RTT** (250× at 50 ms) |

Both deltas are constants in RTT, and both are integer numbers of one-way hops:

- **A write propagates in one hop, not a chain.** `Replica::broadcast` sends every local write to
  every known peer directly, so `gossip_propagation` costs exactly RTT/2 — flat in `N`, and
  unaffected by `POSITIONING.md`'s O(log n)-rounds argument, which is about *reconciliation*, not gossip.
- **Cold sync costs exactly one round trip, whatever the dataset size.** Not O(log n): an empty peer
  has nothing to refine. Its outer range differs, the answer is the whole dataset, and the exchange
  is two one-way hops — the empty node's initial ranges out, the values back. The `n=1000` and
  `n=10000` rows add the same 1 × RTT to very different baselines.
- **So `cold_sync` never exercised the O(log n) refinement chain**, at any RTT. That chain runs when
  the difference is *small relative to the store* — the regime RBSR exists for. `benches/protocol.rs`
  counts its rounds, and the rows above are what converts that count into seconds: **one protocol
  round trip costs one RTT, measured, with no hidden multiplier**. Refinement depth, message count,
  round trips and the resulting wall clock at 50 ms RTT, across n = 10³…10⁶ at `b` = 16 (`d` = 1, one
  element missing; `POSITIONING.md` §2.2 draws the "worst family on latency" conclusion from this table
  without repeating the numbers):

  | quantity | n = 10³ | 10⁴ | 10⁵ | 10⁶ |
  |---|---:|---:|---:|---:|
  | `⌈log₁₆ n⌉` — refinement depth, the model | 3 | 4 | 5 | 5 |
  | one-way messages, measured — incl. the opening exchange and the closing item transfer | 6 | 6 | 6 | 8 |
  | round trips = half the row above | 3 | 3 | 3 | **4** |
  | wall clock at 50 ms RTT | 150 ms | 150 ms | 150 ms | **200 ms** |

  A single missing element in a 10⁶-entry store is therefore ~4 × RTT — ~200 ms at 50 ms RTT —
  against 3.8 kB of traffic.
- Pricing that end-to-end rather than by composition needs a difference the two peers disagree on
  *without* disagreeing on timestamps, which only `just_insert`/`just_remove` can build. Those are
  `reconcile_internal_testing` seams. The canonical `service_reconcile_rtt` lane now lives in
  `reconcile-rs`'s `replicated_map` target; the results below are retained here as research evidence.

### Results: `gossip_fanout_rtt` is flat, confirming there is no round trip to price ([#187](https://github.com/Akvize/reconcile-rs/issues/187))

Measured on a 4-core Xeon @ 2.80 GHz (a different machine from the table above, so read this
subsection's numbers against each other, not against `cold_sync_rtt`/`gossip_propagation_rtt`'s),
seed `0x5eed0280`, `--quick`:

| `N=8` | rtt=0ms | 0.1 ms | 1 ms | 10 ms | 50 ms | loss=0.1% | loss=1% |
|---|---:|---:|---:|---:|---:|---:|---:|
| datagrams / bytes sent by the origin | 7 / 196 B | 7 / 196 B | 7 / 196 B | 7 / 196 B | 7 / 196 B | 7 / 196 B | 7 / 196 B |
| send-loop wall time | 27.0 µs | 23.9 µs | 23.9 µs | 28.9 µs | 35.5 µs | 33.6 µs | 28.5 µs |

Both rows are flat: the datagram/byte count is exact and identical at every lane (as it is at RTT ≈ 0
in plain `gossip_fanout`), and the timed row stays inside a ~12 µs band with no trend against RTT —
in particular **no ~25 ms step at `rtt=50ms`**, the signature `gossip_propagation_rtt` shows at the
same `N` (25.69 ms there, from the table above). That confirms the prediction stated on
`gossip_fanout_rtt`'s own doc comment: `NetemTransport::send_to` enqueues (or drops, in the loss
lanes) a datagram and returns immediately, before the injected delay elapses, so the origin's
send-loop never waits on delivery — there is no round trip on this path for RTT to lengthen.
`gossip_propagation_rtt`'s **+0.50 × RTT** is entirely a receive-side/observation cost, not a
fan-out one; `Replica::broadcast`'s `O(N)` datagram cost (`gossip_fanout`, RTT ≈ 0 table above) and
its RTT-independence (this table) are two separate findings, both now measured. This is the
`gossip_fanout` half of #187's "the same across #280's RTT sweep" acceptance item; #280 shipped the
`gossip_propagation`/`cold_sync` halves.

### Results: loss, at `rtt=1ms`

| benchmark | no loss | loss=0.1% | loss=1% |
|---|---:|---:|---:|
| `cold_sync_rtt/n=1000` | 2.05 ms | 1.97 ms | 20.5 ms |
| `cold_sync_rtt/n=10000` | 12.48 ms | 16.2 ms | 112 ms |
| `gossip_propagation_rtt/N=8` | 595 µs | 6.55 ms | 100 ms |

**A lost datagram costs a full `reconcile_interval`, not a retransmit.** There is no retransmission:
the exchange is only repaired by the next anti-entropy round, so the penalty is the cadence —
`Config::reconcile_interval`, 1 s by default. Every cell above is that mechanism and nothing else:
the mean is `P(any datagram of the exchange lost) × ~1 s`, which for `gossip_propagation_rtt`'s 7
receivers is `1 − 0.99⁷ ≈ 6.8 %` → ~100 ms (measured 100 ms), and at 0.1 % is `≈ 0.7 %` → ~7 ms
(measured 6.55 ms). Hence the wide confidence intervals: the distribution is bimodal, not noisy.

Operationally: **on a lossy path, `reconcile_interval` is the latency knob, not RTT.** 1 s of it
dwarfs the 50 ms top of the RTT sweep by 20×.

Every lane is seeded (`Seed::DEFAULT`, printed with the results) and the impairment stream per
directed link replays exactly; what does not replay bit-for-bit is task interleaving on a
multi-threaded runtime, so read these as reproducible to within the usual benchmark noise. Like
every target here, the lane stays out of CI — compile-checked only.

### Results: a fixed-capacity sketch's fragmentation exposure vs the real chain ([#7](https://github.com/adriendellagaspera/experiments/harness/issues/7))

`experiments/harness/tests/sketch_exchange_fragmentation_under_loss.rs` — a `cargo test`, not a bench,
since the headline case needs a real `n = 10⁶` store and a Monte Carlo loss sweep rather than
Criterion's sampling loop: `cargo test --release -p experiments/harness --test
sketch_exchange_fragmentation_under_loss -- --ignored --nocapture` reproduces it.

[#13](https://github.com/adriendellagaspera/experiments/harness/issues/13)'s "a sketch sends fewer
datagrams, so it lowers the probability an exchange loses one" compares the wrong unit: **IP
delivers a fragmented datagram all-or-nothing**, and a sketch blob is one large datagram, not one
small one. At `n = 10⁶, d = 1` scattered, `FixedFanOut(b = 16)` — the real, measured chain,
`devkit::protocol_cost::reconcile`, not the counted table's derivation — costs **8 messages, 8
datagrams, 8 fragments**, 3 554 B refinement (agrees with
[#9](https://github.com/adriendellagaspera/experiments/harness/issues/9)'s independently-measured 3 554 B
at `τ = 256`). A sketch's "offer, miss, updates" exchange is modeled at an IBLT cell of `count: i64`
(8 B) + a 256-bit key-sum digest (32 B) + an 8 B check-sum = 48 B/cell — reproducing #13's own "256
cells ≈ 12 kB" and its 64-cell row exactly (no sketch implementation exists to measure instead; #5
is blocked on this issue) — and priced through the same `MAX_DATAGRAM_PAYLOAD`/
`MTU_FRAGMENT_PAYLOAD` ceilings:

| exchange | messages | datagrams | fragments |
|---|---:|---:|---:|
| RBSR, `n=10⁶, d=1` (measured) | 8 | 8 | **8** |
| sketch, cap=64 (≈3 KiB) | 3 | 3 | **5** |
| sketch, cap=256 (≈12 KiB) | 3 | 3 | **11** |

Completion probability is drawn from `gossip::netem`'s real seeded loss model one fragment at a
time (`NetemTransport`/`Impairments`, the same instrument `probe_link` calibrates against above),
not composed as `(1 − p)^fragments`, 4 000 trials per point:

| loss | RBSR (8 frags) | sketch cap=64 (5 frags) | sketch cap=256 (11 frags) |
|---|---:|---:|---:|
| 0.1 % | 0.9918 | 0.9960 (better) | 0.9918 (tied) |
| 1 % | 0.9265 | 0.9507 (better) | 0.9030 (**worse**) |

A finer sweep (0.10 %–2.90 %, step 0.20 pp) against the 256-cell sketch shows the gap widening
monotonically with loss, from Δ = −0.0035 at 0.10 % to Δ ≈ −0.05…−0.07 by 2.5–2.9 % — the 256-cell
row is worse at *every* nonzero loss rate tested, not past some threshold.

**That is the correction to state, not a loss-rate crossover: fragment count does not depend on
loss rate, so a fixed 11-vs-8-fragment comparison has no crossover in loss space — it is worse (or
better) at every `p > 0`, and loss only sets how fast the gap widens.** The real crossover is in
**capacity**: solving `frags(cap × 48 B) + 2 ≤ 8` for this `n = 10⁶, d = 1` baseline gives `cap ≤
184` cells as the break-even (184 ties RBSR's 8 fragments exactly; 185 costs a ninth). 256 cells —
the number [#13](https://github.com/adriendellagaspera/experiments/harness/issues/13) has been quoting
throughout — sits on the wrong side of that line at this operating point; 64 does not.

**Ranking against reconcile-rs#23, and the decision.** This is #7's own first outcome, not its
second: *"the RTT term binds on the paths this crate targets → reconcile-rs#45 proceeds, and
capacity is chosen against the fragmentation column, not only against `d`."* The loss term does
not bind unconditionally — a viable capacity (≤ 184 cells at this baseline) stays no worse than the
real chain at every measured loss rate — so this is not the kill criterion "#7 finds the loss term
binds" from #13's own table, and reconcile-rs#23 does not need to go first. What is corrected is
narrower and still load-bearing: **256 cells is not a safe default capacity, and #6's cutover
threshold cannot be chosen from `d` alone** — it has to clear
`frags(cap × cell_bytes) + 2 ≤ RBSR_fragments(n, d)`, a bound that depends on the real chain's own
measured fragment count at the operating point, not a fixed number quoted from one example.
[#13](https://github.com/adriendellagaspera/experiments/harness/issues/13)'s body should read "capacity
chosen against fragmentation" rather than "256 cells" wherever it still says the latter.

Same reproducibility note as the loss lane above: `Seed::DEFAULT`, deterministic given the same
link and endpoints, out of CI (compile-checked only) like every instrument in this file — this one
lives in the `tests/` gate instead of a bench target, so its fast assertions (the sizing arithmetic
and a small-`n` sanity check) do run in `cargo test --workspace`; only the `n = 10⁶` headline case
above is `#[ignore]`d.

## `service_reconcile_rtt`: the refinement chain under injected RTT

```sh
# Run from reconcile-rs:
cargo bench --bench replicated_map -- service_reconcile_rtt
cargo bench --bench replicated_map -- 'service_reconcile_rtt/n=1000000/d=1000'
```

Answers [#461](https://github.com/Akvize/reconcile-rs/issues/461), closing the gap the lane above
states outright: every RTT lane there is `cold_sync_rtt` (d = n, the empty-peer case, one round
trip) or `gossip_propagation_rtt` (one hop); neither exercises the O(log n) *refinement chain* the
counted table below models. This lane does: composes `ReplicatedMap::new_with_transport`,
`gossip::netem::NetemTransport` and the runtime benchmark's `rtt_sweep()` with `just_insert`/`just_remove` — the `reconcile_internal_testing` seams
that build a genuine content difference without a timestamp race, which is why the canonical lane lives in `reconcile-rs`'s `replicated_map` target.

Per `(n, rtt)`: one peer loads the `n`-entry corpus, the other starts empty and pulls it via
cold-sync (`service_reconcile_rtt`'s own docs explain why — an earlier design where both peers
loaded independently hit a real `NetemTransport`-specific non-convergence at larger `n`, unrelated
to the protocol). Once settled, every sample `just_remove`s the `d` chosen keys (scattered or
clustered — the same two layouts `benches/protocol.rs` sweeps), triggers a round, polls until the
peer reflects the removal, then `just_insert`s them back and repeats — so a sample times one full
remove-then-restore cycle, two content differences, not one. `d = 0` has no keys to remove; its
round is timed via a receive-counting transport instead, since a root-fingerprint match makes the
responder reply with nothing at all (`RecvCountingTransport`'s own docs).

### Results: `d = 0`, the baseline every sketch must not regress

| n | rtt=0ms | 0.1 ms | 1 ms | 10 ms | 50 ms | delta vs rtt=0 |
|---:|---:|---:|---:|---:|---:|---|
| 10³ | 4.8 µs | 55.4 µs | 507 µs | 5.01 ms | 25.01 ms | **+0.50 × RTT** |
| 10⁴ | 6.5 µs | 55.5 µs | 508 µs | 5.01 ms | 25.02 ms | **+0.50 × RTT** |
| 10⁵ | 5.0 µs | 56.0 µs | 508 µs | 5.01 ms | 25.02 ms | **+0.50 × RTT** |
| 10⁶ | 8.3 µs | 55.5 µs | 509 µs | 5.01 ms | 25.03 ms | **+0.50 × RTT** |

Flat at exactly **+0.50 × RTT across all four decades of `n`** — one one-way hop, not a round
trip, matching `RecvCountingTransport`'s docs (a root match makes the responder answer with
nothing, so only the initiator's own send is observable) and the injected-RTT lane's own
`gossip_propagation_rtt` row above, also exactly one hop. The `rtt=0ms` column is harness-floor
noise (single-digit µs, like the calibration table above) and does not grow with `n` either. This
is the number any sketch replacing full refinement is judged against: it cannot be *worse* than
one silent hop, whatever `n` is.

### Results: delta vs RTT, per `(n, d)` — the clean cells

Slope of measured wall clock against RTT (endpoints `rtt=0ms`/`rtt=50ms`), same format as the
table above, for every `(n, d, clustering)` cell whose ten samples agree to within 30% at every
RTT — see below for the cells this excludes and why. Every `d > 0` cell here is one full
remove-then-restore cycle — two content-difference resolutions, not one — so it is not directly
the counted table's per-pull unit; see the next section for that comparison:

| n | d = 1 (scattered) | d = 10 (scattered/clustered) | d = 100 (scattered/clustered) | d = 1000 (scattered/clustered) |
|---:|---:|---:|---:|---:|
| 10³ | +5.01 × RTT | +5.01 × RTT / +5.01 × RTT | +5.01 × RTT / +5.01 × RTT | +5.00 × RTT / +4.99 × RTT |
| 10⁴ | +7.01 × RTT | +7.02 × RTT / +7.01 × RTT | +7.04 × RTT / +7.01 × RTT | past capacity (below) / +7.00 × RTT |
| 10⁵ | +7.11 × RTT | +7.11 × RTT / +7.06 × RTT | past capacity (below) / +7.13 × RTT | past capacity (below) / +7.05 × RTT |
| 10⁶ | not stable past rtt=1ms (below) | | | |

**Every clean coefficient is flat in `d`, at a fixed `n`** — 10³ costs ≈ 5.0 × RTT whether `d` is 1
or 1000, scattered or clustered; 10⁴–10⁵ cost ≈ 7.0–7.1 × RTT the same way. This is the O(log n)
model directly confirmed: round trips depend on the *depth* the refinement chain must descend to
isolate the difference, not on how many differences there are, provided the differing ranges still
resolve within the chain's normal recursion. Going from 10³ to 10⁴ costs **+2.0 × RTT** more, and
10⁴ to 10⁵ essentially nothing further (+0.0–0.1 × RTT) — see below for how that steps against the
counted table.

### Results: measured vs counted round trips

Adding a measured column to the counted table above (`d = 1`, one element missing, `b = 16`).
This lane's own unit is *two* content-difference resolutions (remove, then restore) rather than
the counted table's one pull, so the measured round-trip count halves what the raw coefficient
above states, to compare like with like:

| quantity | n = 10³ | 10⁴ | 10⁵ | 10⁶ |
|---|---:|---:|---:|---:|
| round trips, counted (`⌈log₁₆ n⌉`-derived, existing table) | 3 | 3 | 3 | **4** |
| round trips, measured (`d=1` scattered coefficient ÷ 2, this lane) | 2.5 | 3.5 | 3.6 | not stable (below) |
| wall clock at 50 ms RTT, measured | 250.4 ms | 350.6 ms | 355.8 ms | not stable (below) |

**The measured chain steps up a size earlier than the counted one predicts, then flattens where
the counted model still expects it to hold flat.** The counted table's *round trips* row (not its
`⌈log₁₆ n⌉` depth row, which already steps at every size below 10⁶) holds at 3 from n = 10³
through 10⁵, stepping to 4 only at 10⁶; measured round trips already sit at 3.5–3.6 by 10⁴–10⁵ — a
full round trip's worth of difference the counted model does not place until a decade or two
later, then barely move between 10⁴ and 10⁵ where the counted model is also flat. Read
literally, #185's own "4 round trips ≈ 200 ms at n = 10⁶" figure is **not confirmed as stated**:
every size below 10⁶ already costs more measured round trips than the counted model's flat "3"
implies, and 10⁶ itself is not a stable single number at all (below) — measured against a moving,
sometimes much larger, target rather than a clean step to 4. This does not *contradict* `POSITIONING.md`
§1.3's "worst family on latency" takeaway — it reinforces it: the O(log n) round-trip count the
counted model predicts is a floor this lane already exceeds below 10⁶, and at 10⁶ the round-trip
count stops being a fixed function of `n` at all (below), which is a strictly worse property for
the latency-sensitive profile than the counted model states, not a better one. No re-derivation of
the takeaway itself is needed; the counted table's specific numbers are now superseded by this
lane's measurements as the more accurate source.

### Results: past a fixed-capacity sketch — `d` scattered widely enough to defeat range compression

Some `(n, d)` scattered cells do not converge on one round trip the way every cell above does. A
standalone repro (bypassing Criterion, with tracing) confirmed this is real, not a harness
artifact: `d = 1, 10` always converge on the first `start_reconciliation` at every `n` measured,
and the *clustered* layout at the same `d` (one contiguous block, whatever its size) also always
converges in one round — only a **scattered** difference past some size, relative to `n`, plateaus
part-way and needs a handful of explicit retriggers to fully resolve
(`trigger_and_converge`'s own docs). Each retrigger is a real, counted round trip, so the total
*is* the measurement, and the threshold moves with `n`: `d = 1000` scattered is the only affected
cell at n = 10⁴, but `d = 100` scattered is *also* affected by n = 10⁵ — consistent with a
fixed-capacity sketch (#185's own "past a 256-cell sketch's capacity" framing for this grid)
being overwhelmed at a roughly constant *count* of scattered differences relative to the tree's
leaf width at that `n`, not at a constant fraction of `n`:

| n, d (scattered) | rtt=0ms | rtt=0.1ms | rtt=1ms | rtt=10ms | rtt=50ms |
|---|---:|---:|---:|---:|---:|
| 10⁴, d=1000 | 60.0 s | 30.0 s | 30.0 s | 30.1 s | 30.3 s |
| 10⁵, d=100 | 30.0 s | 9.2 ms | 15.9 ms | 83.9 ms | 368.2 ms |
| 10⁵, d=1000 | 270.0 s | 105.0 s | 60.0 s | 28.6 s | 1.9 s |
| 10⁶, d=100 | 30.7 s | 303.6 ms | 111.6 ms | 346.4 ms | 969.8 ms |
| 10⁶, d=1000 | 372.5 s | 126.9 s | 99.0 s | 72.0 s | 2.7 s |

**Higher RTT often costs *less* wall clock here, the opposite of every clean cell above.** `d =
1000` scattered drops monotonically as RTT rises at every `n` — 270 s down to 1.9 s at n = 10⁵.
This is not the network getting faster: each retrigger only fires after a fixed poll deadline
elapses on the *previous* one, so a higher injected RTT gives that same fixed deadline more real
time for the in-flight round to make progress on its own before the deadline gives up and pays for
another full retrigger — fewer retriggers, even though each round trip now individually costs
more. The `d = 100`/n = 10⁵ row shows the same effect over a narrower range (30 s at `rtt=0` down
to single-digit milliseconds by `rtt=0.1ms`, since one retrigger avoided is a much larger fraction
of a cheaper baseline). Cost here is dominated by *how many retriggers are needed*, not by RTT
itself, unlike every cell in the tables above.

### Results: `n = 10⁶` — the round-trip count stops being a fixed function of `n`

Below 10⁶, every `(n, d, clustering)` cell not in the table above reproduces to within 30% across
all ten samples at every RTT (`clean` in the second table). At `n = 10⁶`, that stops being true
for most cells past `rtt = 1 ms`, including ones with no scattered-capacity issue at any smaller
`n` — `d = 1` scattered, `d = 10` clustered, `d = 1000` clustered all included, not just the
already-flagged `d = 100`/`d = 1000` scattered cells:

| n=10⁶, d (clustering) | rtt=0ms | rtt=0.1ms | rtt=1ms | rtt=10ms | rtt=50ms |
|---|---:|---:|---:|---:|---:|
| d=1 (scattered) | [0.18, 1.11, 2.97] ms | [0.91, 5.04, 13.3] ms | 7.40 ms (tight) | [70.8, 351, 913] ms | [351, 809, 1490] ms |
| d=10 (clustered) | [0.40, 2.58, 6.92] ms | [1.18, 14.7, 41.9] ms | 7.68 ms (tight) | 71.3 ms (tight) | [352, 1235, 2435] ms |
| d=1000 (clustered) | 8.55 ms (tight) | 9.14 ms (tight) | [13.8, 73.1, 192] ms | 77.9 ms (tight) | [358, 903, 1991] ms |

(bracketed cells are `[min, mean, max]` across the ten samples; "tight" cells still agree to
within a few percent.) Every affected cell is bimodal, not noisy in the ordinary sense — some
samples take the fast, single-round path every smaller `n` takes unconditionally, others need one
or more retriggers, and which one a given sample lands on is not predictable from `(n, d,
clustering, rtt)` alone at this size. The likely mechanism: this lane's fixed, wall-clock poll
deadline (`converge`'s docs) was sized against smaller-`n` round costs; at `n = 10⁶` a single
genuine round trip's own processing cost (larger aggregates, larger comparison batches) already
consumes enough of that budget that ordinary scheduling variance occasionally tips a sample into
needing a retrigger where a smaller `n` never would. **This is itself the headline result for
`n = 10⁶`: not a specific worse coefficient, but the loss of a stable one at all** — a strictly
harder property for the latency-sensitive profile to reason about than the counted table's flat
round-trip count of 4 implies, and further evidence against, not for, the counted model's numbers
being the load-bearing ones going forward (previous section).

### Determinism

Same seed, same discipline as the lane above (`Seed::DEFAULT`, `NetemTransport`), plus its own:
the retry loops (`MAX_BUILD_ATTEMPTS`, `MAX_ROUND_RETRIGGERS`) poll for actual convergence — never
a fixed sleep. The `n = 10⁶` bimodality above is not scheduler noise in the sense of being
irreproducible: repeated runs at a fixed `(n, rtt)` land on the same *mix* of fast/retriggered
samples, and the "past a fixed-capacity sketch" table's numbers reproduce to within a few percent
run over run — the nondeterminism is in which of the ten Criterion samples takes the slow path
this run, not in whether the underlying behavior recurs.

## The `protocol` benchmark

```sh
cargo bench --bench protocol            # cost tables + the timed drive loops
cargo bench --bench protocol -- --quick

# The printed tables only (no timed group): give Criterion a filter that matches no benchmark id.
cargo bench --bench protocol -- 'no_such_benchmark'
```

`reconciliation_cost` prints, for each `(policy, n, d, clustering)`, **total wire bytes** — the
refinement traffic plus the values the IDLIST outcomes ship — at four value payload sizes
(8 / 64 / 512 / 4096 B), then the breakdown under it: refinement bytes (bincode, the same encoder
the real transport uses), advertised `RangeAggregate`s, one-way messages, datagrams, IP fragments,
the largest logical refinement round (not necessarily one datagram), the IDLIST ranges and the elements they ship, and the local
`Aggregate`/`Rank`/`Select` counts summed over both peers. Deterministic, so it is printed rather
than timed — like `system`'s `memory_footprint`, whose payload-size axis it borrows. The timed
`reconciliation_drive` group alongside it measures the local CPU cost of driving a whole run per
policy, the quantity arXiv:2603.19820 models as `T_loc`.

**One unit, because the two halves are traded against each other.** A policy that splits less
advertises fewer ranges but reaches its IDLIST cutoff on wider ranges, and every enumerated element
is a *value* on the wire — nearly all of them elements the peer already holds. Reported in two
units, that trade is unreadable: at n = 10⁵, d = 100 scattered the paper's `t`=32 policy saves 46 %
of the refinement bytes against the default and ships **5 036 elements instead of 100**, which looks
like a win in the first column and a loss in the second. Summed, it is 1.52× the default's bytes at
8-byte values and 36× at 4 KB. The message column stays separate on purpose: no byte total prices a
round trip, and *this* target runs at RTT ≈ 0 — weigh it at the measured rate of **1.00 × RTT per
round trip** ([#280](https://github.com/Akvize/reconcile-rs/issues/280) — the injected-RTT lane
above).

One drive prices every value size: the payload is not read by any SKIP/IDLIST/SPLIT decision, so
only the per-element price moves with it. The harness checks that rather than assuming it —
`payload_size_does_not_move_the_trace` reconciles the same case over `u64`, 8-byte and 4 KB values
and compares every decision before a table is printed.

**Why it exists.** RBSR's published bounds — `O(d log n)` communication, `O(log n)` sequential
rounds — assume the fixed branching factor `b` of the paper's Algorithm 2. `rbsr` makes the fan-out
a swappable `RefinementPolicy`, so what a given configuration costs is a measurement rather than a
quotation: this target supplies it. Interpretation of the numbers below lives in `POSITIONING.md` §2.2; the
decisions they drove are on issues [#257](https://github.com/Akvize/reconcile-rs/issues/257) and
[#315](https://github.com/Akvize/reconcile-rs/issues/315), and in `rbsr/src/policy.rs`'s own rustdoc.

Refinement bytes and one-way messages for `SqrtFanOut` (`√m`) against the default `FixedFanOut(16)`
and the paper's `t`=32 enumeration threshold, same harness, `d` = 1, one element missing (refinement
traffic only — the first two ship one ≈33 B element on top of it; `t`=32 ships 7–51 and is why the
totals below differ):

| n | `√m` refine B | `√m` msgs | `b`=16 refine B | `b`=16 msgs | `t`=32/`b`=16 refine B | msgs |
|---:|---:|---:|---:|---:|---:|---:|
| 10³ | 2 041 | 6 | 1 701 | 6 | 1 476 | 4 |
| 10⁴ | 5 395 | 8 | 2 195 | 6 | 1 520 | 4 |
| 10⁵ | 16 553 | 6 | 2 789 | 6 | 2 294 | 5 |
| 10⁶ | **53 046** | 8 | **3 834** | 8 | **3 246** | 6 |

For a **single** missing element in a 10⁶-entry store, `b = 16` spends 3.8 kB over 78 ranges against
`SqrtFanOut`'s ~53 kB over ~1 048 ranges (~×3.2 per decade of n on the `√m` bytes), at the *same* 8
one-way messages — log₁₆ 10⁶ ≈ 5 is already the iterated-square-root depth at that size, so the
`Θ(log log n)` round advantage `√m` offers asymptotically does not appear below n ≈ 10¹² (the message
counts above are identical at n = 10⁶: 8 and 8). Local query cost scales with it: ~13× the
`Aggregate`/`Rank`/`Select` queries at n = 10⁶ (2 094/2 092/1 040 against 155/152/70). The gap closes
as `d` grows and scatters — at `d` = 100 over 10⁶ elements the two are within 7 % (270 940 B against
253 153 B), because ~√n ranges stop being overhead once the difference genuinely needs that many;
`√m` is worst exactly in the small-`d` regime RBSR exists for.

The timed `reconciliation_drive` group widens the gap further, in a column no RTT caveat touches:
2.10 ms under `√m` against 45.0 µs at `b` = 16 (≈47×) at n = 10⁶, 460 µs against 25.2 µs at 10⁵, and
only 1.6× apart at 10³ — steeper than the query-count ratio because a `√n` fan-out's queries are
individually dearer (wide `Aggregate`s, spread-out `Select`s touch far more of the tree than a narrow
descent).

The widest single round at d = 1 is 50 781 B (inside the 65 507-byte datagram ceiling, ~35 IP
fragments at a 1500-byte MTU, any one of which loses the whole round); at d = 100 over 10⁶ elements
it reaches 160 908 B over 3 300 ranges, i.e. three datagrams and ~189 fragments —
`send_messages_paced` chunks past the ceiling rather than failing.

`fan_out_sweep` then varies the branching factor alone (`FixedFanOut`, `b` = 2…256). Bytes and local
work follow `b / ln b`, which is derived rather than fitted: refinement advertises `b` aggregates per
level over `log_b n = ln n / ln b` levels, so `refinement ≈ aggregate_size · ln n · (b / ln b)`, whose
derivative `(ln b − 1)/(ln b)²` vanishes at `b = e ≈ 2.718` — `b` = 3 over the integers, with `b` = 2
and `b` = 4 tied above it (2.885 each). Earlier sweeps ran powers of two and stepped over it;
`FAN_OUTS` now carries `b` = 3. At `n = 10⁶`, `d = 1` scattered:

| `b` | 2 | **3** | 4 | 8 | 16 | 32 | 64 | 128 | 256 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| refinement B | 2 061 | **1 868** | 1 960 | 2 613 | 3 834 | 5 021 | 9 668 | 15 856 | 25 880 |
| ranges, measured | 42 | **38** | 40 | 53 | 78 | 101 | 196 | 320 | 520 |
| `b · ln n / ln b`, predicted | 39.9 | **37.7** | 39.9 | 53.2 | 79.7 | 127.6 | 212.6 | 364.5 | 637.8 |
| one-way messages | 22 | 14 | 12 | 10 | 8 | 6 | 6 | 6 | 6 |

`b` = 3 is the minimum, 4.7 % under `b` = 4 and 9.4 % under `b` = 2. The range row is the sharper
test — the model predicts an absolute count with **no fitted constant** — and it holds to ~2 ranges
through `b` = 16. Past that it over-predicts, because the descent bottoms out before the last level
can use its full fan-out; that truncation is also why the implied byte constant drifts (≈ 680 at
`b` = 3…8, ≈ 560 at 256) while the small-`b` end does not.

One-way messages fall as `log_b n` until they hit a floor — 6 at `n = 10⁶`, reached at
`b = 32` — past which extra `b` is paid for and buys nothing. The widest single round grows linearly
in `b` and is the hard ceiling: at `n = 10⁵`, `d = 100` it already exceeds one datagram at `b = 16`.
Across every measured `(n, d, clustering)`, `b = 16` is the only swept value never worse than `√m` on
rounds, while spending 13.8× fewer bytes, ~45× less `T_loc` and a 63× narrower widest round than
`√m`; `b = 4` wins on bytes and CPU but costs two round-trips — break-even at an RTT of ≈8 µs at
1 Gb/s, i.e. only worth it when the "network" is in-process. It is the bandwidth-over-latency value
to reach for.

`threshold_sweep` does the same for the other Algorithm 1 parameter, the enumeration threshold
(`EnumerateBelowThreshold`, `t` = 1…256, `b` held at 16), against `FixedFanOut(16)` — today's
default, and the only baseline the question "should `t` exist at all?" can be answered against. Each
row carries its total as a ratio to that baseline at every value size, plus its **break-even**: the
element price at which the refinement it saves would exactly pay for the elements it ships. Read
that against the element price printed above the tables — the same unit, measured the same way. `t`
is a step function, not a dial: a range's span walks the ladder `n / b^k`, so every `t` between two
rungs picks the same rung and costs exactly the same. At n = 10⁵, d = 100 scattered, `t` = 32 saves
46 % of the refinement bytes (88 817 B against 162 993 B) and ships 5 036 elements instead of 100 —
a trade that needs an element to cost ≤ 15 B, where the cheapest this wire format can carry is 30 B
(a varint key, a 19-byte `Timestamp`, two framing bytes, then the payload). Totalled: 1.52× the
default's bytes at 8-byte values, 36× at 4 KB. No swept `t` saves more than 4 % anywhere, all of it
at 8-byte values, and none beats the default at 64 B or above. The outcome, and why the default did
not move: [#315](https://github.com/Akvize/reconcile-rs/issues/315).

**That is the byte column; the other two say the opposite**
([#468](https://github.com/Akvize/reconcile-rs/issues/468)). Each `t` row also carries the
refinement half alone against the same baseline, the one-way-message delta, the value size at which
the two totals cross, and the RTT at which the round trips it saves outweigh the bytes it adds
(`refinement_against`, `value_crossover`, `rtt_break_even`). At `t` = 2b = 32 — Negentropy's own
cutoff, `t` = 31 measuring identical to it — over the eight swept `(n, d)`:

| `n` | `d` | refine | ranges | msgs | wins on total bytes below `V` = | RTT break-even @1 Gb/s, `V` = 8 B … 4 KB (`any` = wins at every RTT, costing no extra bytes) |
|---:|---:|---:|---:|---:|---:|---|
| 10³ | 1 | 0.87× | 0.87× | 6 → 4 | 13.5 B | any, ≥0.0, ≥0.0, ≥0.2 ms |
| 10⁴ | 1 | 0.69× | 0.69× | 6 → 4 | — (−10.5 B) | ≥0.0, ≥0.0, ≥0.2, ≥1.6 ms |
| 10⁵ | 1 | 0.82× | 0.82× | 6 → 5 | 0.8 B | ≥0.0, ≥0.0, ≥0.2, ≥1.3 ms |
| 10⁵ | 10 | 0.69× | 0.68× | 8 → 5 | — (−9.8 B) | ≥0.0, ≥0.2, ≥1.4, ≥11.0 ms |
| 10⁵ | 100 | 0.54× | 0.55× | 8 → 5 | — (−9.7 B) | ≥0.5, ≥1.9, ≥13.8, ≥108.1 ms |
| 10⁶ | 1 | 0.85× | 0.85× | 8 → 6 | 3.4 B | ≥0.0, ≥0.0, ≥0.1, ≥0.7 ms |
| 10⁶ | 10 | 0.75× | 0.75× | 8 → 6 | 1.8 B | ≥0.0, ≥0.1, ≥1.2, ≥9.8 ms |
| 10⁶ | 100 | 0.68× | 0.68× | 8 → 6 | 1.8 B | ≥0.1, ≥1.5, ≥12.1, ≥96.6 ms |

Read it as three findings:

| | |
|---|---|
| refinement and rounds | `t` = 2b wins **everywhere**: 0.54–0.87× the refinement bytes, 0.55–0.87× the ranges, and 1–3 fewer one-way messages at every `(n, d)` — one descent level, the gap the Negentropy anchor below shows |
| the `V` crossover | a figure, not a verdict: 13.5 B at `n` = 10³, ≤ 3.4 B at `n` ≥ 10⁵, negative at three of eight cases. Only a set-shaped or single-byte-valued store wins on total bytes, which is the band #315 was already reading when it found the smallest value size closest |
| the RTT crossover | 0.0–0.5 ms at 8-byte values, so any WAN link flips it; at 4 KB it is `d` that decides — 0.2–1.6 ms at `d` = 1 against 96–108 ms at `d` = 100 |

Both crossovers assume a lossless link at line rate and count two one-way messages as one round trip
(the measured 1.00 × RTT, above). `LINK_RATE_BYTES_PER_MS` states the rate; nothing here measures a
link. #315's recommendation therefore stands **for total bytes and becomes conditional otherwise** —
`EnumerateBelowThreshold` for small-value, small-`d`, RTT-bound deployments, the default elsewhere;
the caller-facing form is on `EnumerateBelowThreshold`'s rustdoc.

The split rule itself is pinned by unit tests in `rbsr/src/protocol.rs`
(`default_split_fan_out_is_constant_at_sixteen`, `sqrt_fan_out_is_still_the_square_root_of_the_range_size`,
`split_children_partition_the_parent_range`), so changing the *default* fails CI rather than
silently changing every cluster's bandwidth profile — this benchmark quantifies such a change, it
does not guard it. `split_children_partition_the_parent_range` is policy-independent and must hold
under any fan-out rule.

### The Negentropy anchor

The one column here not produced by `reconcile-rs`
([#362](https://github.com/Akvize/reconcile-rs/issues/362)). `print_negentropy_anchor` prints it on
every run of this target, reading `../fixtures/negentropy-counted.tsv`, whose header carries the
provenance and the generating command. Refinement columns only; `b` = 16 and `Clustering::Scattered`
both sides.

| `n` | `d` | B | ranges | msgs | B/range | Negentropy B | ranges | msgs | B/range | ratio |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 10³ | 1 | 1 577 | 39 | 6 | 40.44 | 608 | 32 | 4 | 19.00 | 2.13× |
| 10⁴ | 1 | 2 031 | 49 | 6 | 41.45 | 927 | 48 | 4 | 19.31 | 2.15× |
| 10⁵ | 1 | 2 577 | 61 | 6 | 42.25 | 943 | 48 | 4 | 19.65 | 2.15× |
| 10⁶ | 1 | 3 554 | 78 | 8 | 45.56 | 1 278 | 64 | 6 | 19.97 | 2.28× |
| 10⁶ | 100 | 235 365 | 5 247 | 8 | 44.86 | 67 853 | 3 472 | 6 | 19.54 | 2.30× |

(Re-measured 2026-08-22 after [#382](https://github.com/Akvize/reconcile-rs/issues/382) — `Fingerprint` moved
from a 36 B varint-coded encoding to a raw 32 B one, 4 B/range less on this side only.)

What it supersedes, and what it leaves open:

| | |
|---|---|
| superseded | `POSITIONING.md`'s "44 B/range against 16 B" — both figures were payload-only, and a range does not travel without its bound and framing |
| our per-range cost | not a constant: 40.4 B → 45.6 B as `n` grows, since `KeyRange` bounds cost more varint bytes as keys grow |
| where the gap is | bound encoding is ~5 B here against ~4 B there, so the gap is summary width — §2.1's trade, confirmed |
| explained | at equal `b` = 16 the two descents differ (64 ranges / 6 msgs against 78 / 8), so the total gap is 2.78× against 2.28× per range. It is the **enumeration cutoff**, not the fan-out: [#468](https://github.com/Akvize/reconcile-rs/issues/468), below |

Commensurability, before summing anything against the totals above:

| | |
|---|---|
| compares | the fixture's `fp_ranges`/`fp_bytes` (mode=1 ranges) against `Cost::ranges`/`Cost::refinement_bytes` |
| does not compare | the IDLIST halves — a Negentropy element is a timestamp + a 256-bit id, ours is a key + an HLC + a value |
| instance | modelled, not shared: item `k` gets timestamp `k` and a deterministic 32-byte id, so both refine over the same logical ordering of the same `n` items |

#### The descent gap is their enumeration cutoff ([#468](https://github.com/Akvize/reconcile-rs/issues/468))

Negentropy's `splitRange` carries one size-based cutoff — `numElems < 2 · buckets` ships an IdList,
anything wider splits into `buckets` children — where `shared_cutoffs` has none, so `FixedFanOut`
keeps descending where Negentropy has already stopped. In this crate's vocabulary that rule is
`EnumerateBelowThreshold` at `t = 2b − 1` = 31, one rung below the paper's `t = 2b` = 32 and
measured identical to it at every swept `(n, d)` (the span walks the `m / b^k` ladder, so neither
`t` picks a different rung). `print_negentropy_anchor` drives that policy beside the default on
every anchor row; the third column is what the first two are being compared through:

| `n` | `d` | default `b`=16 | under their cutoff (`t`=31, `b`=16) | Negentropy |
|---:|---:|---:|---:|---:|
| 10³ | 1 | 39 r / 6 msgs | 34 r / 4 msgs | 32 r / 4 msgs |
| 10⁴ | 1 | 49 r / 6 msgs | 34 r / 4 msgs | 48 r / 4 msgs |
| 10⁵ | 1 | 61 r / 6 msgs | 50 r / 5 msgs | 48 r / 4 msgs |
| 10⁶ | 1 | 78 r / 8 msgs | 66 r / 6 msgs | 64 r / 6 msgs |
| 10⁶ | 100 | 5 247 r / 8 msgs | 3 573 r / 6 msgs | 3 472 r / 6 msgs |

Adopting their cutoff closes the message gap outright on four of the five rows (the 10⁵ row lands
one message above) and 71–94 % of the range gap; at 10⁴ it goes past them. What it costs is the
other half of the same trade — 7, 51, 21, 21 and 3 048 enumerated elements against the default's 1,
1, 1, 1 and 100 — which is exactly what [#315](https://github.com/Akvize/reconcile-rs/issues/315)
priced and rejected, and what the `threshold_sweep` section above now reports in the two columns
that trade decides. The remaining per-range 2.28× is summary width, unchanged.

`../../scripts/experimental/negentropy-drive.js` parses Negentropy's emitted messages against its
[protocol v1 spec](https://github.com/hoytech/negentropy/blob/master/docs/negentropy-protocol-v1.md)
rather than its internals, and asserts it consumes each message to the byte — a conformance check on
our reading of that spec. Regeneration is manual and out of band, the cost #362 accepts on the
record; nothing in CI reads the fixture, and a missing one prints a skip, never a failure.

### `count_delta_sweep`: fan-out from the observed count delta ([#12](https://github.com/adriendellagaspera/experiments/harness/issues/12))

`CountDeltaFanOut(floor = 16, cap)` widens to `clamp(|local.size() − remote.size()| + 1, floor, cap)`
and degrades to `floor` — today's default — where the delta reads zero. `size()`-derived only: a
width read off the fingerprint would make the compared index set oracle-dependent and void the union
bound RBSR's soundness rests on, so the count is not the preferred signal but the only admissible
one.

Four cells, each row against `FixedFanOut(16)`'s totals at `V = 8 B`, plus `SqrtFanOut` for the
regime it wins. The `updated` cell is built by `store_updated`: same key set, different values, so
counts are equal on both sides at every level.

| cell | default | `sqrt` | count-delta cap=64 | count-delta cap=256 |
|---|---:|---:|---:|---:|
| needle, n = 10⁶, d = 1 scattered | 3 588 B / 8 msgs | 13.63× / +0 | **1.00× / +0** | **1.00× / +0** |
| broad, n = 10⁵, d = 100 scattered | 155 169 B / 8 msgs | 0.91× / +0 | **0.95× / −2** | 1.07× / −2 |
| n = 10⁵, d = 100 clustered | 7 417 B / 8 msgs | 2.63× / +2 | 1.53× / −2 | 1.71× / −2 |
| n = 10⁵, d = 100 **updated** | 153 873 B / 7 msgs | 0.92× / +0 | **1.00× / +0** | **1.00× / +0** |

Three things the table settles.

**The needle case is untouched, exactly.** At `d = 1` the delta is 1, which clamps to the floor, so
every decision is the default's and the totals are byte-identical — not "within noise". That is the
no-regression box, discharged by construction rather than by tolerance.

**The blind spot is total, and measured end to end rather than argued.** The `updated` row is
byte-identical to the default at every payload size, through the real driver. An LWW write to an
existing key replaces a value instead of adding one, so it moves no count anywhere in the tree, and
the policy sees nothing. This is the same predicate that makes a range skippable at all — a range
whose peers hold different counts is never skipped — so the regime the heuristic cannot see is the
regime where the exact-count guarantee has already run out.

**The cap is not a safety valve, it is the tuning knob, and the byte column is not monotone in it.**

| cap | scattered | clustered |
|---:|---:|---:|
| 24 | 0.93× / +0 msgs | 1.11× / +0 |
| 32 | 0.99× / −2 | 1.34× / +0 |
| **48** | **0.94× / −2** | 1.23× / −2 |
| 64 | 0.95× / −2 | 1.53× / −2 |
| 96 | 1.01× / −2 | 1.81× / −2 |
| 128 | 1.07× / −2 | 1.71× / −2 |
| 256 | 1.07× / −2 | 1.71× / −2 |

Past the point where the delta is separated, a wider fan-out buys no further round and costs ranges:
the scattered column turns at 48 and is 13 points worse by 128. The two cells also disagree on where
the optimum sits — clustered wants the smallest cap that still saves the rounds — so the knob has to
be retuned per difference shape, which is exactly what the policy was supposed to stop doing.

**The verdict, and it is a refusal.** At its best cap the policy buys **2 round trips out of 8** for
−6 % bytes on scattered deletions and +23 % on clustered ones, and **nothing at all** on the
update-shaped divergence that is ordinary LWW steady state. The regime it does see best — a large
count divergence after a partition or a missed run of inserts — is the one
[#13](https://github.com/adriendellagaspera/experiments/harness/issues/13) already records as *not*
exercising the refinement chain: `cold_sync` costs one round trip whatever the dataset size. So the
signal is strongest where the chain is not the cost and absent where the chain runs every interval.
Not built as a fourth shipped policy; kept here as `set_reconciliation_experiments::policy::CountDeltaFanOut`, the
probe that established it. What would reopen it: a measurement showing insert/delete-shaped
divergence dominating steady state, or a cap derivable from the comparison rather than tuned.

**Frame-budget follow-up ([#26](https://github.com/adriendellagaspera/experiments/harness/issues/26)).** The `widest` column shows the *sum of encoded aggregates in one logical refinement round*: 67 794 B for default `FixedFanOut(16)` at n = 10⁵, d = 100 scattered, and 80 484 B for `SqrtFanOut`. This is **not** a single UDP datagram: `Replica` wraps each aggregate in an `EntryFingerprint` and packs the resulting messages into separate datagrams capped at 65 507 B minus authentication overhead. The counted `devkit` datagram/fragment columns divide the aggregate sum by nominal ceilings; they are not observations of the actual packer, framing, or IP fragmentation. The 64-cap count-delta row brings the logical round sum to 59 730 B, but does not on its own establish an MTU-safe frame. The actual framing and loss exposure remain to be measured under #26.

## Historical contention measurements — canonical RSOS target

```sh
# Run from range-based-set-reconciliation:
cargo bench -p rsos --bench contention
cargo bench -p rsos --bench contention -- --quick

# Enable the private RSOS measurement seam to include machine-independent aggregate-update counts.
RUSTFLAGS='--cfg rbsr_internal_testing' cargo bench -p rsos --bench contention -- 'no_such_benchmark'

# Parameters remain environment-controlled; CONTENTION_RAW emits trial rows for pooled analysis.
CONTENTION_WRITERS=1,2,4,8,16,32,64,128 CONTENTION_TRIALS=30 CONTENTION_RAW=1 \
  cargo bench -p rsos --bench contention -- 'no_such_benchmark'
```

Answers [#445](https://github.com/Akvize/reconcile-rs/issues/445) and
[#455](https://github.com/Akvize/reconcile-rs/issues/455), feeding
[#359](https://github.com/Akvize/reconcile-rs/issues/359)/[#14](https://github.com/adriendellagaspera/reconcile-rs/issues/14):
the RSOS contract (`rsos/src/fingerprint_tree_map.rs`) must answer `Aggregate(l, u)` in `O(log n)`,
which means every insert writes the composable summary on every node from the leaf to the root — a
write to the hottest node in the tree, on every insert, by construction. Today that cost is
invisible: `src/replicated_map.rs` already serialises every writer behind one global `RwLock`, so the
root write costs nothing beyond the lock itself. This target isolates the two by running the
identical `N`-writer harness over two arms, both behind one shared `parking_lot::RwLock<_>` of the
exact shape `src/replica.rs`'s `map: Arc<RwLock<FingerprintTreeMap<K, V>>>` uses:

- **`fingerprint_tree_map`** — `RwLock<FingerprintTreeMap<u64, u64>>`, the real contract.
- **`btree_map`** — `RwLock<BTreeMap<u64, u64>>`, the no-aggregate control: same lock, same insert
  shape, no root-path summary to maintain.

Both arms pre-fill the map to 100 000 entries outside the timed region (an empty map has no root
path worth contending on, which would understate the RSOS arm's cost), then spawn `N` writer threads
that each insert 20 000 fresh, disjoint keys — one `write()` acquisition per key, the same shape a
gossip receipt or a local write takes today — starting together via a `Barrier` so the timed region
is genuinely concurrent rather than staggered by thread-spawn latency.

### Method

Reproducible from this section alone; the harness source adds no step that is not stated here.

**The counted half** is deterministic and carries no hardware caveat. `rsos::counters` (behind
`--cfg rbsr_internal_testing`) counts every write of a node's cached `Aggregate`, at the single setter every
maintenance path in `rsos` routes through. One single-threaded, untimed pass over a 100 000-entry
map brackets 4 096 fresh inserts, and then 4 096 overwrites of existing keys, between two counter
snapshots and divides. `BTreeMap` scores zero by construction — it performs the same descent with no
summary to keep — so this is the contract's own work, priced in operations rather than in one
machine's nanoseconds.

**The timed half** is wall-clock and stays so on purpose: lock waiting *is* elapsed time, and no
counted proxy exists for it. What #455 replaced is how it is estimated.

| | |
|---|---|
| Repetition | 30 trials per `(N, arm)`, after 3 discarded warm-up trials at the widest `N` |
| Pairing | both arms measured back to back inside one trial, so a machine-wide disturbance moves both and divides out |
| Arm order | alternated between trials, so each `N` gets each order exactly half the time; the harness reports a bootstrap test for a residual order effect |
| Trial order | every `(N, trial)` of the whole sweep executed in one seeded shuffle, **not** `N` by `N` |
| Interval | 95% percentile bootstrap (10 000 resamples, fixed seed) on the mean — not a `t` interval, since throughput is bounded below and left-tailed |
| Comparison | bootstrap interval on a *difference of means*; two intervals overlapping is not a test, an interval on their difference is |

The shuffled trial order is load-bearing, not hygiene. Running one `N`'s trials consecutively makes
the block of wall-clock they occupy part of the treatment: a co-tenant spike or a thermal excursion
lasting tens of seconds lands entirely on whichever `N` held the floor, and is then reported as a
property of that `N`. Two 30-trial sweeps built the `N`-by-`N` way disagreed here by more than either
one's interval admitted — 0.32 against 0.57 for the same ratio at `N = 2`, intervals disjoint.
Interleaving spreads any such episode across all `N`, converting that bias into variance the
intervals report honestly.

**The experimental unit is the invocation, not the trial.** Trials inside one process share a
machine phase, so an interval computed from them is silent about drift between processes — which on
shared hardware is the larger term. Publishable numbers therefore pool several invocations and
resample **invocations** (a cluster bootstrap), never trials:

```sh
for i in 1 2 3; do
  CONTENTION_RAW=1 cargo bench -p rsos --bench contention -- 'no_such_benchmark' > run$i.txt
done
# Each `[contention-raw]` line is `writers,fingerprint_ops_per_sec,btree_ops_per_sec,fingerprint_first`.
# Pool: group trials by invocation; for each of 10 000 resamples draw len(invocations) invocations
# with replacement, take every trial of each, and record the mean; report the 2.5th and 97.5th
# percentiles of those means.
```

**Which statistic answers which question.** Three are reported, and they are not interchangeable:

| statistic | what it is | what it can answer |
|---|---|---|
| per-arm throughput | ops/s, system-wide | how fast this machine goes; not portable (#281) |
| ratio, `X_fp / X_btree` | paired per trial | the contract's share of the cost *as the lock currently taxes it* |
| **delta, `1/X_fp − 1/X_btree`** | paired per trial, ns/insert | the contract's **own** cost, with the lock term cancelled — an upper bound, exact at `N = 1` |

Delta is the one to read. Behind one exclusive lock each arm's system-wide seconds-per-insert is its
own critical section plus whatever an acquisition costs at that `N`: `1/X_arm = S_arm + H(N)`. The
lock term is near-common to both arms — same lock, same acquisition pattern — so subtracting
reciprocal throughputs cancels it and leaves `S_fp − S_btree`, bounded from above (the model
section states exactly how near-common, and which way the residue points). The ratio moves when
*either* term moves and never says which; delta bounds one.

### Results

Measured on a 4-core KVM guest (Intel Xeon @ 2.80 GHz, 1 thread/core, 15 GiB), `RUSTFLAGS` unset,
release profile. Timed figures pool **3 invocations × 30 trials** per point, cluster-bootstrapped
per the recipe above. The timed half was measured **without** the `--cfg`, so no counter runs
inside a timed region.

**Counted** — identical on any machine, and exactly reproducible run to run:

| quantity | `FingerprintTreeMap` | `BTreeMap` |
|---|---:|---:|
| cached aggregates written per fresh insert | 6.76 | 0 |
| cached aggregates written per overwrite | 6.80 | 0 |

Both figures are the tree's root-path length at this size. The overwrite figure is exactly that —
one cached aggregate per level, which `rsos`'s own test asserts against an independently walked
depth. The fresh-insert figure also carries the node refreshes a split occasions and the particular
path a tail-appended key descends; at this size the two land within 1% of each other.

**This figure does not rest on trusting the seam.** It is the root-path length of a B-tree of order
6 holding 100 000 entries, which anyone can derive from those two published numbers without running
anything. `rsos::counters` verifies it against the code actually executed — a check on the
implementation, not the only route to the number — which is why the result stands even though the
counter itself is behind a test-only feature.

**Timed**, mean [95% cluster-bootstrap interval]:

| writers (`N`) | `fingerprint_tree_map` ops/s | `btree_map` ops/s | ratio (fp/btree) | delta ns/insert |
|---:|---:|---:|---:|---:|
| 1 | 2 888 103 [2 823 536, 2 957 004] | 9 762 176 [9 563 855, 10 020 359] | 0.298 [0.297, 0.299] | 258 [240, 288] |
| 2 | 1 649 710 [1 605 323, 1 673 716] | 4 069 999 [3 964 525, 4 144 117] | 0.457 [0.425, 0.509] | 362 [320, 384] |
| 4 | 1 403 681 [1 327 699, 1 451 446] | 3 408 950 [3 142 353, 3 598 838] | 0.421 [0.388, 0.465] | 417 [377, 465] |
| 8 | 964 143 [872 760, 1 011 022] | 3 231 527 [2 970 987, 3 429 929] | 0.301 [0.255, 0.341] | 733 [652, 859] |
| 16 | 845 546 [725 670, 926 593] | 2 800 533 [2 495 550, 3 114 095] | 0.310 [0.233, 0.376] | 837 [675, 1 065] |

Differences against the uncontended point, as bootstrap intervals — an interval excluding 0 is a
real move at 95%:

| `N` | `delta(N) − delta(1)` | | `ratio(N) − ratio(1)` | |
|---:|---:|---|---:|---|
| 2 | +104 ns [+73, +143] | grows | +0.159 [+0.128, +0.210] | diluted |
| 4 | +159 ns [+89, +224] | grows | +0.123 [+0.091, +0.167] | diluted |
| 8 | +475 ns [+365, +619] | grows | +0.003 [−0.042, +0.043] | indistinguishable |
| 16 | +579 ns [+387, +825] | grows | +0.012 [−0.063, +0.077] | indistinguishable |

**The sweep confined to the core count.** Past `N = 4` this machine is oversubscribed, and
oversubscription has its own mechanism that mimics the one under study: a thread can be preempted
*while holding the lock*, stalling every other writer for a scheduler quantum, and the probability of
being preempted mid-section scales with how long that section is — which is longer for the RSOS arm
by construction. That would inflate `delta` at `N = 8` and `N = 16` without the contract being
responsible. So the sweep is also run confined to `N ≤ 4`, where every writer has a core:

| writers (`N`) | delta ns/insert | `delta(N) − delta(1)` | | ratio to `delta(1)` |
|---:|---:|---:|---|---:|
| 1 | 238 [230, 249] | — | | 1.00× |
| 2 | 364 [327, 427] | +126 [+89, +191] | grows | 1.53× |
| 3 | 415 [414, 416] | +177 [+166, +186] | grows | 1.74× |
| 4 | 402 [391, 411] | +164 [+154, +176] | grows | 1.69× |

`delta` grows **1.7× with no oversubscription anywhere in the sweep**, every difference interval
clear of zero, and the `N = 3` estimate lands within ±1 ns across three independent invocations. The
growth is therefore not an artefact of running more threads than cores. It does appear to flatten
between `N = 3` and `N = 4`; the further rise to 3.2× at `N = 16` in the table above sits entirely in
the oversubscribed regime and **cannot be separated from preemption-while-holding-lock on this
machine**.

### What this answers for #359 — and what it revises

At `N = 1` there is no lock contention at all, so `FingerprintTreeMap` running at 0.298× a bare
`BTreeMap`'s throughput under the identical lock is the RSOS contract's own per-insert cost, alone:
258 ns, buying the 6.76 cached-aggregate writes the counted half prices. That part of #359 stands,
and is now an interval rather than a reading of two runs.

**The rest of #359's conclusion does not survive the sharper statistic.** #359 reported that the
fingerprint/btree ratio "does not widen" with `N` and concluded that the root-path write "does not
become a *larger* share of the cost as writer count grows". The ratio does behave that way here —
0.298 at `N = 1`, 0.310 at `N = 16`, indistinguishable. But a ratio of two terms that both grow is
flat precisely *because* they grow together, so its flatness is not evidence about either one. With
the lock's common cost cancelled, `delta` runs 258 ns at `N = 1` to 837 ns at `N = 16` — **3.2×,
every step's interval excluding zero**. The conclusion to carry forward is therefore:

> The gap between the two arms, with the lock's common cost cancelled, grows 3.2× across this
> sweep. #359's flat ratio is not evidence that the contract's write cost stays bounded under
> contention — a flat ratio is exactly what two terms growing together produce.

Read at the right scope, that is: **the 1.7× under full subscription is the defensible figure**, and
the 3.2× across the whole sweep is an upper bound on an upper bound, since its second half is
confounded by oversubscription. `delta` also bounds the gap from above rather than pinning it (the
model section below says why), so whether all of even the 1.7× is the contract's own cost or part is
a differential lock effect is **bounded here, not decided**. Deciding it needs each arm's parking
behaviour measured directly. A mechanism consistent with the growth, and the prediction it makes for
many-core hardware, is [#457](https://github.com/Akvize/reconcile-rs/issues/457)'s.

### A model for the curve, and where it breaks ([#457](https://github.com/Akvize/reconcile-rs/issues/457))

Nothing below names `FingerprintTreeMap`. It applies to any structure whose per-operation critical
section is longer than a baseline's, under any global lock — an RSOS is one instance.

**Assumptions, stated so they can be attacked.** `N` writers in a closed loop: acquire one exclusive
lock, do the whole operation inside it, release, immediately retry. No think time, so the lock is a
single server that is always busy for `N ≥ 1`, and system-wide seconds per operation is the critical
section plus what an acquisition costs at that writer count:

```text
1/X_arm(N) = S_arm(N) + H(N)
```

`H` is a property of the lock and the contention level — handoff, park/unpark, moving the lock word
between cores — not of what runs inside it. Both arms use the same lock type and the same
acquisition pattern, so take `H` to be common to them. That idealization is the whole reason `delta`
works — subtracting reciprocal throughputs cancels `H` and leaves `S_fp − S_btree` — and it is the
one the next paragraph puts under strain.

No distribution is assumed for the service time. The model is an identity at saturation, not a
stochastic queue: with zero think time the server is busy whenever a writer exists, so mean
throughput is the reciprocal of mean per-operation time regardless of how that time is distributed.
What is held fixed is the store size, the key and value types and the operation mix; only `N` varies.

**The textbook prediction, first, because it fails informatively.** Take the closed-loop model at
face value with no per-acquisition cost — `H(N) = 0` — and it predicts throughput *flat* in `N` for
both arms: one server, always busy, so `X(N) = X(1)`. Measured against that:

| writers (`N`) | augmented, predicted | measured | | control, predicted | measured | |
|---:|---:|---:|---:|---:|---:|---:|
| 2 | 2 888 103 | 1 649 710 | −43% | 9 762 176 | 4 069 999 | −58% |
| 4 | 2 888 103 | 1 403 681 | −51% | 9 762 176 | 3 408 950 | −65% |
| 8 | 2 888 103 | 964 143 | −67% | 9 762 176 | 3 231 527 | −67% |
| 16 | 2 888 103 | 845 546 | −71% | 9 762 176 | 2 800 533 | −71% |

Both arms fall far below it, and by `N = 16` they fall by the *same proportion*. A term that costs
both arms the same fraction of their throughput is a term they share — which is `H(N)`, and which is
large. That is the licence for the rest of this section: `H` is too big to ignore and too
lock-specific to model honestly, so the design cancels it instead of predicting it.

**Where that assumption is weakest, and which way it cuts.** `H` is not *exactly* common to the two
arms. A longer critical section makes a waiter more likely to exhaust its spin and park, and parking
costs more than spinning, so the arm with the longer section — the RSOS one — plausibly pays a
slightly larger `H`. Then `delta = (S_fp − S_btree) + (H_fp − H_btree)` and, since that second term
is non-negative, **delta is an upper bound on the contract's own cost, and its growth an upper bound
on that cost's growth.** Two things keep this from dissolving the result: at `N = 1` nothing parks,
so `delta(1)` is clean and the null's one parameter is measured where the assumption holds exactly;
and the bound has a sign, so "the contract's own cost grows with `N`" survives as a bounded claim
even if some of the 3.2× belongs to the lock. Turning the bound into a point estimate needs the
parking behaviour of each arm measured directly, which this harness does not do.

**The null.** Suppose `S_fp − S_btree` is a constant: the contract does a fixed amount of extra work
per operation, and contention only piles lock time on top. Then measuring that constant where
nothing confounds it — `delta` at `N = 1`, 258 ns — predicts the RSOS arm from the control arm
everywhere else, with no further fitting:

```text
X_fp_predicted(N) = 1 / ( 1/X_btree(N) + 258 ns )
```

One parameter, fitted at one point, extrapolated to the rest. A residual is therefore a statement
about the model, not an artefact of fitting. The harness prints this comparison itself.

| writers (`N`) | predicted `X_fp` | measured `X_fp` | residual | `delta(N) / delta(1)` |
|---:|---:|---:|---:|---:|
| 2 | 1 985 308 | 1 649 710 | −16.9% | 1.40× |
| 4 | 1 813 745 | 1 403 681 | −22.6% | 1.62× |
| 8 | 1 762 266 | 964 143 | −45.3% | 2.84× |
| 16 | 1 625 818 | 845 546 | −48.0% | 3.24× |

**The model does not fit, and the way it misses is the result.** The residual is negative at every
`N` and grows monotonically: the constant-cost null over-predicts the RSOS arm by 17% at `N = 2` and
by 48% at `N = 16`. `delta` rises 3.2× across the sweep, with #455's difference intervals putting
every step of that rise outside zero.

Stated exactly, what the data rules out is a *conjunction*: that the contract's extra work per
operation is constant in `N` **and** that the lock costs both arms the same. One of the two fails.
Everything below argues the first is the one that fails, and says what would show it.

**A mechanism consistent with it.** The two arms differ in what they *write*, not only in how much
they compute. Maintaining a range-summarizable aggregate means writing one cache line per level of
the root path on every operation — the same lines for every writer, since every root path ends at
the same root. Written lines must be held exclusively, so each handoff to a different core costs a
coherence miss per root-path node. The control arm writes its leaf and, rarely, a split; its
per-operation footprint of *written* shared lines is far smaller and far less likely to be the line
another core just took. So `S_fp` should grow with the number of distinct cores that touch the root
path, while `S_btree` barely moves — which is the sign and shape of the residual above.

This is a mechanism the data is consistent with, not one this benchmark isolates: distinguishing
coherence traffic from other `N`-dependent effects needs hardware counters, which is its own piece of
work and not one #457 claims to have done.

**What it predicts, and how to falsify it.** If coherence on the written root path is the term that
grows, then hardware with more cores — and more so across sockets or NUMA nodes, where a handoff
crosses an interconnect — should make it grow *faster*. Concretely, on a machine with `C ≥ 16` real
cores, `delta(C) / delta(1)` measured **with `N ≤ C`** should exceed the 1.69× this machine reaches at
`N = 4`. A flat or shrinking `delta` ratio there refutes the mechanism, and `delta` constant in `N`
would restore the null.

That is a sharper requirement than "sweep further", and it is the one
[#20](https://github.com/adriendellagaspera/reconcile-rs/issues/20) has to meet: sweeping to `N = 128` on a
16-core machine would spend most of its points 8× oversubscribed and reproduce exactly the confound
that makes this machine's `N = 8` and `N = 16` unusable. The regime worth buying hardware for is
**many writers each holding a core**, not many threads sharing a few.

**What it means for [#36](https://github.com/adriendellagaspera/reconcile-rs/issues/36).** The lock is not
merely hiding a fixed tax that removing it would expose unchanged. Part of the contract's cost is
*created* by sharing the root path across writers, so a design that keeps a single hot root — with
or without a lock — carries a term that grows with writer count. Structures that avoid it do so by
not having every writer touch the same node: path copying, per-writer deltas reconciled later, or a
root chain left deliberately uncollapsed, which is what AB-tree does
([#446](https://github.com/Akvize/reconcile-rs/issues/446)).

**Comparability caveat (#281).** The timed half is not deterministic — throughput is wall-clock, so
it inherits scheduler noise the way the RTT lane above does. Both arms run in the same process, on
the same hardware, in the same invocation, so arm-against-arm at a given `N` is what it supports;
absolute ops/s is specific to the machine that produced it. Everything past `N = 4` is also past this
machine's core count, which is exactly why [#20](https://github.com/adriendellagaspera/reconcile-rs/issues/20)
exists. The counted half carries none of this — that is the point of having it.

## Shared experiment foundation

The neutral experiment contract, exact signed-set reference and deterministic placement/mutation/
restart corpora live in the pinned upstream `devkit`. `comparison::experiment` re-exports that
contract. The research policies, sketch implementations and datastore adapters remain local.

Upstream structured experiment reports can be read directly, without parsing benchmark logs:

```sh
cargo run --release --bin summarize_experiment -- /path/to/experiment.json
```

The renderer preserves observed/projected/missing provenance and does not rank missing end-to-end
metrics. Neutral report schema v2 is distinct from the legacy reconciliation schema v2 below and
from the AELMDB source-replay schema. Neutral v1 reports require their original reader; no automatic
conversion is implied. Upstream benchmark output configuration is documented in its benches README.

## Reconciliation comparison formal run

The cross-family microbenchmark produces one versioned JSON document directly from its typed
in-memory report. Human-readable output is never an input format.

Schema v2 names local elapsed durations explicitly; these are `Instant` measurements, not process
CPU. Schema v1 artifacts remain tied to their producing revision and are rejected by the v2 reader.
Fixed/self-sizing IBLT and RIBLT successful decodes are checked against an independent exact signed
difference outside the timed algorithm section. Network rows are projections. Preparation remains
unequal across arms, and zero temporary-memory fields in this legacy baseline are unmeasured
placeholders, not evidence of zero allocation. This report is not a datastore lifecycle comparison.

```sh
git rev-parse HEAD
rustc -Vv
cargo -V
uname -a
cargo bench --bench reconciliation -- --output /tmp/reconciliation-comparison.json
sha256sum /tmp/reconciliation-comparison.json
cargo run --release --bin summarize_reconciliation -- \
  /tmp/reconciliation-comparison.json \
  > /tmp/reconciliation-comparison.md
```

The benchmark refuses to overwrite an existing artifact. Preserve the revision, environment,
artifact hash, JSON and rendered table together. The report contains the seven preregistered
scenarios, four network profiles, classic RBSR, fixed-capacity IBLT → RBSR, both self-sizing probes,
RIBLT and bulk in each row. RIBLT cache-build CPU and persistent state remain separate from its
steady-state projection; both ideal and line-rate STOP byte counts remain visible.

The summarizer selects one post-hoc global M1 policy over the whole matrix; this is an in-sample
oracle, not a policy validated on held-out workloads. Its aggregate is only a compact
ranking of heterogeneous cells; the per-cell table is authoritative. #37 must join this pipeline by
adding a typed lane to the same report. A textual adapter, TSV bridge, or parser for benchmark
stdout is deliberately rejected. #41's four-family oracle remains unavailable until the streaming
lane is present for every typed matrix cell.

## Ordered-store source replay (#52)

`../../scripts/experimental/run-ordered-pilot.py` runs the pinned March 2026 Negentropy/AELMDB source chain
on Linux, using Python 3, git, cc/g++ with C++20 and OpenSSL development headers:

```sh
python3 ../../scripts/experimental/run-ordered-pilot.py /tmp/ordered-pilot
```

The output directory must not exist. Sources are fetched at exact commits; `--sources DIR`
reuses matching clean checkouts. `--smoke` selects one instance/trial and is not the formal pilot.
The exporter writes explicit JSON after measurements; logs are not parsed.

The [frozen profile](../../workloads/datastore/ordered.aelmdb.base_dense.json) selects
`baseline_dense_i`, i=1..8: 1,268i records per peer, 68i in scope, 4i HAVE and 4i NEED.
Each arm has ten fresh-process trials after preparation, each averaging ten reconciliations;
arm order rotates. Exact analytic HAVE/NEED equality is checked on the first reconciliation
of each process, outside its timer. This is scoped ID discovery, not whole-database repair.

[Manifest](../../workloads/ordered-pilot/2026-09-27/manifest.json),
[raw observations](../results/ordered-pilot/2026-09-27/observations.jsonl) and
[summary](../results/ordered-pilot/2026-09-27/summary.json) preserve 240 observations and hashes.
Run: 2026-09-27, AMD EPYC 9V74, Linux x86_64, g++ 13.3.0, O3.
Times below are medians of process means in milliseconds; raw min/max remain in the summary.

| i | Backend | Reconciliation ms | Preparation ms |
|---|---|---:|---:|
| 1 | BTreeLMDB | 0.410 | 3.289 |
| 1 | NoAggWindowSliceAELMDB | 0.215 | 3.977 |
| 1 | AELMDBSlice | 0.199 | 4.056 |
| 8 | BTreeLMDB | 1.294 | 10.484 |
| 8 | NoAggWindowSliceAELMDB | 0.439 | 11.458 |
| 8 | AELMDBSlice | 0.415 | 12.824 |

Protocol byte counts agree across all three arms at each i (443 bytes at i=1, 2,929 at i=8);
this does not establish byte-for-byte transcript identity. BTreeLMDB is the source's B-tree
adapter on LMDB. NoAggWindow disables aggregate windows, not all aggregate maintenance.
AELMDB's reconciliation median is lower than BTreeLMDB's on all eight instances, while its
preparation median is higher. Windows do not improve the median over NoAggWindow on every
instance. These observations support investigating session/preparation trade-offs, not a
strict Pareto-dominance claim or evidence for the new shared traversal proposed in #63.

Limits: small fixed workloads, shared host, no page-cache eviction, network transport, IBLT
arm, mutations or maintenance study. RSS is a snapshot, not peak memory; CPU/I/O are unknown.
Preparation subphases overlap. `total_bench_ms` sums open/build/mean-reconcile durations and
is not a measured end-to-end critical path. JSON here is a source-replay artifact; integration
with #50's neutral ExperimentReport is pending. The pinned compatible source chain does not
test AELMDB v0.2.0. An earlier interrupted run had incomplete raw data and is excluded.

## Not covered yet

- **External comparisons** (e.g. point-read vs Redis over loopback) are intentionally out of scope here to keep the default `cargo bench` dependency-light. They belong behind an optional, non-CI Cargo feature in a follow-up.
- **Cross-family deployment comparison remains incomplete.** The local microbenchmark above exercises sketch families, and the ordered-store replay below runs pinned Negentropy/AELMDB source. Neither provides a matched real-engine RBSR/IBLT lifecycle comparison; #50/#52 own that gap.
- The `img/perf-*.png` graphs in the repo root are produced out-of-band from the `bench` target's Criterion output; regenerating them is a manual step (there is no committed plotting pipeline).
