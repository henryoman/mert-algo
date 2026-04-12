use anyhow::{bail, Result};

use crate::types::TransactionEvent;

pub async fn fetch_baseline_history(_rpc_url: &str, _address: &str) -> Result<Vec<TransactionEvent>> {
    bail!("baseline mode is planned but not implemented yet")
}
