// Copyright 2026 Developers of the reconcile-rs project.
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::harness::Sample;
use super::wire::DATA;

pub(super) fn print_pair(
    budget: usize,
    name: &str,
    scenario: u64,
    control: &Sample,
    candidate: &Sample,
) {
    let shared_drops = control.drops.intersection(&candidate.drops).count();
    let control_only = control.drops.difference(&candidate.drops).count();
    let candidate_only = candidate
        .drops
        .difference(&control.drops)
        .filter(|key| key.kind == DATA)
        .count();
    println!(
        "[pair] budget={budget} profile={name} scenario={scenario:#x} control_converged={} candidate_converged={} control_ms={} candidate_ms={} control_useful_bytes={} candidate_useful_bytes={} control_data_bytes={} candidate_data_bytes={} candidate_control_bytes={} candidate_retry_bytes={} control_data_frames={} candidate_data_frames={} candidate_control_frames={} candidate_retry_frames={} control_frames={} candidate_frames={} candidate_max_pending_bytes={} candidate_max_pending_frames={} candidate_max_seen_ids={} control_fault_drops={} candidate_fault_drops={} control_duplicates={} candidate_duplicates={} control_reorders={} candidate_reorders={} candidate_data_drops={} candidate_ack_drops={} shared_drop_events={} control_only_drop_events={} candidate_only_drop_events={} candidate_retry_exhausted={} stale_injected={} stale_rejected={}",
        control.converged,
        candidate.converged,
        control.elapsed.as_millis(),
        candidate.elapsed.as_millis(),
        control.metrics.useful_bytes,
        candidate.metrics.useful_bytes,
        control.metrics.first_data_bytes,
        candidate.metrics.first_data_bytes,
        candidate.metrics.control_bytes,
        candidate.metrics.retry_bytes,
        control.metrics.first_data_frames,
        candidate.metrics.first_data_frames,
        candidate.metrics.control_frames,
        candidate.metrics.retry_frames,
        control.metrics.total_frames(),
        candidate.metrics.total_frames(),
        candidate.metrics.max_pending_bytes,
        candidate.metrics.max_pending_frames,
        candidate.metrics.max_seen_ids,
        control.metrics.fault_drops,
        candidate.metrics.fault_drops,
        control.metrics.fault_duplicates,
        candidate.metrics.fault_duplicates,
        control.metrics.fault_reorders,
        candidate.metrics.fault_reorders,
        candidate.metrics.data_fault_drops,
        candidate.metrics.control_fault_drops,
        shared_drops,
        control_only,
        candidate_only,
        candidate.metrics.retry_exhausted,
        candidate.metrics.stale_injected,
        candidate.metrics.stale_rejected,
    );
}

pub(super) fn print_bitmap_triplet(
    budget: usize,
    name: &str,
    scenario: u64,
    control: &Sample,
    per_frame: &Sample,
    bitmap: &Sample,
) {
    let control_bytes = control.metrics.first_data_bytes;
    let per_frame_bytes = per_frame.metrics.first_data_bytes
        + per_frame.metrics.control_bytes
        + per_frame.metrics.retry_bytes;
    let bitmap_bytes =
        bitmap.metrics.first_data_bytes + bitmap.metrics.control_bytes + bitmap.metrics.retry_bytes;
    let control_bitmap_shared = shared_data_drops(control, bitmap);
    let per_frame_bitmap_shared = shared_data_drops(per_frame, bitmap);
    println!(
        "[bitmap] budget={budget} profile={name} scenario={scenario:#x} control_converged={} per_frame_converged={} bitmap_converged={} control_ms={} per_frame_ms={} bitmap_ms={} control_bytes={} per_frame_bytes={} bitmap_bytes={} control_useful_bytes={} per_frame_useful_bytes={} bitmap_useful_bytes={} per_frame_data_bytes={} bitmap_data_bytes={} control_frames={} per_frame_frames={} bitmap_frames={} per_frame_data_frames={} bitmap_data_frames={} per_frame_retry_frames={} bitmap_retry_frames={} per_frame_control_bytes={} bitmap_control_bytes={} per_frame_retry_bytes={} bitmap_retry_bytes={} per_frame_ack_frames={} bitmap_ack_frames={} per_frame_max_pending_bytes={} bitmap_max_pending_bytes={} bitmap_max_pending_frames={} bitmap_max_seen_ids={} bitmap_max_receiver_flights={} control_data_drops={} per_frame_data_drops={} bitmap_data_drops={} per_frame_ack_drops={} bitmap_ack_drops={} control_bitmap_shared_drops={} per_frame_bitmap_shared_drops={} bitmap_retry_exhausted={} bitmap_stale_injected={} bitmap_stale_rejected={} bitmap_duplicates={} bitmap_reorders={}",
        control.converged,
        per_frame.converged,
        bitmap.converged,
        control.elapsed.as_millis(),
        per_frame.elapsed.as_millis(),
        bitmap.elapsed.as_millis(),
        control_bytes,
        per_frame_bytes,
        bitmap_bytes,
        control.metrics.useful_bytes,
        per_frame.metrics.useful_bytes,
        bitmap.metrics.useful_bytes,
        per_frame.metrics.first_data_bytes,
        bitmap.metrics.first_data_bytes,
        control.metrics.total_frames(),
        per_frame.metrics.total_frames(),
        bitmap.metrics.total_frames(),
        per_frame.metrics.first_data_frames,
        bitmap.metrics.first_data_frames,
        per_frame.metrics.retry_frames,
        bitmap.metrics.retry_frames,
        per_frame.metrics.control_bytes,
        bitmap.metrics.control_bytes,
        per_frame.metrics.retry_bytes,
        bitmap.metrics.retry_bytes,
        per_frame.metrics.control_frames,
        bitmap.metrics.control_frames,
        per_frame.metrics.max_pending_bytes,
        bitmap.metrics.max_pending_bytes,
        bitmap.metrics.max_pending_frames,
        bitmap.metrics.max_seen_ids,
        bitmap.metrics.max_receiver_flights,
        control.metrics.data_fault_drops,
        per_frame.metrics.data_fault_drops,
        bitmap.metrics.data_fault_drops,
        per_frame.metrics.control_fault_drops,
        bitmap.metrics.control_fault_drops,
        control_bitmap_shared,
        per_frame_bitmap_shared,
        bitmap.metrics.retry_exhausted,
        bitmap.metrics.stale_injected,
        bitmap.metrics.stale_rejected,
        bitmap.metrics.fault_duplicates,
        bitmap.metrics.fault_reorders,
    );
}

fn shared_data_drops(left: &Sample, right: &Sample) -> usize {
    left.drops
        .intersection(&right.drops)
        .filter(|key| key.kind == DATA)
        .count()
}
