# Mini Solana Dev Weekend Competition Rules

Build the lowest-latency runtime algorithm for computing native SOL balance history for a Solana address.

Prize: $1,500 and bragging rights.

AI use is encouraged.

## Challenge

Given a Solana address, compute its native SOL balance over time as fast as possible using RPC at runtime.

For this competition, "SOL PnL" means native SOL balance over time. It does not need USD, USDC, token marks, cost basis, or external-flow-adjusted economic PnL.

The core problem is:

```text
How do you search the set of transactions for an address most efficiently when you do not know how sparse or dense that address history is?
```

Existing Solana RPC methods such as `getSignaturesForAddress` and `getTransaction` force a mostly one-directional recent-to-old traversal, which caps the algorithm space.

The new Helius `getTransactionsForAddress` RPC method changes the problem because you can query from the start, end, middle, or a bounded range, then parallelize independent calls. Good algorithms should use that extra freedom to reduce latency.

## Competition Window

The competition runs for 2 days from the announcement.

At the end of the 2-day window, submitted algorithms will be run against the same hidden benchmark set.

## How To Submit

Submit a gist, repository link, or code sample by either:

- replying to the announcement tweet
- sending it by DM

Submissions should include:

- code for the algorithm
- exact command or function call needed to run it
- required runtime dependencies
- any concurrency or page-size knobs
- a short note explaining the search strategy

Do not include API keys, private endpoints, secrets, or cached benchmark outputs.

## Allowed Data Source

The algorithm must compute results at runtime using RPC.

Allowed historical retrieval source:

- Helius `getTransactionsForAddress`

You may discuss or include `getSignaturesForAddress` / `getTransaction` as a baseline, but the competitive path should use `getTransactionsForAddress` as the primary historical retrieval method.

## Disallowed Approaches

Submissions must not use:

- prebuilt indexes
- databases or warehouses
- cached address histories
- BigQuery or third-party chain archives
- DAS search
- webhooks
- external indexers
- offline snapshots of the benchmark addresses
- external pricing APIs
- manual precomputation for the benchmark set

The algorithm must work from a cold start for each address.

## Correctness Target

For each relevant transaction, native SOL balance change must come from raw transaction metadata:

```text
delta_lamports = postBalances[i] - preBalances[i]
balance_lamports = postBalances[i]
```

Where `i` is the target address account index in the transaction account key list.

The final output must be a deterministic chronological balance history or an equivalent enough representation to verify it.

Valid output should include, or be convertible to:

- `slot`
- `transactionIndex` when available
- `signature`
- `balance_lamports` or `delta_lamports`

All arithmetic should be done in lamports. SOL display formatting is fine, but integer lamports are the source of truth.

## Required Behavior

Algorithms should:

- include failed transactions by default, because failed transactions can still charge fees
- preserve Helius `paginationToken` exactly
- handle versioned transactions and loaded addresses when resolving account indexes
- dedupe merged ranges by signature
- sort final records by `(slot, transactionIndex, signature)`
- avoid double-counting fees, rewards, transfers, or parsed balance-change fields

Do not separately add `fee`, `rewards`, `nativeBalanceChange`, or transfer fields into the balance delta unless you are only producing a diagnostic decomposition.

## Performance Goal

Lowest average wall-clock latency wins.

The benchmark set will include addresses with different history shapes:

- busy addresses
- sparse addresses
- periodic addresses

The judging focus is the algorithm's ability to adapt to unknown transaction density and minimize total runtime.

Useful strategies may include:

- discovering bounds cheaply
- splitting slot or time ranges
- adaptive partitioning based on observed density
- fetching independent ranges concurrently
- reducing unnecessary full transaction payloads
- merging and deduping deterministically
- reporting partial metrics for slow partitions

## Judging

Submissions will be run against the same set of benchmark addresses and ranges.

Winner selection:

```text
lowest average latency across the benchmark set
```

Correctness is required before latency counts. An algorithm that is faster but drops transactions, duplicates transactions, misorders results, or computes balances from the wrong account index is not eligible to win.

Judging may also inspect:

- total RPC request count
- full transaction pages fetched
- signature pages fetched
- retry behavior
- decode time
- merge time
- slowest partition latency
- determinism across repeated runs

## API Key And Secrets

Use an environment variable or local `.env` file for API keys.

Do not print, commit, paste, or submit secrets.

If your code surfaces request errors, redact `api-key=` values from logs and error messages.

## Notes

This is a latency challenge first and an accounting challenge second.

The goal is not to build a general portfolio analytics product. The goal is to find the fastest runtime search and reconstruction algorithm for native SOL balance history when no index exists and address density is unknown.

