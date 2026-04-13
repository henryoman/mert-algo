use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Parser)]
#[command(name = "sol-balance-history")]
#[command(
    about = "Reconstruct native SOL balance history and SOL-denominated wallet PnL from Helius RPC"
)]
pub struct Cli {
    #[arg(long)]
    pub address: String,

    #[arg(long, value_enum, default_value_t = Mode::Simple)]
    pub mode: Mode,

    #[arg(long, default_value = "https://beta.helius-rpc.com/")]
    pub rpc_url: String,

    #[arg(long)]
    pub api_key: Option<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,

    #[arg(long, default_value = "finalized")]
    pub commitment: String,

    #[arg(long, default_value_t = 100)]
    pub page_limit: u32,

    #[arg(long)]
    pub max_pages: Option<usize>,

    #[arg(long, default_value_t = 8)]
    pub concurrency: usize,

    #[arg(long)]
    pub partitions: Option<usize>,

    #[arg(long)]
    pub start_slot: Option<u64>,

    #[arg(long)]
    pub end_slot: Option<u64>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum Mode {
    Simple,
    Optimized,
    Adaptive,
    Mapped,
    Pipelined,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Json,
    Csv,
}

#[derive(Debug, Clone)]
pub struct HistoryRequest {
    pub address: String,
    pub rpc_url: String,
    pub api_key: String,
    pub commitment: String,
    pub page_limit: u32,
    pub max_pages: Option<usize>,
    pub concurrency: usize,
    pub partitions: Option<usize>,
    pub start_slot: Option<u64>,
    pub end_slot: Option<u64>,
}

impl Cli {
    pub fn into_history_request(self) -> Result<HistoryRequest> {
        let api_key = match self.api_key {
            Some(api_key) => api_key,
            None => std::env::var("HELIUS_API_KEY")
                .context("missing --api-key and HELIUS_API_KEY is not set")?,
        };

        anyhow::ensure!(
            (1..=100).contains(&self.page_limit),
            "--page-limit must be between 1 and 100 for transactionDetails=full"
        );
        anyhow::ensure!(self.concurrency > 0, "--concurrency must be greater than 0");
        anyhow::ensure!(
            matches!(self.commitment.as_str(), "finalized" | "confirmed"),
            "--commitment must be finalized or confirmed"
        );

        if let (Some(start), Some(end)) = (self.start_slot, self.end_slot) {
            anyhow::ensure!(start <= end, "--start-slot must be <= --end-slot");
        }

        Ok(HistoryRequest {
            address: self.address,
            rpc_url: self.rpc_url,
            api_key,
            commitment: self.commitment,
            page_limit: self.page_limit,
            max_pages: self.max_pages,
            concurrency: self.concurrency,
            partitions: self.partitions,
            start_slot: self.start_slot,
            end_slot: self.end_slot,
        })
    }
}
