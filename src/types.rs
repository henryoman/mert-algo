use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TransactionEvent {
    pub signature: String,
    pub slot: u64,
    pub transaction_index: u32,
    pub block_time: Option<i64>,
    pub err: Option<String>,
    pub pre_balance_lamports: u64,
    pub post_balance_lamports: u64,
    pub delta_lamports: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalancePoint {
    pub signature: String,
    pub slot: u64,
    pub transaction_index: u32,
    pub block_time: Option<i64>,
    pub balance_lamports: i128,
}
