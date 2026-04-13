use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TransactionEvent {
    pub signature: String,
    pub slot: u64,
    pub transaction_index: u64,
    pub block_time: Option<i64>,
    pub err: Option<String>,
    pub account_index: usize,
    pub fee_lamports: u64,
    pub is_fee_payer: bool,
    pub pre_balance_lamports: u64,
    pub post_balance_lamports: u64,
    pub delta_lamports: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalancePoint {
    pub signature: String,
    pub slot: u64,
    pub transaction_index: u64,
    pub block_time: Option<i64>,
    pub delta_lamports: i64,
    pub balance_lamports: i128,
    pub fee_lamports: u64,
    pub err: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunMetrics {
    pub strategy: String,
    pub elapsed_ms: u128,
    pub rpc_requests: u64,
    pub full_pages: u64,
    pub signature_pages: u64,
    pub decoded_events: usize,
    pub partitions: usize,
    pub page_limit: u32,
    pub concurrency: usize,
}

#[derive(Debug, Clone)]
pub struct HistoryRun {
    pub events: Vec<TransactionEvent>,
    pub metrics: RunMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct SolPnlSummary {
    pub address: String,
    pub transaction_count: usize,
    pub failed_transaction_count: usize,
    pub first_slot: Option<u64>,
    pub last_slot: Option<u64>,
    pub first_block_time: Option<i64>,
    pub last_block_time: Option<i64>,
    pub start_balance_lamports: Option<u64>,
    pub end_balance_lamports: Option<u64>,
    pub net_change_lamports: i128,
    pub gross_inflow_lamports: i128,
    pub gross_outflow_lamports: i128,
    pub fees_paid_lamports: u64,
    pub pnl_lamports: i128,
    pub pnl_policy: &'static str,
    pub checksum: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceHistoryReport {
    pub summary: SolPnlSummary,
    pub metrics: RunMetrics,
    pub balance_history: Vec<BalancePoint>,
}
