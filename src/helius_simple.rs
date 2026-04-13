use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use futures::{stream, stream::FuturesUnordered, StreamExt, TryStreamExt};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::HistoryRequest;
use crate::types::{HistoryRun, RunMetrics, TransactionEvent};

const SIGNATURE_PAGE_LIMIT: u32 = 1000;

#[derive(Default)]
struct MetricsState {
    rpc_requests: AtomicU64,
    full_pages: AtomicU64,
    signature_pages: AtomicU64,
}

#[derive(Debug, Clone)]
struct SignaturePoint {
    signature: String,
    slot: u64,
    transaction_index: u64,
}

#[derive(Debug, Clone)]
struct RawHeliusClient {
    client: Client,
    url: String,
}

pub async fn fetch_helius_simple_history(request: &HistoryRequest) -> Result<HistoryRun> {
    let started_at = Instant::now();
    let metrics = Arc::new(MetricsState::default());
    let client = RawHeliusClient::new(request)?;
    let mut events = Vec::new();
    events.push(
        fetch_full_range(
            &client,
            request,
            request.start_slot,
            request.end_slot,
            &metrics,
        )
        .await?,
    );

    Ok(HistoryRun {
        metrics: run_metrics(
            "simple",
            started_at,
            request,
            &metrics,
            events.first().map(|e| e.len()).unwrap_or(0),
            1,
        ),
        events,
    })
}

pub async fn fetch_helius_optimized_history(request: &HistoryRequest) -> Result<HistoryRun> {
    let started_at = Instant::now();
    let metrics = Arc::new(MetricsState::default());
    let client = RawHeliusClient::new(request)?;

    let Some((start_slot, end_slot)) = effective_slot_bounds(&client, request, &metrics).await?
    else {
        return Ok(empty_run("optimized", started_at, request, &metrics));
    };

    let partitions = desired_partitions(request);
    let ranges = partition_slots(start_slot, end_slot, partitions);
    let partition_count = ranges.len();
    let events = fetch_ranges_parallel(&client, request, ranges, &metrics).await?;

    Ok(HistoryRun {
        metrics: run_metrics(
            "optimized",
            started_at,
            request,
            &metrics,
            events.iter().map(|e| e.len()).sum(),
            partition_count,
        ),
        events,
    })
}

pub async fn fetch_helius_adaptive_history(request: &HistoryRequest) -> Result<HistoryRun> {
    let started_at = Instant::now();
    let metrics = Arc::new(MetricsState::default());
    let client = RawHeliusClient::new(request)?;

    let Some((start_slot, end_slot)) = effective_slot_bounds(&client, request, &metrics).await?
    else {
        return Ok(empty_run("adaptive", started_at, request, &metrics));
    };

    let signatures =
        fetch_signature_points(&client, request, start_slot, end_slot, &metrics).await?;
    let ranges = density_or_slot_ranges(
        &signatures,
        start_slot,
        end_slot,
        desired_partitions(request),
    );
    let partition_count = ranges.len();
    let events = fetch_ranges_parallel(&client, request, ranges, &metrics).await?;

    Ok(HistoryRun {
        metrics: run_metrics(
            "adaptive",
            started_at,
            request,
            &metrics,
            events.iter().map(|e| e.len()).sum(),
            partition_count,
        ),
        events,
    })
}

pub async fn fetch_helius_mapped_history(request: &HistoryRequest) -> Result<HistoryRun> {
    let started_at = Instant::now();
    let metrics = Arc::new(MetricsState::default());
    let client = RawHeliusClient::new(request)?;

    let Some((start_slot, end_slot)) = effective_slot_bounds(&client, request, &metrics).await?
    else {
        return Ok(empty_run("mapped", started_at, request, &metrics));
    };

    let discovery_ranges = partition_slots(
        start_slot,
        end_slot,
        desired_partitions(request).max(request.concurrency),
    );
    let mut signature_shards = stream::iter(discovery_ranges)
        .map(|(start, end)| {
            let client = &client;
            let metrics = Arc::clone(&metrics);
            async move { fetch_signature_points(client, request, start, end, &metrics).await }
        })
        .buffer_unordered(request.concurrency)
        .try_collect::<Vec<_>>()
        .await?;

    let mut signatures = Vec::new();
    for shard in &mut signature_shards {
        signatures.append(shard);
    }
    sort_signatures(&mut signatures);

    let ranges = density_or_slot_ranges(
        &signatures,
        start_slot,
        end_slot,
        desired_partitions(request),
    );
    let partition_count = ranges.len();
    let events = fetch_ranges_parallel(&client, request, ranges, &metrics).await?;

    Ok(HistoryRun {
        metrics: run_metrics(
            "mapped",
            started_at,
            request,
            &metrics,
            events.iter().map(|e| e.len()).sum(),
            partition_count,
        ),
        events,
    })
}

