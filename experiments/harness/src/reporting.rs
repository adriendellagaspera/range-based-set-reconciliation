// Copyright 2026 Developers of the reconcile-rs project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Measurement and reporting on top of `rbsr`'s kept probe-harness core (its private
//! `probe_harness` module, reached only through its re-exports below): a
//! [`Wilson`](wilson_99_ci) interval, per-comparison tallying ([`Observing`]/[`Tally`]), sweep
//! drivers ([`measure`]/[`measure_d`]/[`measure_observed`]), a threshold-wrapping policy combinator
//! ([`EnumerateBelow`]), and two report formatters.
//!
//! Everything here is written over `rbsr`'s public API — [`rbsr::Comparison::agrees`],
//! [`rbsr::Comparison::span`], [`rbsr::Comparison::remote_size`] — never over anything beyond what
//! the probe harness already re-exports ([`rbsr::NarrowStore`], [`rbsr::drive`],
//! [`rbsr::Termination`], [`rbsr::balanced_swap`], [`rbsr::DRIVE_STORE_SIZE`]), so it is unaffected
//! by the fingerprint-accessor gap `lib.rs`'s module docs describe.

use std::cell::Cell;
use std::collections::HashSet;

use rand::rngs::StdRng;
use rand::SeedableRng;

use rbsr::{
    balanced_swap, drive, Comparison, Decision, NarrowStore, RefinementPolicy, Termination,
};

/// Algorithm 1's `t` in front of any split rule: **IDLIST at `span <= t`, delegate otherwise.**
///
/// Expressible entirely over the public [`Comparison`] API — pairing it with an otherwise
/// non-progressing probe stride is something any policy author can do, and (per #356) the result
/// is that the stall this crate documents disappears.
pub struct EnumerateBelow<P> {
    /// The enumeration cutoff: `span() <= threshold` always enumerates.
    pub threshold: usize,
    /// The policy consulted once `span() > threshold`.
    pub inner: P,
}

impl<P: RefinementPolicy> RefinementPolicy for EnumerateBelow<P> {
    fn decide(&self, comparison: Comparison) -> Decision {
        if comparison.agrees() {
            Decision::Skip
        } else if comparison.span() <= self.threshold {
            Decision::Enumerate
        } else {
            self.inner.decide(comparison)
        }
    }
}

/// A policy that answers exactly like `inner` while tallying what each comparison *was*, entirely
/// off the public [`Comparison`] API.
///
/// - **`comparisons`** — every range the drive classified: the multiplier a union bound is stated
///   over.
/// - **`collision_capable`** — ranges where both sides advertised the same size while disagreeing.
///   [`Comparison::agrees`] tests the whole aggregate, so a size mismatch is refused at every width
///   and no fingerprint collision can turn it into a SKIP; only these can contribute a false
///   convergence.
/// - **`non_progressing`** — `Split` decisions with `stride >= span` at `span > 1`: the ones that
///   emit one child equal to the parent instead of refining (the eventual-progress law, #420).
pub struct Observing<P> {
    /// The wrapped policy, consulted for the real decision.
    pub inner: P,
    /// Every comparison seen.
    pub comparisons: Cell<u64>,
    /// Comparisons where both sides advertised the same size while disagreeing.
    pub collision_capable: Cell<u64>,
    /// Non-progressing `Split` decisions seen.
    pub non_progressing: Cell<u64>,
}

impl<P> Observing<P> {
    /// Wrap `inner`, starting every counter at zero.
    pub fn new(inner: P) -> Observing<P> {
        Observing {
            inner,
            comparisons: Cell::new(0),
            collision_capable: Cell::new(0),
            non_progressing: Cell::new(0),
        }
    }
}

impl<P: RefinementPolicy> RefinementPolicy for Observing<P> {
    fn decide(&self, comparison: Comparison) -> Decision {
        self.comparisons.set(self.comparisons.get() + 1);
        if !comparison.agrees() && comparison.span() == comparison.remote_size() {
            self.collision_capable.set(self.collision_capable.get() + 1);
        }
        let decision = self.inner.decide(comparison);
        if let Decision::Split(stride) = decision {
            if comparison.span() > 1 && stride.get() >= comparison.span() {
                self.non_progressing.set(self.non_progressing.get() + 1);
            }
        }
        decision
    }
}

/// Two-sided 99% Wilson score interval for `successes` out of `trials`.
pub fn wilson_99_ci(successes: u64, trials: u64) -> (f64, f64) {
    assert!(trials > 0, "an interval needs at least one trial");
    let n = trials as f64;
    let p = successes as f64 / n;
    const Z: f64 = 2.575_829_303_548_901; // Phi^-1(0.995), the two-sided 99% quantile
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denom;
    let margin = (Z / denom) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    ((center - margin).max(0.0), (center + margin).min(1.0))
}

/// One policy's outcome over `trials` independently seeded instances at one width.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tally {
    /// Trials run.
    pub trials: u64,
    /// Drives that settled (reached a fixed point).
    pub settled: u64,
    /// Drives proved to stall (a state recurred).
    pub stalled: u64,
    /// Drives that hit the round cap with neither verdict.
    pub round_cap: u64,
    /// Settled drives whose enumerated ranges missed part of the true symmetric difference.
    pub false_convergence: u64,
    /// Comparisons made across every settled drive.
    pub comparisons: u64,
    /// Rounds run across every drive.
    pub rounds: u64,
}

