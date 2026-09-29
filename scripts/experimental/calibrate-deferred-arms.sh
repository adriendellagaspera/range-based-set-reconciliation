#!/usr/bin/env bash
# Turn "cluster-scale" into a number of core-hours, on this machine, before renting anything.
#
# Three arms are deferred for compute rather than for method (#10, and the contention prediction
# `benches/README.md` records): the `w`/`τ` = 32 collision rates, the `n` = 10⁸ decade of the
# n-scaling discriminator, and the >=16-real-core contention falsification. Each is deferred on an
# estimate nobody has measured. This measures the three throughputs those estimates need and
# extrapolates, so the provisioning decision is arithmetic rather than a guess.
#
# NOT a gate. Nothing here is wired into a hook or CI: it is a one-off instrument, run by hand when
# the question "what would this cost" comes up. `.claude/rules/gated-checks.md` does not apply --
# there is no gate whose result this replays.
#
# What it measures vs. what it extrapolates is labelled per row. The extrapolations rest on exactly
# two assumptions, both stated where used:
#
#   1. A trial's cost is independent of the fingerprint width. True by construction: `width`/`tau`
#      only selects a mask in `NarrowStore`/`HashCompareStore` (see the arms' own `mask`), so the
#      work per trial is the same at 16 and at 32. This is what makes a w = 16 timing transfer --
#      *within* one arm. Across arms it does not, and not for the reason the aggregates suggest:
#      arm B's aggregate is the dearer of the two (a 256-bit `Fingerprint::combine` per element
#      where arm A does a `wrapping_add`, plus one extra `rsos::digest` over `(size, Sigma)`), yet
#      arm B measures ~16% *faster*, because the difference *generators* dominate -- arm A's
#      `balanced_swap` runs a rejection sampler (`while a.contains(..) || b.contains(..)`, a linear
#      scan per rejection) and arm B's `pure_deletion` is one clone and one `swap_remove`. Which is
#      why each arm is timed on its own rather than priced from the other's throughput.
#   2. Store memory is linear in element count. Approximate: a B-tree's node slack does not scale
#      exactly, so the n = 10⁸ figure is an order-of-magnitude bound, not a budget.
#
# Env:
#   EUR_PER_CORE_HOUR  price to cost the result at (default 0.02, a spot/dedicated-vCPU ballpark;
#                      check it, this script cannot)
#   TARGET_EVENTS_32   events wanted at w = 32 (default 8 -- enough for a two-sided interval that
#                      excludes zero, matching the w = 24 arm's own sizing intent)
#   SKIP_ARM_A / SKIP_MEMORY / SKIP_CONTENTION   set to 1 to skip a slow section (SKIP_ARM_A
#                      skips both rate arms, A and B -- they are one section here)
set -Eeuo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$SCRIPT_DIR/.."

EUR_PER_CORE_HOUR=${EUR_PER_CORE_HOUR:-0.02}
TARGET_EVENTS_32=${TARGET_EVENTS_32:-8}

# `EXPECTED_EVENTS` in tests/aggregate_and_truncation_collision_rates.rs. Kept in sync by hand:
# if that constant changes, the trial counts below are wrong and every extrapolation with them.
ARM_A_TRIALS=$((100 * (1 << 16) + 5 * (1 << 24))) # w=16 at 100 events + w=24 at 5 events

# `WRITER_COUNTS` and `TRIALS` in benches/contention.rs, same hand-sync caveat as above: the full
# sweep is levels x trials, and `WRITER_COUNTS` is [1,2,4,8,16] -- five levels, NOT one per core.
# The first draft multiplied by the core count instead and overstated the sweep by about 10x.
SWEEP_LEVELS=5
SWEEP_TRIALS=30
RED_LEVELS=2 # CONTENTION_WRITERS=1,2 below
RED_TRIALS=3

section() { printf '\n\033[1m== %s\033[0m\n' "$1"; }
row() { printf '  %-34s %s\n' "$1" "$2"; }

