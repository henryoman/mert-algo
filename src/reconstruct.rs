use std::collections::HashSet;

use crate::types::{
    BalanceHistoryReport, BalancePoint, RunMetrics, SolPnlSummary, TransactionEvent,
};

pub fn build_balance_history_report(
    address: String,
    events: &mut Vec<TransactionEvent>,
    metrics: RunMetrics,
) -> BalanceHistoryReport {
    events.sort_by(|a, b| {
        (a.slot, a.transaction_index, a.signature.as_str()).cmp(&(
            b.slot,
            b.transaction_index,
            b.signature.as_str(),
        ))
    });

    let mut seen = HashSet::with_capacity(events.len());
    events.retain(|event| seen.insert(event.signature.clone()));

    let mut points = Vec::with_capacity(events.len());

    for event in events.iter() {
        points.push(BalancePoint {
            signature: event.signature.clone(),
            slot: event.slot,
            transaction_index: event.transaction_index,
            block_time: event.block_time,
            delta_lamports: event.delta_lamports,
            balance_lamports: i128::from(event.post_balance_lamports),
            fee_lamports: if event.is_fee_payer {
                event.fee_lamports
            } else {
                0
            },
            err: event.err.clone(),
        });
    }

    let summary = summarize(address, events, &points);

    BalanceHistoryReport {
        summary,
        metrics,
        balance_history: points,
    }
}

fn summarize(
    address: String,
    events: &[TransactionEvent],
    points: &[BalancePoint],
) -> SolPnlSummary {
    let first = events.first();
    let last = events.last();
    let start_balance = first.map(|event| event.pre_balance_lamports);
    let end_balance = last.map(|event| event.post_balance_lamports);
    let net_change = match (start_balance, end_balance) {
        (Some(start), Some(end)) => i128::from(end) - i128::from(start),
        _ => 0,
    };

    let gross_inflow = events
        .iter()
        .filter(|event| event.delta_lamports > 0)
        .map(|event| i128::from(event.delta_lamports))
        .sum();
    let gross_outflow = events
        .iter()
        .filter(|event| event.delta_lamports < 0)
        .map(|event| i128::from(-event.delta_lamports))
        .sum();
    let fees_paid = events
        .iter()
        .filter(|event| event.is_fee_payer)
        .map(|event| event.fee_lamports)
        .sum();

    SolPnlSummary {
        address,
        transaction_count: events.len(),
        failed_transaction_count: events.iter().filter(|event| event.err.is_some()).count(),
        first_slot: first.map(|event| event.slot),
        last_slot: last.map(|event| event.slot),
        first_block_time: first.and_then(|event| event.block_time),
        last_block_time: last.and_then(|event| event.block_time),
        start_balance_lamports: start_balance,
        end_balance_lamports: end_balance,
        net_change_lamports: net_change,
        gross_inflow_lamports: gross_inflow,
        gross_outflow_lamports: gross_outflow,
        fees_paid_lamports: fees_paid,
        pnl_lamports: net_change,
        pnl_policy: "native_sol_balance_delta_only_no_external_flow_classification",
        checksum: checksum(points),
    }
}

fn checksum(points: &[BalancePoint]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for point in points {
        hash = fnv(hash, point.signature.as_bytes());
        hash = fnv(hash, &point.slot.to_le_bytes());
        hash = fnv(hash, &point.transaction_index.to_le_bytes());
        hash = fnv(hash, &point.delta_lamports.to_le_bytes());
        hash = fnv(hash, &point.balance_lamports.to_le_bytes());
    }
    hash
}

fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(signature: &str, slot: u64, index: u64, pre: u64, post: u64) -> TransactionEvent {
        TransactionEvent {
            signature: signature.to_string(),
            slot,
            transaction_index: index,
            block_time: None,
            err: None,
            account_index: 0,
            fee_lamports: 5_000,
            is_fee_payer: true,
            pre_balance_lamports: pre,
            post_balance_lamports: post,
            delta_lamports: post as i64 - pre as i64,
        }
    }

    #[test]
    fn uses_post_balance_not_zero_based_accumulation() {
        let mut events = vec![event("b", 2, 0, 90, 120), event("a", 1, 0, 100, 90)];

        let report = build_balance_history_report("addr".to_string(), &mut events, metrics());

        assert_eq!(report.balance_history[0].balance_lamports, 90);
        assert_eq!(report.balance_history[1].balance_lamports, 120);
        assert_eq!(report.summary.start_balance_lamports, Some(100));
        assert_eq!(report.summary.end_balance_lamports, Some(120));
        assert_eq!(report.summary.net_change_lamports, 20);
    }

    #[test]
    fn dedupes_by_signature_after_sorting() {
        let mut events = vec![
            event("a", 1, 0, 100, 90),
            event("a", 1, 0, 100, 90),
            event("b", 2, 0, 90, 95),
        ];

        let report = build_balance_history_report("addr".to_string(), &mut events, metrics());

        assert_eq!(report.balance_history.len(), 2);
        assert_eq!(report.summary.fees_paid_lamports, 10_000);
    }

    fn metrics() -> RunMetrics {
        RunMetrics {
            strategy: "test".to_string(),
            elapsed_ms: 0,
            rpc_requests: 0,
            full_pages: 0,
            signature_pages: 0,
            decoded_events: 0,
            partitions: 1,
            page_limit: 100,
            concurrency: 1,
        }
    }
}
