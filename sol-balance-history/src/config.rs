use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Parser)]
#[command(name = "sol-balance-history")]
#[command(about = "Reconstruct native SOL balance history from RPC")]
pub struct Cli {
    #[arg(long)]
    pub address: String,

    #[arg(long, value_enum, default_value_t = Mode::Simple)]
    pub mode: Mode,

    #[arg(long)]
    pub rpc_url: String,

    #[arg(long)]
    pub api_key: String,

    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,

    #[arg(long, default_value = "finalized")]
    pub commitment: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum Mode {
    Baseline,
    Simple,
    Optimized,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Json,
    Csv,
}