# Peak RSS of a command, in KiB, on stdout; the command's own output is discarded.
#
# GNU `time -f %M` is the obvious way and is deliberately not relied on: it is absent from slim
# container images (including the one this script was written on), where `/usr/bin/time` does not
# exist at all. Polling `VmHWM` needs nothing but procfs, and `VmHWM` is a high-water mark, so a
# poll that misses the peak instant still reads it.
peak_rss_kb() {
    "$@" >/dev/null 2>&1 &
    local pid=$! hwm=0 sample
    while kill -0 "$pid" 2>/dev/null; do
        sample=$(awk '/^VmHWM:/{print $2}' "/proc/$pid/status" 2>/dev/null || true)
        [ -n "$sample" ] && [ "$sample" -gt "$hwm" ] && hwm=$sample
        sleep 0.2
    done
    wait "$pid" 2>/dev/null || true
    printf '%s' "$hwm"
}

section "Machine (record this beside any number below)"
row "cores (nproc)" "$(nproc)"
if command -v lscpu >/dev/null 2>&1; then
    row "model" "$(lscpu | sed -n 's/^Model name: *//p' | head -1)"
    row "sockets x cores x threads" \
        "$(lscpu | awk -F: '/^Socket\(s\)/{s=$2} /^Core\(s\) per socket/{c=$2} /^Thread\(s\) per core/{t=$2} END{gsub(/ /,"",s);gsub(/ /,"",c);gsub(/ /,"",t);print s" x "c" x "t}')"
    smt=$(lscpu | awk -F: '/^Thread\(s\) per core/{gsub(/ /,"",$2); print $2}')
    if [ "${smt:-1}" != "1" ]; then
        row "SMT" "ON -- vCPUs are not all physical cores; the contention arm needs ThreadsPerCore=1"
    else
        row "SMT" "off -- every vCPU is a physical core"
    fi
fi
row "RAM total" "$(awk '/^MemTotal:/{printf "%.1f GiB", $2/1048576}' /proc/meminfo)"

# ---------------------------------------------------------------------------------------------
section "Arm 1 -- collision rates at w/tau = 32 (#10)"
# ---------------------------------------------------------------------------------------------
# Both arms are timed, never one from the other: `tau` masks just as `width` does, but arm B's
# per-trial work is not arm A's, and the net difference has the opposite sign to the one the
# aggregates alone predict (assumption 1 in the header has the measured figures).
time_arm() { # $1 test fn -> prints elapsed seconds
    local start
    start=$(date +%s)
    cargo test --release -p rbsr-research --test aggregate_and_truncation_collision_rates \
        -- --ignored --exact "$1" >/dev/null 2>&1
    local elapsed=$(($(date +%s) - start))
    [ "$elapsed" -lt 1 ] && elapsed=1
    printf '%s' "$elapsed"
}

if [ "${SKIP_ARM_A:-0}" = "1" ]; then
    row "MEASURED trials/s" "skipped (SKIP_ARM_A=1)"
else
    # Built first, so neither timing includes a compile.
    cargo test --release -p rbsr-research --test aggregate_and_truncation_collision_rates \
        --no-run -q >/dev/null 2>&1
    echo "  timing both arms at their shipped sizing ($ARM_A_TRIALS trials each); ~15 min total..."
    a_elapsed=$(time_arm arm_a_aggregate_collision_rate)
    b_elapsed=$(time_arm arm_b_comparison_map_collision_rate)
    row "MEASURED wall time, arm A" "${a_elapsed} s for ${ARM_A_TRIALS} trials"
    row "MEASURED wall time, arm B" "${b_elapsed} s for ${ARM_A_TRIALS} trials"

    # trials for E events at width k is E * 2^k: the per-trial event probability is 2^-k, which is
    # the arms' own sizing rule (`EXPECTED_EVENTS`), not an assumption added here.
    awk -v trials_done="$ARM_A_TRIALS" -v ae="$a_elapsed" -v be="$b_elapsed" \
        -v e="$TARGET_EVENTS_32" -v eur="$EUR_PER_CORE_HOUR" '
    BEGIN {
        trials = e * 2^32;
        a_ps = trials_done / ae; b_ps = trials_done / be;
        a_h = trials / a_ps / 3600; b_h = trials / b_ps / 3600;
        printf "  %-34s %.0f (A), %.0f (B)\n", "MEASURED trials/s/core", a_ps, b_ps;
        printf "  %-34s %.3g trials  (%d events x 2^32)\n", "EXTRAPOLATED trials, each arm", trials, e;
        printf "  %-34s %.1f core-hours\n", "EXTRAPOLATED w=32 (arm A)", a_h;
        printf "  %-34s %.1f core-hours\n", "EXTRAPOLATED tau=32 (arm B)", b_h;
        printf "  %-34s %.2f EUR at %.4f/core-h\n", "EXTRAPOLATED both arms", (a_h+b_h)*eur, eur;
        printf "  %-34s %.1f h on 32 cores, %.1f h on 96\n", "EXTRAPOLATED wall clock", (a_h+b_h)/32, (a_h+b_h)/96;
    }'
    row "assumption" "per-trial cost is width-independent (width/tau selects a mask only)"
