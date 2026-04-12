use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::{stream, StreamExt, TryStreamExt};
use helius::error::HeliusError;
use helius::types::inner::{
    GetTransactionsFilters, SlotFilter, TransactionDetails, TransactionEntry,
    TransactionStatusFilter,
};
use helius::types::{Cluster, GetTransactionsForAddressOptions, SortOrder, UiTransactionEncoding};
use helius::{Helius, HeliusBuilder};
use solana_commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedTransaction, UiMessage, UiTransactionStatusMeta,
};

use crate::config::HistoryRequest;
use crate::types::TransactionEvent;

pub async fn fetch_helius_simple_history(
    request: &HistoryRequest,
) -> Result<Vec<TransactionEvent>> {
    let helius = build_client(request).await?;
    fetch_full_range(&helius, request, request.start_slot, request.end_slot).await
}

pub async fn fetch_helius_optimized_history(
    request: &HistoryRequest,
) -> Result<Vec<TransactionEvent>> {
    let helius = build_client(request).await?;

    let (first_slot, last_slot) = match discover_slot_bounds(&helius, request).await? {
        Some(bounds) => bounds,
        None => return Ok(Vec::new()),
    };

    let start_slot = request.start_slot.unwrap_or(first_slot);
    let end_slot = request.end_slot.unwrap_or(last_slot);
    if start_slot > end_slot {
        return Ok(Vec::new());
    }

    let partitions = request
        .partitions
        .unwrap_or_else(|| request.concurrency.saturating_mul(4).max(1));
    let ranges = partition_slots(start_slot, end_slot, partitions);

    stream::iter(ranges)
        .map(|(start, end)| {
            let helius = &helius;
            async move { fetch_full_range(helius, request, Some(start), Some(end)).await }
        })
        .buffer_unordered(request.concurrency)
        .try_fold(Vec::new(), |mut all, mut shard| async move {
            all.append(&mut shard);
            Ok(all)
        })
        .await
}

async fn build_client(request: &HistoryRequest) -> Result<Helius> {
    let mut builder = HeliusBuilder::new()
        .with_api_key(request.api_key.clone())
        .context("failed to configure Helius API key")?
        .with_commitment(CommitmentConfig {
            commitment: commitment_level(&request.commitment)?,
        });

    if request.rpc_url.trim().is_empty() {
        builder = builder.with_cluster(Cluster::MainnetBeta);
    } else {
        builder = builder
            .with_custom_url(&request.rpc_url)
            .context("invalid --rpc-url")?;
    }

    builder
        .build()
        .await
        .context("failed to build Helius client")
}

async fn fetch_full_range(
    helius: &Helius,
    request: &HistoryRequest,
    start_slot: Option<u64>,
    end_slot: Option<u64>,
) -> Result<Vec<TransactionEvent>> {
    let mut pagination_token = None;
    let mut events = Vec::new();
    let mut page_count = 0usize;

    loop {
        if request
            .max_pages
            .map(|max_pages| page_count >= max_pages)
            .unwrap_or(false)
        {
            break;
        }

        let options = GetTransactionsForAddressOptions {
            limit: Some(request.page_limit),
            transaction_details: Some(TransactionDetails::Full),
            sort_order: Some(SortOrder::Asc),
            pagination_token: pagination_token.clone(),
            commitment: Some(commitment_level(&request.commitment)?),
            filters: filters(start_slot, end_slot, Some(TransactionStatusFilter::Any)),
            encoding: Some(UiTransactionEncoding::Json),
            max_supported_transaction_version: Some(0),
            ..Default::default()
        };

        let result = with_retry(|| {
            let rpc = helius.rpc();
            let address = request.address.clone();
            let options = options.clone();
            async move { rpc.get_transactions_for_address(address, options).await }
        })
        .await?;

        for entry in &result.data {
            if let Some(event) = decode_entry(entry, &request.address)? {
                events.push(event);
            }
        }

        page_count += 1;
        pagination_token = result.pagination_token;
        if pagination_token.is_none() {
            break;
        }
    }

    Ok(events)
}

async fn discover_slot_bounds(
    helius: &Helius,
    request: &HistoryRequest,
) -> Result<Option<(u64, u64)>> {
    let first = fetch_signature_edge(helius, request, SortOrder::Asc).await?;
    let last = fetch_signature_edge(helius, request, SortOrder::Desc).await?;

    match (first, last) {
        (Some(first), Some(last)) => Ok(Some((first, last))),
        _ => Ok(None),
    }
}

