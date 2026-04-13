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

Run adaptive density-partitioned mode over a known slot range:

```bash
cargo run --release -- \
  --address <PUBKEY> \
  --mode adaptive \
  --start-slot <START> \
  --end-slot <END> \
  --partitions 32 \
  --concurrency 16 \
  --format json
```

CSV output writes balance rows only:

```bash
cargo run -- \
  --address <PUBKEY> \
  --mode simple \
  --format csv
```

Run the checked-in benchmark matrix:

```bash
bash scripts/benchmark_matrix.sh
```

The benchmark script writes per-run JSON and `summary.csv` under `benchmarks/<timestamp>/`.
It requires `jq` for summarizing the JSON output.

## Modes

- `simple`: sequential `getTransactionsForAddress` full-mode scan with chronological pagination.
- `optimized`: discovers slot bounds with signature-mode calls, splits the slot span into equal slot ranges, fetches full transactions in parallel, then sorts and dedupes locally.
- `adaptive`: discovers signatures in the range, partitions by transaction density without splitting slots, fetches full transactions in parallel, then sorts and dedupes locally.

## Correctness Rules

- Use `transactionDetails: Full` for exact balance deltas.
- Use `sortOrder: Asc` for chronological reads.
- Preserve `paginationToken` exactly.
- Include failed transactions because failed transactions can still charge fees.
- Use `postBalances[i] - preBalances[i]`; do not separately add fees or rewards into the native delta.
- Sort locally by `(slot, transactionIndex, signature)`.
- Dedupe by signature after merging shards.
- Include loaded addresses from versioned transactions when matching account indexes.

## Benchmarks

Benchmarks were run with the release binary on April 12, 2026 PDT / April 13, 2026 UTC, using `HELIUS_API_KEY` from `.env`.

Method:

- Each target first used `simple --max-pages 1 --page-limit 100` to discover a bounded chronological slot range.
- Every variant then ran against the same target and same slot range.
- Matching `checksum` means the variants returned the same ordered balance history for that range.
- `rpc_requests` includes retry attempts. Bounded optimized/adaptive runs skip signature-bound discovery when both slot bounds are supplied.
- `full_pages` counts full `transactionDetails=full` pages.

Public benchmark targets:

| Label | Address | Purpose |
| --- | --- | --- |
| `walletmaster_sample` | `7x6qE3DRMW2ZCgT1YQuBLePiheEWw7qjH6rYjj6GDtEd` | wallet-like public sample with sparse-ish history |
| `spl_token_program` | `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA` | dense, high-reference public account |
| `wrapped_sol_mint` | `So11111111111111111111111111111111111111112` | mint account sanity target |

### First-page windows

These ranges are small. They mainly test overhead, page size, and whether partitioning returns identical data.

| Target | Variant | Slots | ms | RPC | Full pages | Rows | Partitions | Checksum |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| walletmaster_sample | simple-p100 | 383732198-384010213 | 844 | 2 | 2 | 100 | 1 | 368748239513577252 |
| walletmaster_sample | simple-p25 | 383732198-384010213 | 1037 | 5 | 5 | 100 | 1 | 368748239513577252 |
| walletmaster_sample | opt-p2-c2 | 383732198-384010213 | 815 | 6 | 4 | 100 | 2 | 368748239513577252 |
| walletmaster_sample | opt-p4-c4 | 383732198-384010213 | 863 | 10 | 8 | 100 | 4 | 368748239513577252 |
| walletmaster_sample | opt-p8-c8 | 383732198-384010213 | 816 | 17 | 15 | 100 | 8 | 368748239513577252 |
| spl_token_program | simple-p100 | 31303514-31303556 | 783 | 3 | 3 | 135 | 1 | 14826887099948873057 |
| spl_token_program | simple-p25 | 31303514-31303556 | 1041 | 7 | 7 | 135 | 1 | 14826887099948873057 |
| spl_token_program | opt-p2-c2 | 31303514-31303556 | 1036 | 7 | 5 | 135 | 2 | 14826887099948873057 |
| spl_token_program | opt-p4-c4 | 31303514-31303556 | 1059 | 9 | 7 | 135 | 4 | 14826887099948873057 |
| spl_token_program | opt-p8-c8 | 31303514-31303556 | 887 | 13 | 11 | 135 | 8 | 14826887099948873057 |
| wrapped_sol_mint | simple-p100 | 31340476-31906671 | 690 | 3 | 3 | 101 | 1 | 15039433388617072272 |
| wrapped_sol_mint | simple-p25 | 31340476-31906671 | 962 | 6 | 6 | 101 | 1 | 15039433388617072272 |
| wrapped_sol_mint | opt-p2-c2 | 31340476-31906671 | 861 | 6 | 4 | 101 | 2 | 15039433388617072272 |
| wrapped_sol_mint | opt-p4-c4 | 31340476-31906671 | 1037 | 9 | 7 | 101 | 4 | 15039433388617072272 |
| wrapped_sol_mint | opt-p8-c8 | 31340476-31906671 | 787 | 15 | 13 | 101 | 8 | 15039433388617072272 |

