// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Issue #8: a peeled IBLT cell yields a 256-bit digest, not a key, and
//! [`FingerprintTreeMap`](rsos::FingerprintTreeMap) has no index from one to the other. Three
//! options were proposed, priced only in prose: (a) a `Fingerprint -> K` index beside the map,
//! (b) the cell carries the encoded key XOR-summed, (c) the sketch is scoped to a range and
//! resolved by local enumeration. This file prices (a) and (b) with real numbers instead of the
//! issue's own hand-waved ranges; (c) is coupled to #6's per-range-vs-global decision and is not
//! priced here.
//!
//! **No change lands in `rsos`/`reconcile-rs` from this file.** Whichever recovery scheme wins is
//! a wire/index change to the upstream engineering crate, out of this research companion's
//! mandate (`AGENTS.md` §1: this repo measures and records the call, `src/policy.rs`'s probes are
//! never shipped) and out of this session's repository scope. What lands here is the quantified
//! comparison the decision should be made from.
//!
//! **Option (a)'s memory cost** ([`index_memory_bytes`]): a lower bound (`n` entries of
//! `Fingerprint` + `K`, no bucket accounted) and an approximate upper one (`x 4/3`, a round stand-in
//! for a hash table's load-factor/control-byte overhead — deliberately not a false-precision claim
//! about `std::collections::HashMap`'s specific backend, which is unspecified and can change).
//!
//! **Option (b)'s wire cost** ([`keyed_cell_exchange_shape`]) reuses
//! `sketch_exchange_fragmentation_under_loss.rs`'s cell model (`count: i64` + a 256-bit key-sum
//! digest + an 8 B check-sum = 48 B, cited there against #13's own "256 cells ~= 12 kB") plus the
//! encoded key's own bytes, through the same `MAX_DATAGRAM_PAYLOAD`/`MTU_FRAGMENT_PAYLOAD`
//! ceilings — so this file's fragment counts sit on the same axis that file already found the bare
//! (keyless) sketch marginal on, and can be read against it directly.
//!
//! **The tombstone-window contribution to effective `d`** is not a workspace quantity this file
//! can measure (it depends on a deployment's delete rate, not on anything `cargo test` drives) —
//! stated as a formula in the module doc of the "Outcome" recorded on the issue instead:
//! `effective_d_from_tombstones ~= delete_rate_per_second x gc_window_seconds`, independent of
//! whatever difference `d` a snapshot comparison would otherwise show.

#![forbid(unsafe_code)]

use rsos::Fingerprint;

/// [`rsos::Fingerprint`]'s wire width — the load-bearing constant under every number this file
/// computes. Checked, not assumed: if this ever changes, every figure below (and the one
/// `sketch_exchange_fragmentation_under_loss.rs` already recorded) goes stale silently otherwise.
const FINGERPRINT_BYTES: usize = 32;

/// The same three-message shape and base cell size
/// `sketch_exchange_fragmentation_under_loss.rs` prices #7 against: `count: i64` (8 B) + a 256-bit
/// key-sum digest (32 B) + an 8 B check-sum.
const BASE_CELL_BYTES: usize = 48;
const SMALL_MESSAGE_BYTES: usize = 64;
const MAX_DATAGRAM_PAYLOAD: usize = 65_507;
const MTU_FRAGMENT_PAYLOAD: usize = 1_472;

/// A round stand-in for a hash table's load-factor/control-byte overhead, applied to the raw
/// `n * entry_size` lower bound. Not a claim about any specific `HashMap` implementation's actual
/// layout -- just enough to turn "tens of MB" into a stated range with a stated multiplier.
const HASH_TABLE_OVERHEAD_NUMERATOR: usize = 4;
const HASH_TABLE_OVERHEAD_DENOMINATOR: usize = 3;

/// Option (a): `(raw lower bound, approximate upper bound)` in bytes for a `Fingerprint -> K`
/// index over `n` entries of a `key_bytes`-wide key.
fn index_memory_bytes(n: usize, key_bytes: usize) -> (usize, usize) {
    let entry_bytes = FINGERPRINT_BYTES + key_bytes;
    let raw = n * entry_bytes;
    let approx_upper = raw * HASH_TABLE_OVERHEAD_NUMERATOR / HASH_TABLE_OVERHEAD_DENOMINATOR;
    (raw, approx_upper)
}

