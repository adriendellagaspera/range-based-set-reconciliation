use devkit::protocol_cost::{MAX_DATAGRAM_PAYLOAD, MTU_FRAGMENT_PAYLOAD};

use super::model::{RibltMeasurement, RibltProjection, WireUse};
use set_reconciliation_comparators::iblt::transition::CELL_BYTES;

const SMALL_MESSAGE_BYTES: u64 = 64;
pub(super) const CODED_SYMBOL_BYTES: u64 = 48;
const STOP_BYTES: u64 = 64;

pub(super) fn message_shape(bytes: u64) -> WireUse {
    let (_, fragments, exposed) = chunked_shape(bytes);
    WireUse {
        bytes,
        fragments,
        loss_exposed_fragments: exposed,
        rounds: 0,
    }
}

pub(super) fn iblt_attempt(cells: usize, terminal: bool) -> WireUse {
    let offer = message_shape(cells as u64 * CELL_BYTES);
    let feedback = message_shape(SMALL_MESSAGE_BYTES);
    let terminal = if terminal {
        message_shape(SMALL_MESSAGE_BYTES)
    } else {
        WireUse::default()
    };
    let mut total = offer.plus(feedback).plus(terminal);
    total.rounds = 1;
    total
}

pub(super) fn riblt_projection(
    measured: RibltMeasurement,
    rtt_seconds: f64,
    bandwidth_bytes_per_second: f64,
    loss_probability: f64,
) -> RibltProjection {
    let symbols_per_datagram = (MTU_FRAGMENT_PAYLOAD as u64 / CODED_SYMBOL_BYTES).max(1);
    let data_datagrams = (measured.prefix as u64).div_ceil(symbols_per_datagram);
    let datagrams = data_datagrams + 1;
    let base_bytes = measured.prefix as u64 * CODED_SYMBOL_BYTES + STOP_BYTES;
    let survival = 1.0 - loss_probability;
    let expected_wire = if survival == 0.0 {
        f64::INFINITY
    } else {
        base_bytes as f64 / survival
    };
    let expected_retry_rounds = expected_max_retries(datagrams, loss_probability);
    let steady_seconds = measured.stream_local_elapsed_seconds
        + expected_wire / bandwidth_bytes_per_second
        + (1.0 + expected_retry_rounds) * rtt_seconds;
    let stop_overshoot_bytes = (bandwidth_bytes_per_second * rtt_seconds / 2.0).ceil() as u64;
    let line_rate_stop_bytes = if survival == 0.0 {
        u64::MAX
    } else {
        ((base_bytes + stop_overshoot_bytes) as f64 / survival).ceil() as u64
    };

    RibltProjection {
        steady_seconds,
        ideal_stop_bytes: if expected_wire.is_finite() {
            expected_wire.ceil() as u64
        } else {
            u64::MAX
        },
        line_rate_stop_bytes,
        datagrams,
        stop_overshoot_bytes,
        expected_retry_rounds,
    }
}

pub(super) fn bulk_shape(bytes: u64) -> WireUse {
    let (_, fragments, exposed) = chunked_shape(bytes);
    WireUse {
        bytes,
        fragments,
        loss_exposed_fragments: exposed,
        rounds: 1,
    }
}

fn chunked_shape(bytes: u64) -> (u64, u64, u64) {
    if bytes == 0 {
        return (0, 0, 0);
    }
    let datagram = MAX_DATAGRAM_PAYLOAD as u64;
    let fragment = MTU_FRAGMENT_PAYLOAD as u64;
    let full = bytes / datagram;
    let remainder = bytes % datagram;
    let full_fragments = datagram.div_ceil(fragment);
    let datagrams = full + (remainder > 0) as u64;
    let fragments = full * full_fragments
        + if remainder == 0 {
            0
        } else {
            remainder.div_ceil(fragment)
        };
    let exposed = if full > 0 {
        full_fragments
    } else {
        remainder.div_ceil(fragment)
    };
    (datagrams, fragments, exposed)
}

fn expected_max_retries(datagrams: u64, loss_probability: f64) -> f64 {
    if datagrams == 0 || loss_probability == 0.0 {
        return 0.0;
    }
    if loss_probability == 1.0 {
        return f64::INFINITY;
    }
    let mut expected = 0.0;
    let mut p_to_k = loss_probability;
    for _ in 0..1_000 {
        let tail = 1.0 - (1.0 - p_to_k).powf(datagrams as f64);
        expected += tail;
        if tail < 1e-12 {
            break;
        }
        p_to_k *= loss_probability;
    }
    expected
}
