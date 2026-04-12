use std::io::{self, Write};

use anyhow::Result;

use crate::config::OutputFormat;
use crate::types::BalancePoint;

pub fn write_balance_points(format: OutputFormat, points: &[BalancePoint]) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(points),
        OutputFormat::Csv => write_csv(points),
    }
}

fn write_json(points: &[BalancePoint]) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, points)?;
    handle.write_all(b"\n")?;
    Ok(())
}

fn write_csv(points: &[BalancePoint]) -> Result<()> {
    let stdout = io::stdout();
    let handle = stdout.lock();
    let mut writer = csv::Writer::from_writer(handle);

    for point in points {
        writer.serialize(point)?;
    }

    writer.flush()?;
    Ok(())
}
