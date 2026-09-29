// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The probe policies this crate measures rather than ships: the two oracle-coupled ones from
//! #356 ([`FingerprintDerivedSplit`], [`SpanRelativeFingerprintSplit`]) and the
//! count-delta-driven [`CountDeltaFanOut`] from #12.
//!
//! [`CountDeltaFanOut`] is the odd one out and deliberately so: it reads `size()` only, so unlike
//! the two below it violates nothing and would be spellable against a released `rbsr`. It lives
//! here because it is a candidate under measurement, not because it needs the gate.
//!
//! The other two live *here*, not in `rbsr`, because they deliberately violate the law
//! `rbsr::Comparison`'s docs state ("no fingerprint-derived decisions", #352): a policy plugged
//! into a *real* drive is meant to see only `span()`/`remote_size()`/`agrees()`/`children_emitted()`.
//! They read past that on purpose, using the `local_for_testing`/`remote_for_testing` accessors
//! `rbsr::Comparison` exposes under `cfg(rbsr_internal_testing)` for exactly this kind of
//! probe (the standalone RBSR testing seam) — never a shipped policy, and never reachable
//! outside that cfg.

use rbsr::{Comparison, Decision, FanOut, RefinementPolicy, SplitStride};

/// The cutoffs [`rbsr::SqrtFanOut`]/[`rbsr::FixedFanOut`] share, reimplemented here over the
/// public [`Comparison`] API (`rbsr::policy::cutoffs::shared_cutoffs` is crate-private) so both
/// probe policies below enumerate at exactly the same boundary those controls do — the only
/// variable a probe isolates is how wide a SPLIT cuts, never *when* a range is enumerated
/// instead of split.
fn shared_cutoffs(comparison: Comparison) -> Option<Decision> {
    let local = comparison.span();
    let remote = comparison.remote_size();
    if comparison.agrees() {
        Some(Decision::Skip)
    } else if remote == 0 {
        Some(Decision::Enumerate)
    } else if local == 0 {
        Some(Decision::Split(SplitStride::ONE))
    } else if local == 1 && remote == 1 {
        Some(Decision::Enumerate)
    } else if local == 1 {
        Some(Decision::Split(SplitStride::ONE))
    } else {
        None
    }
}

/// **Test-only probe (#356), `cfg(rbsr_internal_testing)`-gated.** Deliberately violates the
/// law `rbsr::Comparison`'s docs state: it derives its split stride from the **local**
/// aggregate's fingerprint instead of from the range alone, reintroducing the oracle dependence
/// rank-cut refinement exists to avoid — the index set this produces is no longer a deterministic
/// function of the data alone.
///
/// Reachable only through `local_for_testing` (`cfg(rbsr_internal_testing)`), so a policy
/// author outside this crate cannot spell the same violation against a released `rbsr`. Never a
/// shipped policy — see `tests/oracle_dependent_split_vs_the_union_bound.rs` for what it measures.
///
/// Enumeration cutoffs match [`rbsr::FixedFanOut`]'s (`shared_cutoffs`, private to this module), so
/// the only variable this isolates is *how the split stride is chosen*, never *when* a range is
/// enumerated instead
/// of split.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FingerprintDerivedSplit;

impl RefinementPolicy for FingerprintDerivedSplit {
    fn decide(&self, comparison: Comparison) -> Decision {
        if let Some(decision) = shared_cutoffs(comparison) {
            return decision;
        }
        // (A5)-violating on purpose (#356): the stride comes from `local`'s fingerprint, the same
        // oracle the skip rule's per-comparison collision probability is stated over.
        let stride = 1 + comparison.local_for_testing().fingerprint().0[0] as usize % 32;
        Decision::Split(SplitStride::per_child(stride))
    }
}

/// **Test-only probe (#356), `cfg(rbsr_internal_testing)`-gated.** Oracle-coupled *and*
/// span-relative: `1 + fingerprint.low_limb mod (span − 1)`, so every SPLIT emits at least two
/// children.
///
/// The cell [`FingerprintDerivedSplit`] leaves empty. It reads the same oracle — the *choice of
/// cut point* is fingerprint-determined, so the index set an execution compares is still
/// correlated with the digest the collision probability is stated over — while keeping the
/// progress property every shipped policy has. Drives under it terminate, so the soundness
/// question can be measured on a full-size population rather than a censored one. Never a shipped
/// policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpanRelativeFingerprintSplit;

impl RefinementPolicy for SpanRelativeFingerprintSplit {
    fn decide(&self, comparison: Comparison) -> Decision {
        if let Some(decision) = shared_cutoffs(comparison) {
            return decision;
        }
        // Reads the same oracle `FingerprintDerivedSplit` does, but reduced mod the span, so the
        // stride lands in `1..span-1` and every SPLIT emits at least two children.
        // `shared_cutoffs` has already returned for `span <= 1`, so `span - 1 >= 1` here.
        let stride = 1 + comparison.local_for_testing().fingerprint().0[0] as usize
            % (comparison.span() - 1);
        Decision::Split(SplitStride::per_child(stride))
    }
}

/// **Probe policy (#12), not a shipped one.** Fan-out chosen from the observed **count delta**
/// `|span() − remote_size()|` instead of from the range size — the third axis `FixedFanOut` and
/// `SqrtFanOut` both leave open, each choosing its width from `m` alone.
///
/// Width-only, never [`Decision::Skip`]: the enumeration cutoffs are `shared_cutoffs`', so the only
/// variable this isolates against the controls is how wide a SPLIT cuts.
///
/// **`size()`-derived only, and that confinement is the result rather than a limitation.** RBSR's
/// soundness bound unions a per-comparison collision probability over the ranges an execution
/// compares, and the union is legal only because that index set is a deterministic function of the
/// data. Of an `Aggregate`'s two components only `size()` keeps that; a width read off the
/// fingerprint would void the bound, which is exactly what the two policies above do on purpose.
/// So the count is not the *preferred* signal, it is the only admissible one — and it reads zero
/// precisely on a range whose peers hold equal cardinalities, which is where an LWW update to an
/// existing key lands and where a false SKIP is possible at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CountDeltaFanOut {
    floor: FanOut,
    cap: FanOut,
}

impl CountDeltaFanOut {
    /// A policy that degrades to `floor` where the delta reads zero and widens up to `cap`.
    ///
    /// `cap` is raised to `floor` when it is below it, so [`fan_out_for_delta`](Self::fan_out_for_delta)
    /// is total: a cap under the floor would otherwise describe a policy that both must and must not
    /// fall back to the default.
    pub const fn new(floor: FanOut, cap: FanOut) -> CountDeltaFanOut {
        CountDeltaFanOut {
            floor,
            cap: if cap.get() < floor.get() { floor } else { cap },
        }
    }

    /// What this policy degrades to where the delta reads zero.
    pub const fn floor(self) -> FanOut {
        self.floor
    }

    /// The ceiling on widening. Never below [`floor`](Self::floor).
    pub const fn cap(self) -> FanOut {
        self.cap
    }

    /// The fan-out for a count delta: `floor` at zero, else `delta + 1` clamped to `floor..=cap`.
    ///
    /// `delta + 1` rather than `delta` because separating `k` differences takes `k + 1` parts at
    /// worst. It is the fan-out *asked for*, not a guarantee about children emitted:
    /// [`SplitStride::for_fan_out`] is `⌈span / b⌉`, which realizes fewer than `b` children
    /// whenever the stride does not divide the span.
    pub fn fan_out_for_delta(self, delta: usize) -> FanOut {
        if delta == 0 {
            return self.floor;
        }
        FanOut::new(
            delta
                .saturating_add(1)
                .clamp(self.floor.get(), self.cap.get()),
        )
    }
}

impl RefinementPolicy for CountDeltaFanOut {
    fn decide(&self, comparison: Comparison) -> Decision {
        if let Some(decision) = shared_cutoffs(comparison) {
            return decision;
        }
        let delta = comparison.span().abs_diff(comparison.remote_size());
        Decision::Split(SplitStride::for_fan_out(
            comparison.span(),
            self.fan_out_for_delta(delta),
        ))
    }
}

#[cfg(all(test, rbsr_internal_testing))]
mod tests;