fi

# ---------------------------------------------------------------------------------------------
section "Arm 2 -- the n = 10^8 decade (#10's discriminator)"
# ---------------------------------------------------------------------------------------------
if [ "${SKIP_MEMORY:-0}" = "1" ]; then
    row "MEASURED bytes/element" "skipped (SKIP_MEMORY=1)"
else
    echo "  building two 10^6-element FingerprintTreeMaps and reading peak RSS..."
    # Built first, so the compiler's own RSS is not what gets measured.
    cargo test --release -p rbsr-research --test sketch_exchange_fragmentation_under_loss \
        --no-run -q >/dev/null 2>&1
    rss_kb=$(peak_rss_kb cargo test --release -p rbsr-research \
        --test sketch_exchange_fragmentation_under_loss \
        -- --ignored --exact headline_case_ranks_the_sketch_against_rbsr_under_loss)
    if [[ "$rss_kb" =~ ^[0-9]+$ ]] && [ "$rss_kb" -gt 0 ]; then
        row "MEASURED peak RSS, two 10^6 stores" "$((rss_kb / 1024)) MiB"
        awk -v rss="$rss_kb" '
        BEGIN {
            per_elem = rss * 1024 / 2e6;
            printf "  %-34s %.0f B  (two stores, 2x10^6 elements)\n", "MEASURED bytes/element", per_elem;
            gib = per_elem*2e8/1073741824;
            printf "  %-34s %.0f GiB for two 10^8 stores\n", "EXTRAPOLATED n=10^8 RAM", gib;
            # Smallest power-of-two VM that holds it with 2x headroom, floored at 4 GiB. Written as
            # arithmetic rather than a threshold: the first draft hard-coded "one 64 GiB VM" for
            # anything under 60 GiB, which was a guess about the magnitude of the answer, and the
            # first real run came back at 5 GiB and printed 64 anyway.
            want = gib*2; if (want < 4) want = 4;
            for (vm = 4; vm < want; vm *= 2) {}
            printf "  %-34s one %d GiB VM (2x headroom on %.0f GiB)\n", "EXTRAPOLATED box", vm, gib;
        }'
        row "assumption" "memory linear in element count (B-tree node slack is not exactly linear)"
        row "caveat" "peak RSS includes the harness, so this over-states bytes/element"
    else
        row "MEASURED bytes/element" "could not read peak RSS (no procfs VmHWM?)"
    fi

    # Wall clock. Two terms, and only one of them matters: `reconciliation_drive` in
    # benches/protocol.rs fits n^0.26 for the default policy (49 us at n = 10^6, so ~160 us at
    # 10^8) while construction fits n^1.15 -- six orders of magnitude apart. So this times
    # construction and treats the drive as free, rather than pretending to price both.
    if [ "${SKIP_FILL:-0}" = "1" ]; then
        row "MEASURED build time" "skipped (SKIP_FILL=1)"
    else
        echo "  timing FingerprintTreeMap::fill over its 10^1..10^6 sweep; ~3 min..."
        fill_log=$(mktemp)
        cargo bench --bench bench -- "FingerprintTreeMap::fill" >"$fill_log" 2>&1
        # Criterion medians, per size, normalised to seconds. Extrapolated on the *last decade's*
        # ratio, not a fit over the whole sweep: the per-decade ratio climbs monotonically here
        # (9.7x, 12.9x, 12.6x, 16.6x, 21.3x), which is the tree outgrowing cache, so a global fit
        # understates the decades that have not been measured.
        awk '
        /^FingerprintTreeMap::fill\/FingerprintTreeMap::fill\// {
            split($0, p, "/"); pending = p[3] + 0; next
        }
        pending && /time:/ {
            v = $4; u = $5
            m = (u ~ /^ns/) ? 1e-9 : (u ~ /^ms/) ? 1e-3 : (u ~ /s$/ && length(u) > 1) ? 1e-6 : 1
            sec[pending] = v * m; if (pending > big) { big2 = big; big = pending }
            pending = 0
        }
        END {
            if (!(big in sec) || !(big2 in sec)) {
                printf "  %-34s %s\n", "MEASURED build time", "could not parse criterion output";
                exit
            }
            r = sec[big] / sec[big2];
            printf "  %-34s %.2f s at n=%d\n", "MEASURED build time, one store", sec[big], big;
            printf "  %-34s %.1fx\n", "MEASURED last-decade ratio", r;
            t = sec[big]; for (n = big; n < 100000000; n *= 10) t *= r;
            printf "  %-34s %.0f s per store, %.0f min for two\n", \
                "EXTRAPOLATED build at n=10^8", t, 2*t/60;
            printf "  %-34s %s\n", "EXTRAPOLATED one full drive", "the same, +<1 s: the drive is free";
        }' "$fill_log"
        rm -f "$fill_log"
        row "caveat" "this bench fills <u32,u32>; the probes use <u64,u64>, so it under-states"
        row "  and" "one drive, not the sweep -- multiply by the trials the discriminator needs"
    fi
