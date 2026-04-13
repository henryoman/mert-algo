# AGENTS.md

## Project Mission

This repo is for the Mert / Helius challenge:

Build the fastest possible runtime SOL-denominated PnL / balance-change tool for a Solana address using Helius `getTransactionsForAddress` as the historical data source.

The important premise from Mert's public post is:

- no prebuilt index
- no warehouse
- no cached address history
- scan address history at runtime
- split by time horizons / ranges
- fetch ranges in parallel
- return results as fast as possible

Treat this as a latency challenge first and an accounting challenge second. Correctness is still mandatory, but do not drift into building a general portfolio analytics product.

## Hard Rules

- Use Helius `getTransactionsForAddress` for historical transaction retrieval.
- Do not replace the core path with `getSignaturesForAddress`, `getTransaction`, `getBlock`, a database, BigQuery, DAS search, webhooks, or an external indexer.
- Do not build or depend on a persistent indexing layer.
- Do not cache historical address results as part of normal correctness.
- Do not call external pricing APIs unless the user explicitly changes the challenge scope.
- Load the API key from `.env` as `HELIUS_API_KEY`; never print or commit it.
- Redact `api-key=` from any surfaced errors.
- Keep `.env` and `target/` ignored.

## What PnL Means Here

The current supported PnL is native SOL balance-change PnL over the fetched range:

```text
delta_lamports = postBalances[i] - preBalances[i]
balance_lamports = postBalances[i]
pnl_lamports = end_balance_lamports - start_balance_lamports
```

This is exact native SOL wallet balance change in lamports.

Do not claim this is fully external-flow-adjusted economic portfolio PnL unless flow classification is implemented. Full economic PnL would require:

```text
PnL = E_T - E_0 - external_deposits + external_withdrawals
```

That is a future layer. The current policy must remain explicit in output:

```text
native_sol_balance_delta_only_no_external_flow_classification
```

## Correctness Rules

- The source of truth is raw transaction metadata:

```text
postBalances[i] - preBalances[i]
```

- `balance_lamports` must come from `postBalances[i]`, not from cumulative sum starting at zero.
- Include failed transactions by default because failed transactions can still charge fees.
- Do not separately add `fee`, `rewards`, `nativeBalanceChange`, or transfer fields into the delta. That double-counts unless producing a decomposition.
- Find the target address in the transaction account key list and use that index for `preBalances` / `postBalances`.
- Include loaded addresses from versioned transactions when resolving account indexes.
- Preserve Helius `paginationToken` exactly.
- Sort final records by:

```text
(slot, transactionIndex, signature)
```

- Dedupe final records by signature after merging shards.
- Keep all arithmetic in lamports / integers. Convert to SOL only as display sugar.

## Helius API Constraints

`getTransactionsForAddress` supports:

- `transactionDetails: "full"` or `"signatures"`
- `sortOrder: "asc"` or `"desc"`
- `paginationToken`
- slot filters
- block-time filters
- status filters
- signature filters
- `maxSupportedTransactionVersion`

Known page limits:

- full transaction pages: max 100
- signature pages: max 1000

Default commitment should be `finalized`. `confirmed` may be exposed for speed experiments. Do not use `processed` for this method.

## Performance Strategy

The production targets are `optimized` and `adaptive`, not `simple`.

Use this hierarchy:

1. Use `signatures` mode to discover slot/time bounds cheaply.
2. Split requested time horizons or slot ranges into independent partitions.
3. Fetch each partition in `full` mode concurrently.
4. Decode balance deltas as responses arrive.
5. Merge, sort, and dedupe deterministically.
6. Emit summary, benchmark metrics, and balance history.

The sequential `simple` mode exists only for sanity checks and small ranges.

Avoid designs that require full-history serial pagination for deep wallets. If a user asks for fastest, work on range partitioning, concurrency, and payload reduction before adding unrelated features.

## CLI Expectations

The CLI should work without passing `--api-key` when `.env` contains:

```text
HELIUS_API_KEY=...
```

Important flags:

```bash
cargo run -- \
  --address <PUBKEY> \
  --mode optimized \
  --start-slot <START> \
  --end-slot <END> \
  --partitions 32 \
  --concurrency 8 \
  --format json
```

For smoke tests:

```bash
cargo run -- \
  --address TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \
  --mode simple \
  --max-pages 1 \
  --page-limit 1 \
  --format json
```

## Code Layout

- `src/main.rs`: CLI entrypoint.
- `src/config.rs`: CLI parsing and `.env` / `HELIUS_API_KEY` request construction.
- `src/helius_simple.rs`: Helius retrieval, decoding, retry, simple mode, equal-slot optimized mode, adaptive signature-density mode.
- `src/reconstruct.rs`: sorting, dedupe, summary, report construction.
- `src/output.rs`: JSON and CSV writers.
- `src/types.rs`: serialized event, balance point, and summary types.

If the retrieval module grows, split it into:

- `src/helius/client.rs`
- `src/helius/decode.rs`
- `src/helius/partition.rs`
- `src/helius/retry.rs`

Do that only when the current file becomes hard to work in.

## Testing

Before reporting completion, run:

```bash
cargo fmt
cargo test
cargo check
```

When network is available and `.env` is present, also run one bounded live smoke test:

```bash
cargo run -- \
  --address TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \
  --mode simple \
  --max-pages 1 \
  --page-limit 1 \
  --format json > /tmp/sol-balance-history-smoke.json
```

For optimized mode, use a bounded slot range first. Do not accidentally run an unbounded high-concurrency full-history fetch while testing.

## Current Gaps To Prioritize

These are the next important improvements:

1. Add time-horizon flags such as `--horizons 1h,24h,7d,30d,all`.
2. Convert horizons to slot or block-time filters and run them concurrently.
3. Add per-partition request metrics and slowest-partition reporting.
4. Add optional concise output for challenge submissions.
5. Add more benchmark targets, especially very deep real wallets and uneven high-activity accounts.
6. If full token PnL becomes required, design it separately and keep the native SOL path untouched.

## Style

- Keep code direct and boring.
- Avoid abstractions that do not improve latency, correctness, or benchmark clarity.
- Prefer typed SDK structs over ad hoc JSON when the SDK exposes the fields needed.
- Keep user-facing wording precise about what is and is not computed.
- Never expose secrets in logs, errors, tests, docs, or examples.
