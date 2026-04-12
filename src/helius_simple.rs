use anyhow::{bail, Result};

use crate::types::TransactionEvent;

pub async fn fetch_helius_simple_history(
    _rpc_url: &str,
    _api_key: &str,
    _address: &str,
) -> Result<Vec<TransactionEvent>> {
    bail!("simple helius mode is planned but not implemented yet")
}
