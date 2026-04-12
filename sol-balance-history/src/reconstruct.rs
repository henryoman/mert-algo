use crate::types::{BalancePoint, TransactionEvent};

pub fn reconstruct_balance_history(events: &mut Vec<TransactionEvent>) -> Vec<BalancePoint> {
    events.sort_by(|a, b| {
        (a.slot, a.transaction_index, a.signature.as_str()).cmp(&(
            b.slot,
            b.transaction_index,
            b.signature.as_str(),
        ))
    });

    let mut balance: i128 = 0;
    let mut points = Vec::with_capacity(events.len());

    for event in events {
        balance += i128::from(event.delta_lamports);
        points.push(BalancePoint {
            signature: event.signature.clone(),
            slot: event.slot,
            transaction_index: event.transaction_index,
            block_time: event.block_time,
            balance_lamports: balance,
        });
    }

    points
}