async fn fetch_signature_edge(
    helius: &Helius,
    request: &HistoryRequest,
    sort_order: SortOrder,
) -> Result<Option<u64>> {
    let options = GetTransactionsForAddressOptions {
        limit: Some(1),
        transaction_details: Some(TransactionDetails::Signatures),
        sort_order: Some(sort_order),
        commitment: Some(commitment_level(&request.commitment)?),
        filters: filters(
            request.start_slot,
            request.end_slot,
            Some(TransactionStatusFilter::Any),
        ),
        ..Default::default()
    };

    let result = with_retry(|| {
        let rpc = helius.rpc();
        let address = request.address.clone();
        let options = options.clone();
        async move { rpc.get_transactions_for_address(address, options).await }
    })
    .await?;

    for entry in result.data {
        if let TransactionEntry::Signature(signature) = entry {
            return Ok(Some(signature.slot));
        }
    }

    Ok(None)
}

fn filters(
    start_slot: Option<u64>,
    end_slot: Option<u64>,
    status: Option<TransactionStatusFilter>,
) -> Option<GetTransactionsFilters> {
    if start_slot.is_none() && end_slot.is_none() && status.is_none() {
        return None;
    }

    Some(GetTransactionsFilters {
        slot: if start_slot.is_some() || end_slot.is_some() {
            Some(SlotFilter {
                gte: start_slot,
                lte: end_slot,
                ..Default::default()
            })
        } else {
            None
        },
        status,
        ..Default::default()
    })
}

fn partition_slots(start_slot: u64, end_slot: u64, partitions: usize) -> Vec<(u64, u64)> {
    let total_slots = end_slot - start_slot + 1;
    let partitions = partitions.max(1).min(total_slots as usize);
    let width = total_slots.div_ceil(partitions as u64);

    let mut ranges = Vec::with_capacity(partitions);
    let mut start = start_slot;
    while start <= end_slot {
        let end = start.saturating_add(width - 1).min(end_slot);
        ranges.push((start, end));
        if end == end_slot {
            break;
        }
        start = end + 1;
    }
    ranges
}

fn decode_entry(entry: &TransactionEntry, address: &str) -> Result<Option<TransactionEvent>> {
    let TransactionEntry::Full(tx) = entry else {
        return Ok(None);
    };

    let meta = tx
        .meta
        .as_ref()
        .ok_or_else(|| anyhow!("transaction at slot {} has no metadata", tx.slot))?;
    let account_keys = account_keys(&tx.transaction, meta)?;
    let Some(account_index) = account_keys.iter().position(|key| key == address) else {
        return Ok(None);
    };

    let pre_balance = *meta
        .pre_balances
        .get(account_index)
        .ok_or_else(|| anyhow!("missing pre balance for account index {account_index}"))?;
    let post_balance = *meta
        .post_balances
        .get(account_index)
        .ok_or_else(|| anyhow!("missing post balance for account index {account_index}"))?;

    let signature = first_signature(&tx.transaction)
        .ok_or_else(|| anyhow!("transaction at slot {} has no signature", tx.slot))?;
    let delta = i128::from(post_balance) - i128::from(pre_balance);
    let delta_lamports = i64::try_from(delta).context("lamport delta does not fit in i64")?;

    Ok(Some(TransactionEvent {
        signature,
        slot: tx.slot,
        transaction_index: tx.transaction_index,
        block_time: tx.block_time,
        err: meta.err.as_ref().map(|err| format!("{err:?}")),
        account_index,
        fee_lamports: meta.fee,
        is_fee_payer: account_index == 0,
        pre_balance_lamports: pre_balance,
        post_balance_lamports: post_balance,
        delta_lamports,
    }))
}

fn account_keys(
    transaction: &EncodedTransaction,
    meta: &UiTransactionStatusMeta,
) -> Result<Vec<String>> {
    let mut keys = match transaction {
        EncodedTransaction::Json(tx) => match &tx.message {
            UiMessage::Raw(message) => message.account_keys.clone(),
            UiMessage::Parsed(message) => message
                .account_keys
                .iter()
                .map(|account| account.pubkey.clone())
                .collect(),
        },
        EncodedTransaction::Accounts(accounts) => accounts
            .account_keys
            .iter()
            .map(|account| account.pubkey.clone())
            .collect(),
        _ => {
            return Err(anyhow!(
                "expected JSON transaction encoding; binary transaction received"
            ))
        }
    };

    if let OptionSerializer::Some(loaded) = meta.loaded_addresses.as_ref() {
        keys.extend(loaded.writable.iter().cloned());
        keys.extend(loaded.readonly.iter().cloned());
    }

    Ok(keys)
}

