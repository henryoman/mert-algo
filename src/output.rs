use std::io::{self, Write};

use anyhow::Result;

use crate::config::OutputFormat;
use crate::types::{BalanceHistoryReport, BalancePoint};

pub fn write_report(format: OutputFormat, report: &BalanceHistoryReport) -> Result<()> {
    match format {
        OutputFormat::Json => write_json(report),
        OutputFormat::Csv => write_csv(&report.balance_history),
    }
}

fn write_json(report: &BalanceHistoryReport) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, report)?;
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
