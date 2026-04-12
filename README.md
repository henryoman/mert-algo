# SOL Balance History Reconstruction

Reconstruct a Solana address's native SOL balance history and SOL-denominated wallet change using Helius `getTransactionsForAddress`.

The implementation uses live RPC only. It does not rely on a prebuilt index, warehouse, token pricing service, or fiat conversion layer.

## What This Computes

For every full transaction returned by Helius, the decoder finds the target address in the transaction account list and computes:

```text
delta_lamports = postBalances[i] - preBalances[i]
balance_lamports = postBalances[i]
```

`balance_lamports` is taken directly from transaction metadata instead of being reconstructed from zero. The cumulative change is still summarized, but the point-in-time balance source of truth is the post-transaction balance.

The JSON summary includes `pnl_lamports`. Its policy is:

```text
native_sol_balance_delta_only_no_external_flow_classification
```

That means it is exact native SOL balance change over the fetched range. It is not full economic portfolio PnL adjusted for deposits, withdrawals, token marks, or cost basis.

## Setup

Put the Helius key in `.env`:

```text
HELIUS_API_KEY=...
```

`.env` and `target/` are ignored by git.

## Commands

Run the full chronological mode:

```bash
cargo run -- \
  --address <PUBKEY> \
  --mode simple \
  --format json
```

Run a bounded smoke test:

```bash
cargo run -- \
  --address TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \
  --mode simple \
  --max-pages 1 \
  --page-limit 1 \
  --format json
```

Run optimized slot-partitioned mode over a known slot range:

```bash
cargo run -- \
  --address <PUBKEY> \
  --mode optimized \
  --start-slot <START> \
  --end-slot <END> \
  --partitions 16 \
  --concurrency 8 \
  --format json
```

CSV output writes balance rows only:

```bash
cargo run -- \
  --address <PUBKEY> \
  --mode simple \
  --format csv
```

## Modes

- `simple`: sequential `getTransactionsForAddress` full-mode scan with chronological pagination.
- `optimized`: discovers slot bounds with signature-mode calls, partitions the slot range, fetches full transactions in parallel, then sorts and dedupes locally.

## Correctness Rules

- Use `transactionDetails: Full` for exact balance deltas.
- Use `sortOrder: Asc` for chronological reads.
- Preserve `paginationToken` exactly.
- Include failed transactions because failed transactions can still charge fees.
- Use `postBalances[i] - preBalances[i]`; do not separately add fees or rewards into the native delta.
- Sort locally by `(slot, transactionIndex, signature)`.
- Dedupe by signature after merging shards.
- Include loaded addresses from versioned transactions when matching account indexes.

## Verification

```bash
cargo test
cargo check
```

Current local verification:

```text
cargo test: 5 passed
cargo check: passed
simple live smoke: passed
optimized single-slot live smoke: passed
csv live smoke: passed
```