fn first_signature(transaction: &EncodedTransaction) -> Option<String> {
    match transaction {
        EncodedTransaction::Json(tx) => tx.signatures.first().cloned(),
        EncodedTransaction::Accounts(accounts) => accounts.signatures.first().cloned(),
        _ => None,
    }
}

fn commitment_level(commitment: &str) -> Result<CommitmentLevel> {
    match commitment {
        "confirmed" => Ok(CommitmentLevel::Confirmed),
        "finalized" => Ok(CommitmentLevel::Finalized),
        unsupported => Err(anyhow!(
            "unsupported commitment {unsupported:?}; Helius getTransactionsForAddress supports confirmed or finalized"
        )),
    }
}

async fn with_retry<T, Fut, F>(mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = helius::error::Result<T>>,
{
    let max_retries = 4;
    for attempt in 0..=max_retries {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) if is_retryable(&error) && attempt < max_retries => {
                tokio::time::sleep(Duration::from_millis(250 * 2_u64.pow(attempt))).await;
            }
            Err(error) => return Err(anyhow!(redact_api_keys(&error.to_string()))),
        }
    }

    unreachable!("retry loop always returns")
}

fn is_retryable(error: &HeliusError) -> bool {
    matches!(
        error,
        HeliusError::RateLimitExceeded { .. }
            | HeliusError::InternalError { .. }
            | HeliusError::Timeout { .. }
            | HeliusError::Network(_)
            | HeliusError::ReqwestError(_)
    )
}

fn redact_api_keys(message: &str) -> String {
    let Some(start) = message.find("api-key=") else {
        return message.to_string();
    };

    let value_start = start + "api-key=".len();
    let value_end = message[value_start..]
        .find(['&', ')', ' '])
        .map(|offset| value_start + offset)
        .unwrap_or(message.len());

    let mut redacted = String::with_capacity(message.len());
    redacted.push_str(&message[..value_start]);
    redacted.push_str("<redacted>");
    redacted.push_str(&message[value_end..]);
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn partitions_slot_ranges_without_overlap() {
        assert_eq!(
            partition_slots(10, 19, 3),
            vec![(10, 13), (14, 17), (18, 19)]
        );
        assert_eq!(partition_slots(10, 10, 8), vec![(10, 10)]);
    }

    #[test]
    fn decodes_full_transaction_balance_delta() {
        let raw = json!({
            "slot": 1054,
            "transactionIndex": 42,
            "transaction": {
                "signatures": ["sig1"],
                "message": {
                    "accountKeys": [
                        "target",
                        "11111111111111111111111111111111"
                    ],
                    "header": {
                        "numReadonlySignedAccounts": 0,
                        "numReadonlyUnsignedAccounts": 1,
                        "numRequiredSignatures": 1
                    },
                    "instructions": [],
                    "recentBlockhash": "hash"
                }
            },
            "meta": {
                "err": null,
                "status": { "Ok": null },
                "fee": 5000,
                "preBalances": [1000000000, 0],
                "postBalances": [999995000, 0],
                "innerInstructions": [],
                "logMessages": [],
                "preTokenBalances": [],
                "postTokenBalances": [],
                "rewards": []
            },
            "blockTime": 1641038400
        });
        let tx: helius::types::inner::FullTransactionEntry =
            serde_json::from_value(raw).expect("fixture should deserialize");
        let entry = TransactionEntry::Full(Box::new(tx));

        let event = decode_entry(&entry, "target")
            .expect("decode should succeed")
            .expect("target should be present");

        assert_eq!(event.signature, "sig1");
        assert_eq!(event.transaction_index, 42);
        assert_eq!(event.pre_balance_lamports, 1_000_000_000);
        assert_eq!(event.post_balance_lamports, 999_995_000);
        assert_eq!(event.delta_lamports, -5_000);
        assert!(event.is_fee_payer);
    }

    #[test]
    fn redacts_api_keys_from_errors() {
        let message = "Network error: url (https://mainnet.helius-rpc.com/?api-key=secret-value)";

        assert_eq!(
            redact_api_keys(message),
            "Network error: url (https://mainnet.helius-rpc.com/?api-key=<redacted>)"
        );
    }
}