Takeaways:

- `page-limit 100` beats `page-limit 25` on all first-page windows because it uses fewer full pages.
- Partitioning is not automatically faster on tiny ranges because discovery and extra range requests add overhead.
- Checksums matched within each target, so the merge/dedupe path preserved correctness.

### Larger 500-row windows

These ranges are more representative of the challenge because serial pagination starts to matter.

| Target | Variant | Slots | ms | RPC | Full pages | Sig pages | Rows | Partitions | Checksum |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| walletmaster_sample | simple-p100 | 383732198-385119911 | 2790 | 7 | 7 | 0 | 501 | 1 | 14648520737400887876 |
| walletmaster_sample | opt-p8-c8 | 383732198-385119911 | 795 | 17 | 17 | 0 | 501 | 8 | 14648520737400887876 |
| walletmaster_sample | opt-p16-c8 | 383732198-385119911 | 1121 | 33 | 33 | 0 | 501 | 16 | 14648520737400887876 |
| walletmaster_sample | opt-p32-c16 | 383732198-385119911 | 910 | 57 | 57 | 0 | 501 | 32 | 14648520737400887876 |
| walletmaster_sample | adaptive-p8-c8 | 383732198-385119911 | 1170 | 18 | 16 | 2 | 501 | 8 | 14648520737400887876 |
| walletmaster_sample | adaptive-p16-c8 | 383732198-385119911 | 1410 | 34 | 32 | 2 | 501 | 16 | 14648520737400887876 |
| walletmaster_sample | adaptive-p32-c16 | 383732198-385119911 | 1382 | 62 | 60 | 2 | 501 | 30 | 14648520737400887876 |
| spl_token_program | simple-p100 | 31303514-31303565 | 1512 | 7 | 7 | 0 | 540 | 1 | 10651770158733798012 |
| spl_token_program | opt-p8-c8 | 31303514-31303565 | 1041 | 16 | 16 | 0 | 540 | 8 | 10651770158733798012 |
| spl_token_program | opt-p16-c8 | 31303514-31303565 | 789 | 20 | 20 | 0 | 540 | 13 | 10651770158733798012 |
| spl_token_program | opt-p32-c16 | 31303514-31303565 | 847 | 35 | 35 | 0 | 540 | 26 | 10651770158733798012 |
| spl_token_program | adaptive-p8-c8 | 31303514-31303565 | 915 | 14 | 12 | 2 | 540 | 5 | 10651770158733798012 |
| spl_token_program | adaptive-p16-c8 | 31303514-31303565 | 1128 | 22 | 20 | 2 | 540 | 10 | 10651770158733798012 |
| spl_token_program | adaptive-p32-c16 | 31303514-31303565 | 920 | 24 | 22 | 2 | 540 | 11 | 10651770158733798012 |

Takeaways:

- On the 501-row wallet sample, `opt-p8-c8` was fastest and about 3.5x faster than `simple-p100`.
- On the dense 540-row SPL Token range, `opt-p16-c8` was fastest, with `opt-p32-c16` second.
- Adaptive partitioning adds signature-discovery pages, but it can reduce full pages by splitting dense and sparse regions more evenly.
- Over-partitioning can still hurt: `adaptive-p16-c8` was slower than `adaptive-p8-c8` on the wallet sample.