pub async fn fetch_helius_pipelined_history(request: &HistoryRequest) -> Result<HistoryRun> {
    let started_at = Instant::now();
    let metrics = Arc::new(MetricsState::default());
    let client = RawHeliusClient::new(request)?;

    let Some((start_slot, end_slot)) = effective_slot_bounds(&client, request, &metrics).await?
    else {
        return Ok(empty_run("pipelined", started_at, request, &metrics));
    };

    let mut pagination_token = None;
    let mut pending = FuturesUnordered::new();
    let mut completed = Vec::new();
    let mut next_sequence = 0usize;

    loop {
        let (points, next_token) = fetch_signature_page(
            &client,
            request,
            start_slot,
            end_slot,
            pagination_token,
            &metrics,
        )
        .await?;

        let ranges = page_density_ranges(&points, request.page_limit as usize);
        for (start, end) in ranges {
            let sequence = next_sequence;
            next_sequence += 1;
            let metrics = Arc::clone(&metrics);
            let client = client.clone();
            pending.push(async move {
                let events =
                    fetch_full_range(&client, request, Some(start), Some(end), &metrics).await?;
                Ok::<_, anyhow::Error>((sequence, events))
            });
        }

        while pending.len() >= request.concurrency {
            if let Some(shard) = pending.next().await {
                completed.push(shard?);
            }
        }

        pagination_token = next_token;
        if pagination_token.is_none() {
            break;
        }
    }

    while let Some(shard) = pending.next().await {
        completed.push(shard?);
    }

    completed.sort_by_key(|(sequence, _)| *sequence);
    let mut events = Vec::new();
    for (_, shard) in completed {
        events.push(shard);
    }

    Ok(HistoryRun {
        metrics: run_metrics(
            "pipelined",
            started_at,
            request,
            &metrics,
            events.iter().map(|e| e.len()).sum(),
            next_sequence,
        ),
        events,
    })
}

impl RawHeliusClient {
    fn new(request: &HistoryRequest) -> Result<Self> {
        let base_url = if request.rpc_url.trim().is_empty() {
            "https://mainnet.helius-rpc.com/"
        } else {
            request.rpc_url.trim()
        };
        let mut url = Url::parse(base_url).context("invalid --rpc-url")?;
        url.query_pairs_mut()
            .append_pair("api-key", &request.api_key);

        let client = Client::builder()
            .pool_max_idle_per_host(request.concurrency.max(16))
            .pool_idle_timeout(Duration::from_secs(90))
            .tcp_nodelay(true)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            client,
            url: url.to_string(),
        })
    }

    async fn get_full_transactions(
        &self,
        request: &HistoryRequest,
        start_slot: Option<u64>,
        end_slot: Option<u64>,
        pagination_token: Option<String>,
        metrics: &Arc<MetricsState>,
    ) -> Result<RawTransactionsResponse<RawFullEntry>> {
        let options = RawTransactionsOptions::full(
            request.page_limit,
            request.commitment.clone(),
            pagination_token,
            start_slot,
            end_slot,
        );
        self.post_get_transactions(&request.address, options, metrics)
            .await
    }

    async fn get_signatures(
        &self,
        request: &HistoryRequest,
        start_slot: Option<u64>,
        end_slot: Option<u64>,
        sort_order: &'static str,
        limit: u32,
        pagination_token: Option<String>,
        metrics: &Arc<MetricsState>,
    ) -> Result<RawTransactionsResponse<RawSignatureEntry>> {
        let options = RawTransactionsOptions::signatures(
            limit,
            request.commitment.clone(),
            pagination_token,
            start_slot,
            end_slot,
            sort_order,
        );
        self.post_get_transactions(&request.address, options, metrics)
            .await
    }

    async fn post_get_transactions<T>(
        &self,
        address: &str,
        options: RawTransactionsOptions,
        metrics: &Arc<MetricsState>,
    ) -> Result<RawTransactionsResponse<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        with_retry(metrics, || async {
            let body = RawRpcRequest {
                jsonrpc: "2.0",
                id: "mert-algo",
                method: "getTransactionsForAddress",
                params: (address, options.clone()),
            };

            let response = self
                .client
                .post(&self.url)
                .json(&body)
                .send()
                .await
                .map_err(|error| RpcAttemptError::Retryable(error.to_string()))?;

            let status = response.status();
            let text = response
                .text()
                .await
                .map_err(|error| RpcAttemptError::Retryable(redact_api_keys(&error.to_string())))?;

            if !status.is_success() {
                let message = redact_api_keys(&text);
                return if is_retryable_status(status) {
                    Err(RpcAttemptError::Retryable(message))
                } else {
                    Err(RpcAttemptError::Fatal(message))
                };
            }

            let envelope: RawRpcResponse<RawTransactionsResponse<T>> = serde_json::from_str(&text)
                .map_err(|error| {
                    RpcAttemptError::Fatal(format!("failed to decode Helius response: {error}"))
                })?;

            if let Some(error) = envelope.error {
                let message = redact_api_keys(&error.message);
                return if is_retryable_rpc_error(error.code, &message) {
                    Err(RpcAttemptError::Retryable(message))
                } else {
                    Err(RpcAttemptError::Fatal(message))
                };
            }

            envelope
                .result
                .ok_or_else(|| RpcAttemptError::Fatal("Helius response missing result".to_string()))
        })
        .await
    }
}