impl Tally {
    /// Mean comparisons per settled drive, `0.0` when nothing settled.
    pub fn mean_comparisons(&self) -> f64 {
        if self.settled == 0 {
            0.0
        } else {
            self.comparisons as f64 / self.settled as f64
        }
    }
}

/// `a`'s and `b`'s keys that appear in exactly one of the two.
pub fn symmetric_difference(a: &[u64], b: &[u64]) -> HashSet<u64> {
    let a: HashSet<u64> = a.iter().copied().collect();
    let b: HashSet<u64> = b.iter().copied().collect();
    a.symmetric_difference(&b).copied().collect()
}

/// [`measure`], with the policy wrapped in [`Observing`] so the run also reports the two
/// union-bound multipliers **at the width being measured**.
///
/// Measuring them at the measured width rather than at `w = 64` is required for an oracle-coupled
/// policy and merely harmless for a rank-cut one: a rank-cut policy descends the same ranges at
/// every width, so one measurement transfers; an oracle-coupled policy's index set is a different
/// random variable at each width, so nothing transfers. The residual bias is that a comparison
/// resolved by an actual collision is booked as agreeing rather than as collision-capable, which
/// undercounts by the very rate under measurement — second order at every width reported here.
pub fn measure_observed<P: RefinementPolicy>(
    width: u32,
    trials: u64,
    swap_size: usize,
    policy: P,
) -> (Tally, f64) {
    let observing = Observing::new(policy);
    let tally = measure_d(width, trials, swap_size, &observing, |_, _, _| {});
    let capable = observing.collision_capable.get() as f64;
    (tally, capable / trials.max(1) as f64)
}

/// Run `trials` drives of `policy` at `width`, recording each trial's verdict.
pub fn measure<P: RefinementPolicy>(
    width: u32,
    trials: u64,
    policy: &P,
    per_trial: impl FnMut(u64, Termination, bool),
) -> Tally {
    measure_d(width, trials, 1, policy, per_trial)
}

/// [`measure`] over a `swap_size`-element difference instead of a single swapped key.
///
/// The difference size is load-bearing for the soundness half, not a free parameter: only a range
/// holding an *equal count* on both sides can falsely agree, so at `swap_size = 1` the outer range
/// is essentially the only collision-capable comparison a drive makes — and it is compared before
/// any split decision, which makes the rate policy-independent by construction.
pub fn measure_d<P: RefinementPolicy>(
    width: u32,
    trials: u64,
    swap_size: usize,
    policy: &P,
    mut per_trial: impl FnMut(u64, Termination, bool),
) -> Tally {
    let mut tally = Tally {
        trials,
        ..Tally::default()
    };
    for trial in 0..trials {
        let mut rng = StdRng::seed_from_u64(trial);
        let (a_keys, b_keys) = balanced_swap(&mut rng, rbsr::DRIVE_STORE_SIZE, swap_size);
        let diff = symmetric_difference(&a_keys, &b_keys);
        let a = NarrowStore::new(width, a_keys);
        let b = NarrowStore::new(width, b_keys);

        let result = drive(&a, &b, policy, &mut rng);
        tally.rounds += result.rounds as u64;
        let mut missed = false;
        match result.termination {
            Termination::Settled => {
                tally.settled += 1;
                tally.comparisons += result.comparisons;
                let found: HashSet<u64> = result
                    .enumerated
                    .iter()
                    .flat_map(|r| a.keys_in(r).into_iter().chain(b.keys_in(r)))
                    .collect();
                missed = !diff.is_subset(&found);
                if missed {
                    tally.false_convergence += 1;
                }
            }
            Termination::Stalled { .. } => tally.stalled += 1,
            Termination::RoundCap => tally.round_cap += 1,
        }
        per_trial(trial, result.termination, missed);
    }
    tally
}

/// Report a [`Tally`]'s termination breakdown for `label` at `width`.
pub fn report_termination(label: &str, width: u32, t: Tally) {
    let pct = |n: u64| 100.0 * n as f64 / t.trials as f64;
    println!(
        "{label:<34} w={width:<3} settled {:>7}/{:<7} ({:>6.2}%)  proved-stalled {:>7} ({:>6.2}%)  \
         round-cap {:>6} ({:>5.2}%)  mean rounds {:>6.1}",
        t.settled,
        t.trials,
        pct(t.settled),
        t.stalled,
        pct(t.stalled),
        t.round_cap,
        pct(t.round_cap),
        t.rounds as f64 / t.trials as f64,
    );
}

/// Report a [`Tally`]'s false-convergence rate and 99% Wilson interval for `label` at `width`.
pub fn report_soundness(label: &str, width: u32, t: Tally) {
    if t.settled == 0 {
        println!("{label:<34} w={width:<3} no settled drive — nothing to report");
        return;
    }
    let (lo, hi) = wilson_99_ci(t.false_convergence, t.settled);
    let scale = (1u64 << width) as f64;
    println!(
        "{label:<34} w={width:<3} {:>6} events / {:>7} settled  rate={:.4e} \
         99% CI=[{lo:.3e}, {hi:.3e}]  loose bound={:.4e} (mean cmp {:.2})",
        t.false_convergence,
        t.settled,
        t.false_convergence as f64 / t.settled as f64,
        t.mean_comparisons() / scale,
        t.mean_comparisons(),
    );
}

#[cfg(test)]
mod tests;
