mod baseline;
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
    let cli = Cli::parse();

    let mut events = match cli.mode {
        Mode::Baseline => baseline::fetch_baseline_history(&cli.rpc_url, &cli.address).await?,
        Mode::Simple => {
            helius_simple::fetch_helius_simple_history(&cli.rpc_url, &cli.api_key, &cli.address)
                .await?
        }
        Mode::Optimized => anyhow::bail!("optimized mode is planned but not implemented yet"),
    };

    let points = reconstruct::reconstruct_balance_history(&mut events);
    output::write_balance_points(cli.format, &points)?;

    Ok(())
}