### 2,000-row windows

These are the most useful results so far for the contest shape.

| Target | Variant | Slots | ms | RPC | Full pages | Sig pages | Rows | Partitions | Checksum |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| walletmaster_sample | simple-p100 | 383732198-390238037 | 9387 | 21 | 21 | 0 | 2000 | 1 | 16500805959713175146 |
| walletmaster_sample | opt-p8-c8 | 383732198-390238037 | 3193 | 32 | 32 | 0 | 2000 | 8 | 16500805959713175146 |
| walletmaster_sample | opt-p16-c8 | 383732198-390238037 | 1973 | 42 | 42 | 0 | 2000 | 16 | 16500805959713175146 |
| walletmaster_sample | opt-p32-c16 | 383732198-390238037 | 1468 | 67 | 67 | 0 | 2000 | 32 | 16500805959713175146 |
| walletmaster_sample | adaptive-p8-c8 | 383732198-390238037 | 2031 | 35 | 32 | 3 | 2000 | 8 | 16500805959713175146 |
| walletmaster_sample | adaptive-p16-c8 | 383732198-390238037 | 2336 | 51 | 48 | 3 | 2000 | 16 | 16500805959713175146 |
| walletmaster_sample | adaptive-p32-c16 | 383732198-390238037 | 2050 | 67 | 64 | 3 | 2000 | 32 | 16500805959713175146 |
| spl_token_program | simple-p100 | 31303514-31372121 | 4009 | 22 | 22 | 0 | 2002 | 1 | 16639145197458147058 |
| spl_token_program | opt-p8-c8 | 31303514-31372121 | 2420 | 34 | 34 | 0 | 2002 | 8 | 16639145197458147058 |
| spl_token_program | opt-p16-c8 | 31303514-31372121 | 2494 | 47 | 47 | 0 | 2002 | 16 | 16639145197458147058 |
| spl_token_program | opt-p32-c16 | 31303514-31372121 | 2467 | 72 | 72 | 0 | 2002 | 32 | 16639145197458147058 |
| spl_token_program | adaptive-p8-c8 | 31303514-31372121 | 1906 | 35 | 31 | 4 | 2002 | 8 | 16639145197458147058 |
| spl_token_program | adaptive-p16-c8 | 31303514-31372121 | 2198 | 51 | 47 | 4 | 2002 | 16 | 16639145197458147058 |
| spl_token_program | adaptive-p32-c16 | 31303514-31372121 | 1576 | 61 | 57 | 4 | 2002 | 26 | 16639145197458147058 |

2,000-row takeaways:

- `walletmaster_sample`: fastest was equal-slot `opt-p32-c16` at 1468 ms, about 6.4x faster than serial.
- `spl_token_program`: fastest was `adaptive-p32-c16` at 1576 ms, about 2.5x faster than serial.
- Adaptive reduced full pages on both 32-partition tests, but signature discovery adds overhead.
- Equal-slot can win on sparse-ish ranges if high concurrency hides empty/uneven partitions.
- Adaptive appears stronger on dense or uneven transaction distributions.

Current best rule of thumb:

- Use `simple` for very small ranges.
- For medium ranges, test both `optimized --partitions 16 --concurrency 8` and `adaptive --partitions 8 --concurrency 8`; latest results show this can flip by target.
- Use `optimized --partitions 32 --concurrency 16` as an aggressive try for larger sparse-ish ranges.
- Use `adaptive --partitions 32 --concurrency 16` as an aggressive try for larger dense ranges.
- Keep `page-limit 100` for full-mode scans.

## Verification

```bash
cargo test
cargo check
```

Current local verification:

```text
cargo test: 6 passed
cargo check: passed
CLI help: passed
benchmark script syntax: passed
live benchmark matrix: passed, 28 runs
simple live smoke: passed
optimized single-slot live smoke: passed
csv live smoke: passed
```