async fn fetch_full_range(
    client: &RawHeliusClient,
    request: &HistoryRequest,
    start_slot: Option<u64>,
    end_slot: Option<u64>,
    metrics: &Arc<MetricsState>,
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

        let result = client
            .get_full_transactions(request, start_slot, end_slot, pagination_token, metrics)
            .await?;
        metrics.full_pages.fetch_add(1, Ordering::Relaxed);

        for entry in result.data {
            if let Some(event) = decode_full_entry(entry, &request.address)? {
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

async fn fetch_ranges_parallel(
    client: &RawHeliusClient,
    request: &HistoryRequest,
    ranges: Vec<(u64, u64)>,
    metrics: &Arc<MetricsState>,
) -> Result<Vec<Vec<TransactionEvent>>> {
    let mut shards = stream::iter(ranges.into_iter().enumerate())
        .map(|(index, (start, end))| {
            let metrics = Arc::clone(metrics);
            async move {
                let events =
                    fetch_full_range(client, request, Some(start), Some(end), &metrics).await?;
                Ok::<_, anyhow::Error>((index, events))
            }
        })
        .buffer_unordered(request.concurrency)
        .try_collect::<Vec<_>>()
        .await?;

    shards.sort_by_key(|(index, _)| *index);
    let mut events = Vec::new();
    for (_, shard) in shards {
        events.push(shard);
    }
    Ok(events)
}

async fn discover_slot_bounds(
    client: &RawHeliusClient,
    request: &HistoryRequest,
    metrics: &Arc<MetricsState>,
) -> Result<Option<(u64, u64)>> {
    let first = fetch_signature_edge(client, request, "asc", metrics).await?;
    let last = fetch_signature_edge(client, request, "desc", metrics).await?;

    match (first, last) {
        (Some(first), Some(last)) => Ok(Some((first, last))),
        _ => Ok(None),
    }
}

async fn effective_slot_bounds(
    client: &RawHeliusClient,
    request: &HistoryRequest,
    metrics: &Arc<MetricsState>,
) -> Result<Option<(u64, u64)>> {
    if let (Some(start_slot), Some(end_slot)) = (request.start_slot, request.end_slot) {
        return Ok(Some((start_slot, end_slot)));
    }

    let Some((first_slot, last_slot)) = discover_slot_bounds(client, request, metrics).await?
    else {
        return Ok(None);
    };

    Ok(Some((
        request.start_slot.unwrap_or(first_slot),
        request.end_slot.unwrap_or(last_slot),
    )))
}

async fn fetch_signature_edge(
    client: &RawHeliusClient,
    request: &HistoryRequest,
    sort_order: &'static str,
    metrics: &Arc<MetricsState>,
) -> Result<Option<u64>> {
    let result = client
        .get_signatures(
            request,
            request.start_slot,
            request.end_slot,
            sort_order,
            1,
            None,
            metrics,
        )
        .await?;

    Ok(result
        .data
        .into_iter()
        .next()
        .map(|signature| signature.slot))
}

async fn fetch_signature_points(
    client: &RawHeliusClient,
    request: &HistoryRequest,
    start_slot: u64,
    end_slot: u64,
    metrics: &Arc<MetricsState>,
) -> Result<Vec<SignaturePoint>> {
    let mut pagination_token = None;
    let mut points = Vec::new();

    loop {
        let (mut page, next_token) = fetch_signature_page(
            client,
            request,
            start_slot,
            end_slot,
            pagination_token,
            metrics,
        )
        .await?;
        points.append(&mut page);
        pagination_token = next_token;
        if pagination_token.is_none() {
            break;
        }
    }

    sort_signatures(&mut points);
    Ok(points)
}

async fn fetch_signature_page(
    client: &RawHeliusClient,
    request: &HistoryRequest,
    start_slot: u64,
    end_slot: u64,
    pagination_token: Option<String>,
    metrics: &Arc<MetricsState>,
) -> Result<(Vec<SignaturePoint>, Option<String>)> {
    let result = client
        .get_signatures(
            request,
            Some(start_slot),
            Some(end_slot),
            "asc",
            SIGNATURE_PAGE_LIMIT,
            pagination_token,
            metrics,
        )
        .await?;
    metrics.signature_pages.fetch_add(1, Ordering::Relaxed);

    let points = result
        .data
        .into_iter()
        .map(|signature| SignaturePoint {
            signature: signature.signature,
            slot: signature.slot,
            transaction_index: signature.transaction_index,
        })
        .collect();

    Ok((points, result.pagination_token))
}

fn desired_partitions(request: &HistoryRequest) -> usize {
    request
        .partitions
        .unwrap_or_else(|| request.concurrency.saturating_mul(4).max(1))
        .max(1)
}

fn density_or_slot_ranges(
    signatures: &[SignaturePoint],
    start_slot: u64,
    end_slot: u64,
    partitions: usize,
) -> Vec<(u64, u64)> {
    if signatures.is_empty() {
        partition_slots(start_slot, end_slot, partitions)
    } else {
        partition_by_signature_density(signatures, partitions)
    }
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

fn page_density_ranges(signatures: &[SignaturePoint], target_per_range: usize) -> Vec<(u64, u64)> {
    if signatures.is_empty() {
        return Vec::new();
    }

    let target_per_range = target_per_range.max(1);
    let desired_partitions = signatures.len().div_ceil(target_per_range).max(1);
    partition_by_signature_density(signatures, desired_partitions)
}

fn partition_by_signature_density(
    signatures: &[SignaturePoint],
    desired_partitions: usize,
) -> Vec<(u64, u64)> {
    if signatures.is_empty() {
        return Vec::new();
    }

    let desired_partitions = desired_partitions.max(1).min(signatures.len());
    let target_per_partition = signatures.len().div_ceil(desired_partitions);
    let mut ranges = Vec::with_capacity(desired_partitions);
    let mut start_slot = signatures[0].slot;
    let mut current_count = 0usize;
    let mut previous_slot = signatures[0].slot;

    for point in signatures {
        if current_count >= target_per_partition
            && point.slot != previous_slot
            && ranges.len() + 1 < desired_partitions
        {
            ranges.push((start_slot, previous_slot));
            start_slot = point.slot;
            current_count = 0;
        }

        current_count += 1;
        previous_slot = point.slot;
    }

    ranges.push((start_slot, signatures.last().expect("nonempty").slot));
    ranges
}

fn sort_signatures(signatures: &mut [SignaturePoint]) {
    signatures.sort_by(|a, b| {
        (a.slot, a.transaction_index, a.signature.as_str()).cmp(&(
            b.slot,
            b.transaction_index,
            b.signature.as_str(),
        ))
    });
}

fn decode_full_entry(entry: RawFullEntry, address: &str) -> Result<Option<TransactionEvent>> {
    let meta = entry
        .meta
        .ok_or_else(|| anyhow!("transaction at slot {} has no metadata", entry.slot))?;
    let account_index = {
        let mut account_keys = entry.transaction.message.account_key_strings(&meta);
        let Some(index) = account_keys.position(|key| key == address) else {
            return Ok(None);
        };
        index
    };

    let pre_balance = *meta
        .pre_balances
        .get(account_index)
        .ok_or_else(|| anyhow!("missing pre balance for account index {account_index}"))?;
    let post_balance = *meta
        .post_balances
        .get(account_index)
        .ok_or_else(|| anyhow!("missing post balance for account index {account_index}"))?;

    let signature = entry
        .transaction
        .signatures
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("transaction at slot {} has no signature", entry.slot))?;
    let delta = i128::from(post_balance) - i128::from(pre_balance);
    let delta_lamports = i64::try_from(delta).context("lamport delta does not fit in i64")?;

    Ok(Some(TransactionEvent {
        signature,
        slot: entry.slot,
        transaction_index: entry.transaction_index,
        block_time: entry.block_time,
        err: meta.err.map(|err| format!("{err:?}")),
        account_index,
        fee_lamports: meta.fee,
        is_fee_payer: account_index == 0,
        pre_balance_lamports: pre_balance,
        post_balance_lamports: post_balance,
        delta_lamports,
    }))
}

async fn with_retry<T, Fut, F>(metrics: &Arc<MetricsState>, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<T, RpcAttemptError>>,
{
    let max_retries = 4;
    for attempt in 0..=max_retries {
        metrics.rpc_requests.fetch_add(1, Ordering::Relaxed);
        match operation().await {
            Ok(value) => return Ok(value),
            Err(RpcAttemptError::Retryable(message)) if attempt < max_retries => {
                tokio::time::sleep(Duration::from_millis(250 * 2_u64.pow(attempt))).await;
                let _ = message;
            }
            Err(error) => return Err(anyhow!(redact_api_keys(&error.to_string()))),
        }
    }

    unreachable!("retry loop always returns")
}

fn run_metrics(
    strategy: &str,
    started_at: Instant,
    request: &HistoryRequest,
    metrics: &Arc<MetricsState>,
    decoded_events: usize,
    partitions: usize,
) -> RunMetrics {
    RunMetrics {
        strategy: strategy.to_string(),
        elapsed_ms: started_at.elapsed().as_millis(),
        rpc_requests: metrics.rpc_requests.load(Ordering::Relaxed),
        full_pages: metrics.full_pages.load(Ordering::Relaxed),
        signature_pages: metrics.signature_pages.load(Ordering::Relaxed),
        decoded_events,
        partitions,
        page_limit: request.page_limit,
        concurrency: request.concurrency,
    }
}

fn empty_run(
    strategy: &str,
    started_at: Instant,
    request: &HistoryRequest,
    metrics: &Arc<MetricsState>,
) -> HistoryRun {
    HistoryRun {
        metrics: run_metrics(strategy, started_at, request, metrics, 0, 0),
        events: Vec::new(),
    }
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

fn is_retryable_rpc_error(code: i64, message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    code == -32005
        || code == -32603
        || message.contains("rate")
        || message.contains("timeout")
        || message.contains("temporar")
}

fn redact_api_keys(message: &str) -> String {
    let Some(start) = message.find("api-key=") else {
        return message.to_string();
    };

    let value_start = start + "api-key=".len();
    let value_end = message[value_start..]
        .find(['&', ')', ' ', '"'])
        .map(|offset| value_start + offset)
        .unwrap_or(message.len());

    let mut redacted = String::with_capacity(message.len());
    redacted.push_str(&message[..value_start]);
    redacted.push_str("<redacted>");
    redacted.push_str(&message[value_end..]);
    redacted
}

#[derive(Debug)]
enum RpcAttemptError {
    Retryable(String),
    Fatal(String),
}

impl std::fmt::Display for RpcAttemptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retryable(message) | Self::Fatal(message) => formatter.write_str(message),
        }
    }
}

#[derive(Serialize, Clone)]
struct RawRpcRequest<'a> {
    jsonrpc: &'static str,
    id: &'static str,
    method: &'static str,
    params: (&'a str, RawTransactionsOptions),
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RawTransactionsOptions {
    transaction_details: &'static str,
    sort_order: &'static str,
    limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pagination_token: Option<String>,
    commitment: String,
    filters: RawTransactionsFilters,
    #[serde(skip_serializing_if = "Option::is_none")]
    encoding: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_supported_transaction_version: Option<u8>,
}

impl RawTransactionsOptions {
    fn full(
        limit: u32,
        commitment: String,
        pagination_token: Option<String>,
        start_slot: Option<u64>,
        end_slot: Option<u64>,
    ) -> Self {
        Self {
            transaction_details: "full",
            sort_order: "asc",
            limit,
            pagination_token,
            commitment,
            filters: RawTransactionsFilters::new(start_slot, end_slot),
            encoding: Some("json"),
            max_supported_transaction_version: Some(0),
        }
    }

    fn signatures(
        limit: u32,
        commitment: String,
        pagination_token: Option<String>,
        start_slot: Option<u64>,
        end_slot: Option<u64>,
        sort_order: &'static str,
    ) -> Self {
        Self {
            transaction_details: "signatures",
            sort_order,
            limit,
            pagination_token,
            commitment,
            filters: RawTransactionsFilters::new(start_slot, end_slot),
            encoding: None,
            max_supported_transaction_version: None,
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RawTransactionsFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<RawSlotFilter>,
    status: &'static str,
}

impl RawTransactionsFilters {
    fn new(start_slot: Option<u64>, end_slot: Option<u64>) -> Self {
        Self {
            slot: if start_slot.is_some() || end_slot.is_some() {
                Some(RawSlotFilter {
                    gte: start_slot,
                    lte: end_slot,
                })
            } else {
                None
            },
            status: "any",
        }
    }
}

#[derive(Serialize, Clone)]
struct RawSlotFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    gte: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lte: Option<u64>,
}

#[derive(Deserialize)]
struct RawRpcResponse<T> {
    result: Option<T>,
    error: Option<RawRpcError>,
}

#[derive(Deserialize)]
struct RawRpcError {
    code: i64,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTransactionsResponse<T> {
    data: Vec<T>,
    pagination_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSignatureEntry {
    signature: String,
    slot: u64,
    transaction_index: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFullEntry {
    slot: u64,
    transaction_index: u64,
    transaction: RawTransaction,
    meta: Option<RawMeta>,
    block_time: Option<i64>,
}

#[derive(Deserialize)]
struct RawTransaction {
    signatures: Vec<String>,
    message: RawMessage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    account_keys: Vec<RawAccountKey>,
}

impl RawMessage {
    fn account_key_strings<'a>(&'a self, meta: &'a RawMeta) -> impl Iterator<Item = &'a str> {
        let keys = self.account_keys.iter().map(RawAccountKey::as_str);

        let loaded_writable = meta
            .loaded_addresses
            .as_ref()
            .map(|loaded| loaded.writable.iter().map(String::as_str))
            .into_iter()
            .flatten();

        let loaded_readonly = meta
            .loaded_addresses
            .as_ref()
            .map(|loaded| loaded.readonly.iter().map(String::as_str))
            .into_iter()
            .flatten();

        keys.chain(loaded_writable).chain(loaded_readonly)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawAccountKey {
    String(String),
    Object { pubkey: String },
}

impl RawAccountKey {
    fn as_str(&self) -> &str {
        match self {
            Self::String(key) => key.as_str(),
            Self::Object { pubkey } => pubkey.as_str(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMeta {
    err: Option<Value>,
    fee: u64,
    pre_balances: Vec<u64>,
    post_balances: Vec<u64>,
    #[serde(default)]
    loaded_addresses: Option<RawLoadedAddresses>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLoadedAddresses {
    #[serde(default)]
    writable: Vec<String>,
    #[serde(default)]
    readonly: Vec<String>,
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
    fn partitions_by_signature_density_without_splitting_slots() {
        let signatures = vec![
            sig("a", 10, 0),
            sig("b", 10, 1),
            sig("c", 11, 0),
            sig("d", 12, 0),
            sig("e", 13, 0),
            sig("f", 13, 1),
        ];

        assert_eq!(
            partition_by_signature_density(&signatures, 3),
            vec![(10, 10), (11, 12), (13, 13)]
        );
    }

    #[test]
    fn page_density_ranges_use_requested_full_page_size() {
        let signatures = vec![
            sig("a", 10, 0),
            sig("b", 11, 0),
            sig("c", 12, 0),
            sig("d", 13, 0),
            sig("e", 14, 0),
        ];

        assert_eq!(
            page_density_ranges(&signatures, 2),
            vec![(10, 11), (12, 13), (14, 14)]
        );
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
        let tx: RawFullEntry = serde_json::from_value(raw).expect("fixture should deserialize");

        let event = decode_full_entry(tx, "target")
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

    fn sig(signature: &str, slot: u64, transaction_index: u64) -> SignaturePoint {
        SignaturePoint {
            signature: signature.to_string(),
            slot,
            transaction_index,
        }
    }
}
