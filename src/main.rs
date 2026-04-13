mod config;
mod helius_simple;
mod output;
mod reconstruct;
mod types;

use anyhow::Result;
use clap::Parser;

use crate::config::{Cli, Mode};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    let mode = cli.mode;
    let request = cli.clone().into_history_request()?;

    let mut run = match cli.mode {
        Mode::Simple => helius_simple::fetch_helius_simple_history(&request).await?,
        Mode::Optimized => helius_simple::fetch_helius_optimized_history(&request).await?,
        Mode::Adaptive => helius_simple::fetch_helius_adaptive_history(&request).await?,
    };

    let report =
        reconstruct::build_balance_history_report(request.address, &mut run.events, run.metrics);
    output::write_report(cli.format, &report)?;

    if matches!(mode, Mode::Optimized) && report.summary.transaction_count == 0 {
        eprintln!("No transactions found in the requested range.");
    }

    Ok(())
}