/// Option (b): `(messages, datagrams, fragments)` for a sketch exchange at `capacity` cells, each
/// cell widened by `key_bytes` beyond the base 48 B to carry the XOR-summed encoded key.
fn keyed_cell_exchange_shape(capacity: usize, key_bytes: usize) -> (usize, usize, usize) {
    let offer_bytes = capacity * (BASE_CELL_BYTES + key_bytes);
    let small = SMALL_MESSAGE_BYTES;
    let datagrams_of = |bytes: usize| bytes.div_ceil(MAX_DATAGRAM_PAYLOAD).max(1);
    let fragments_of = |bytes: usize| bytes.div_ceil(MTU_FRAGMENT_PAYLOAD).max(1);
    let messages = 3;
    let datagrams = datagrams_of(offer_bytes) + datagrams_of(small) + datagrams_of(small);
    let fragments = fragments_of(offer_bytes) + fragments_of(small) + fragments_of(small);
    (messages, datagrams, fragments)
}

#[test]
fn fingerprint_is_32_bytes_the_load_bearing_constant_for_this_analysis() {
    assert_eq!(
        std::mem::size_of::<Fingerprint>(),
        FINGERPRINT_BYTES,
        "every memory/wire figure in this file assumes a 256-bit Fingerprint"
    );
}

/// Option (a) at #7's/#13's own `n = 10^6` operating point, for `K = u64` (the common case: an
/// integer or hashed key). Printed rather than merely asserted internally consistent, so the
/// number quoted in the issue is the number this file actually computed.
#[test]
fn option_a_index_memory_at_n_1e6_for_a_u64_key() {
    let (raw, approx_upper) = index_memory_bytes(1_000_000, 8);
    assert_eq!(raw, 40_000_000, "1e6 * (32 B fingerprint + 8 B u64 key)");
    assert_eq!(
        approx_upper, 53_333_333,
        "raw * 4/3, the stated overhead approximation"
    );
    println!(
        "[digest-resolution] option (a), n=1e6, K=u64: {raw} B raw ({raw_mib:.1} MiB) .. \
         {approx_upper} B with ~4/3 hash-table overhead ({upper_mib:.1} MiB) -- 'tens of MB', now \
         a stated range rather than a hand-wave",
        raw_mib = raw as f64 / (1024.0 * 1024.0),
        upper_mib = approx_upper as f64 / (1024.0 * 1024.0),
    );
}

/// Option (b) at the two capacities #7 already priced the bare (keyless) sketch at, across the
/// key widths #8's own text names: `u64`/`u128` keys (8/16 B), a hashed key (32 B, `Fingerprint`
/// width), and the issue's own 64 B example. Monotonic in `key_bytes` by construction (a wider
/// cell never yields fewer fragments) -- the property checked -- with the concrete counts printed
/// against #7's already-marginal keyless baseline (11 fragments at cap=256).
#[test]
fn option_b_fragment_cost_grows_with_key_width_past_the_keyless_baseline() {
    const KEY_WIDTHS: &[usize] = &[8, 16, 32, 64];
    for &capacity in &[64usize, 256] {
        let mut previous_fragments = 0;
        for &key_bytes in KEY_WIDTHS {
            let (messages, datagrams, fragments) = keyed_cell_exchange_shape(capacity, key_bytes);
            println!(
                "[digest-resolution] option (b), cap={capacity}, key={key_bytes} B: {messages} \
                 messages / {datagrams} datagrams / {fragments} fragments"
            );
            assert!(
                fragments >= previous_fragments,
                "a wider key-carrying cell must never cost fewer fragments"
            );
            previous_fragments = fragments;
        }
    }

    let keyless = 11; // sketch_exchange_fragmentation_under_loss.rs's cap=256 baseline
    let (_, _, at_64_bytes) = keyed_cell_exchange_shape(256, 64);
    assert_eq!(
        at_64_bytes, 22,
        "#8's own '64 B key field puts 256 cells at ~=20 kB' is a rough estimate; priced through \
         the same ceilings #7 used the real figure is 28 672 B (~=28 KiB, not 20), \
         ceil(28672/1472) = 20 fragments for the offer alone, +1 miss +1 updates = 22 total -- \
         almost 3x #7's keyless 11-fragment baseline"
    );
    assert!(
        at_64_bytes > keyless,
        "embedding a 64 B key must cost strictly more fragments than #7's keyless baseline"
    );
}