fi

# ---------------------------------------------------------------------------------------------
section "Arm 3 -- contention falsification, needs C >= 16 real cores"
# ---------------------------------------------------------------------------------------------
if [ "${SKIP_CONTENTION:-0}" = "1" ]; then
    row "MEASURED s per (N, arm) point" "skipped (SKIP_CONTENTION=1)"
else
    echo "  timing a reduced contention sweep ($RED_LEVELS levels x $RED_TRIALS trials)..."
    start=$(date +%s)
    CONTENTION_WRITERS=1,2 CONTENTION_TRIALS=$RED_TRIALS \
        cargo bench --bench contention >/dev/null 2>&1
    elapsed=$(($(date +%s) - start))
    [ "$elapsed" -lt 1 ] && elapsed=1
    row "MEASURED reduced sweep" "${elapsed} s for $RED_LEVELS levels x $RED_TRIALS trials"
    awk -v el="$elapsed" -v rl="$RED_LEVELS" -v rt="$RED_TRIALS" \
        -v lv="$SWEEP_LEVELS" -v tr="$SWEEP_TRIALS" '
    BEGIN {
        # Cost per (level, trial). The bench pairs its two arms *within* a trial, so a trial
        # already covers both and there is no per-arm factor to apply on top.
        per = el / (rl * rt);
        printf "  %-34s %.1f s\n", "MEASURED per (level, trial)", per;
        printf "  %-34s %.0f min at C=16 (%d levels x %d trials)\n", \
            "EXTRAPOLATED full sweep", per*lv*tr/60, lv, tr;
        printf "  %-34s %.0f min at C=32 (one more level)\n", \
            "EXTRAPOLATED full sweep", per*(lv+1)*tr/60, lv+1;
    }'
    row "assumption" "wall clock ~flat in N while N <= cores (measured here at N=1,2,4: 24/26/23 s)"
    row "  where it breaks" "a real contention collapse inflates the big levels -- that IS the finding"
    row "requirement" "N <= C real cores, exclusive; SMT siblings and noisy neighbours invalidate it"
    row "note" "cross-socket is the stronger test (interconnect handoff), so 2-socket bare metal"
fi

section "Verdict"
cat <<'EOF'
  The three arms above are independent one-off jobs, not a cluster:
    - Arm 1 tolerates interruption, but at the measured price do NOT reach for spot: handling
      preemption costs more engineering time than the whole run costs money. One hourly box.
    - Arm 2 wants nothing rented at all: its own rows size both the RAM and the wall clock, and
      at 10^8 that is single-digit GiB for minutes. Read them; do not assume this is the big one.
    - Arm 3 wants exclusive real cores and a known topology  -> hourly dedicated/bare metal, x86.
  Keep arm 3 on x86: its baseline is a 4-core Xeon and its subject is x86 cache coherence, so an
  ARM box (where 1 vCPU is always 1 physical core, and cheaper) is a confound there while being a
  good fit for arms 1 and 2.
EOF
