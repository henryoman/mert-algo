# Project Treatment: Runtime SOL Balance History Reconstruction via RPC

## Objective

Build a low-latency system that reconstructs an address’s **native SOL balance as a time series** using **live RPC queries only**, with **no prebuilt indexing layer**.

Formally, the system must compute an ordered sequence:

\[
\{(t_k, s_k, B_k)\}_{k=1}^{n}
\]

where:

- \(t_k\) = block time
- \(s_k\) = slot / transaction position
- \(B_k\) = wallet balance in lamports after transaction \(k\)

The core optimization target is:

\[
\min T_{\text{total}} = T_{\text{rpc}} + T_{\text{decode}} + T_{\text{merge}} + T_{\text{reconstruct}}
\]

subject to exact balance-history correctness.

This treatment assumes the system uses Helius `getTransactionsForAddress`, which is a Helius-exclusive RPC method with advanced filtering, bidirectional sorting, and efficient pagination. It is not part of standard Solana RPC.

## Functional Requirements

### Required output

The system must return a chronologically ordered SOL balance history for a target address. The minimal valid output schema is:

- `blockTime`
- `slot`
- `transactionIndex`
- `signature`
- `balanceLamports`

An equivalent delta-based form is also acceptable:

- `blockTime`
- `slot`
- `transactionIndex`
- `signature`
- `deltaLamports`

with cumulative reconstruction applied downstream.

### Required inputs

- target wallet address
- RPC endpoint
- API key
- optional temporal filters
- optional slot filters
- optional success/failure filtering
- optional page size / concurrency settings

### Required correctness properties

The system must:

- preserve chronological order
- avoid omission across pagination boundaries
- avoid duplication across range partitions
- correctly interpret `preBalances` and `postBalances`
- handle failed vs succeeded transactions consistently
- support versioned transactions where needed

Helius documents that `getTransactionsForAddress` supports chronological and reverse sorting, status filters, slot filters, block-time filters, signature filters, pagination tokens, and version handling.

## Non-Functional Requirements

### Latency

Primary KPI: wall-clock time to produce the final ordered balance history.

Secondary KPIs:

- number of RPC round trips
- bytes transferred
- decode time
- merge time
- peak memory
- correctness under long histories

### Scalability

The design must remain viable for:

- shallow histories
- deep histories
- bursty addresses
- addresses with sparse history
- addresses with versioned transactions

### Observability

The system should expose:

- per-request latency
- request error rate
- pagination depth
- transactions scanned
- transactions decoded
- merge duration
- reconstruction duration
- total runtime

## RPC Surface

### Baseline Solana RPC path

Standard Solana RPC provides:

- `getSignaturesForAddress`
- `getTransaction`

`getSignaturesForAddress` returns signatures ordered from **newest to oldest**, which constrains traversal to a reverse crawl. `getTransaction` returns full transaction data by signature, including `blockTime` and `meta`.

This baseline path is structurally inefficient for this project because it forces:

1. signature discovery
2. follow-up transaction fetches
3. reverse traversal
4. high RPC count

### Helius RPC path

`getTransactionsForAddress` materially expands the search space because it supports:

- `transactionDetails = "signatures"` or `"full"`
- `sortOrder = "asc"` or `"desc"`
- `limit`
- `paginationToken`
- `filters.slot`
- `filters.blockTime`
- `filters.signature`
- `filters.status`
- `filters.tokenAccounts`
- `maxSupportedTransactionVersion`

Helius documents that `full` returns complete transaction data in one call, while `signatures` is faster and supports larger pages. Limits are up to **1000** for `signatures` and up to **100** for `full`.

## Project Scope

### In scope

- runtime historical retrieval
- chronological balance reconstruction
- full-history and bounded-range queries
- parallel range scanning
- deterministic merge logic
- benchmarking and correctness verification

### Out of scope

- pre-indexing
- warehouse-backed analytics
- token PnL
- fiat conversion
- unrealized mark-to-market logic
- UI-heavy analytics product features

## Core Technical Idea

This is not a pricing problem. It is a **history retrieval and state reconstruction problem**.

For each relevant transaction \(k\), if the target account is present at account index \(i\), native SOL delta is:

\[
\Delta_k = \text{postBalances}_k[i] - \text{preBalances}_k[i]
\]

Then cumulative balance is:

\[
B_k = B_{k-1} + \Delta_k
\]

Helius’s example response format shows `meta.preBalances` and `meta.postBalances` in full transaction responses. Standard Solana `getTransaction` also returns `meta`.

## Candidate Architectures

### Architecture A: Baseline reverse crawl

#### Flow

1. call `getSignaturesForAddress`
2. page newest to oldest
3. call `getTransaction` per signature
4. compute delta
5. reverse into chronological order
6. accumulate balances

#### Advantages

- portable across standard Solana RPC
- simple
- easy to validate

#### Disadvantages

- poor latency on long histories
- two-stage RPC plan
- forced reverse traversal
- weak parallelism

This architecture is the control baseline, not the target winner.

### Architecture B: Full-mode chronological scan

#### Flow

1. call `getTransactionsForAddress` with `transactionDetails: "full"`
2. set `sortOrder: "asc"`
3. paginate chronologically
4. compute balance delta per transaction
5. emit timeline

#### Advantages

- native chronological traversal
- no extra `getTransaction` calls
- simpler merge logic

#### Disadvantages

- page size capped at 100
- still expensive for large histories
- potentially suboptimal bandwidth use

Helius explicitly states that `full` eliminates the need for `getTransaction` calls but requires `limit <= 100`.

### Architecture C: Two-phase signatures-first scan

#### Flow

1. scan with `getTransactionsForAddress(..., transactionDetails: "signatures")`
2. use large pages and filters to map history cheaply
3. fetch full transaction data only for required windows
4. reconstruct balance timeline from narrowed subsets

#### Advantages

- lower initial bandwidth
- larger page sizes
- better coarse-to-fine search behavior

#### Disadvantages

- more complex control logic
- more merge complexity
- needs explicit refinement policy

This architecture is strong when history is large and full decoding everywhere is wasteful.

### Architecture D: Parallel range-partitioned scan

#### Flow

1. partition history by slot or block-time ranges
2. assign ranges to workers
3. query ranges in parallel using `getTransactionsForAddress`
4. decode and sort within each shard
5. merge shards by `(slot, transactionIndex)`
6. reconstruct cumulative balance

#### Advantages

- exploits bidirectional sorting and filters
- minimizes long serial pagination chains
- best latency profile for deep histories

#### Disadvantages

- hardest merge logic
- sensitive to boundary duplication / omission
- requires careful concurrency control

This should be the primary target architecture because Helius supports slot/blockTime/signature filters, chronological sorting, reverse sorting, and keyset pagination.

## Recommended System Design

### Recommended strategy

Use a **parallel, range-partitioned pipeline** with two execution modes:

#### Mode 1: simple mode
For short histories:
- `getTransactionsForAddress`
- `transactionDetails: "full"`
- `sortOrder: "asc"`
- sequential pagination

#### Mode 2: optimized mode
For deep histories:
- discovery pass using `signatures`
- range partitioning by slot or block time
- parallel shard fetch
- targeted full-tx retrieval only where needed
- deterministic merge and reconstruction

### Recommended internal modules

#### 1. Query planner
Responsible for:
- selecting execution mode
- selecting page size
- selecting concurrency
- generating slot/time partitions
- generating follow-up fetch plans

#### 2. RPC client
Responsible for:
- JSON-RPC transport
- retries
- backoff
- timeout policy
- pagination token handling

#### 3. Decoder
Responsible for:
- extracting `blockTime`
- extracting `slot`
- extracting `transactionIndex`
- identifying target account index
- computing lamport deltas

#### 4. Merger
Responsible for:
- shard ordering
- boundary deduplication
- total ordering by `(slot, transactionIndex, signature)`

#### 5. Reconstructor
Responsible for:
- cumulative balance reconstruction
- integrity checks
- final series emission

#### 6. Benchmark harness
Responsible for:
- fixed-wallet test sets
- latency measurement
- correctness comparison
- report generation

## Data Model

### Canonical transaction event

```text
TransactionEvent {
  signature: string
  slot: u64
  transactionIndex: u32
  blockTime: i64 | null
  err: any | null
  preBalanceLamports: u64
  postBalanceLamports: u64
  deltaLamports: i64
}
```

### Canonical balance point

```text
BalancePoint {
  signature: string
  slot: u64
  transactionIndex: u32
  blockTime: i64 | null
  balanceLamports: i128
}
```

## Algorithmic Rules

### Ordering rule

Primary ordering key:

\[
(slot,\ transactionIndex)
\]

Fallback tie-breaker:
- signature

### Delta rule

For target account index \(i\):

\[
\Delta_k = postBalances[i] - preBalances[i]
\]

### Accumulation rule

\[
B_0 = \text{initial balance}
\]
\[
B_k = B_{k-1} + \Delta_k
\]

### Pagination rule

Pagination must be monotone and lossless. For Helius pagination, preserve and reuse `paginationToken` exactly as returned. Helius documents the token format as `"slot:position"`.

### Boundary rule for parallel shards

Adjacent shards must overlap by a small boundary window or include explicit deduplication by:
- signature
- slot
- transactionIndex

This avoids edge loss at partition boundaries.

## Language and Runtime

### Recommended implementation language

**Rust**

Reason:
- low-overhead concurrency
- strong control over memory and allocation
- fast JSON parsing options
- deterministic pipeline structure
- best fit for a latency-oriented Solana data plane

### Secondary tooling

Optional:
- TypeScript or Python only for auxiliary benchmarking scripts or report generation
- not for the critical path

## Benchmark Plan

### Benchmark inputs

Use at least 4 wallet classes:

1. shallow / low-activity wallet
2. deep / high-activity wallet
3. bursty wallet with clustered activity
4. wallet with versioned transactions

### Benchmark outputs

For each implementation mode, record:

- total runtime
- RPC requests issued
- bytes transferred
- decode time
- merge time
- reconstruct time
- final row count
- correctness checksum

### Correctness oracle

Cross-check against:
- baseline reverse crawl
- spot validation on selected transaction windows
- consistency of cumulative ending balance

## Deliverables

### Required deliverables

1. runnable CLI
2. config file for endpoint, API key, concurrency, and mode
3. benchmark harness
4. correctness tests
5. markdown technical report
6. sample outputs for multiple wallet classes

### CLI shape

```text
sol-balance-history \
  --address <PUBKEY> \
  --mode <simple|optimized> \
  --rpc-url <URL> \
  --api-key <KEY> \
  --commitment finalized \
  --format <json|csv>
```

## Minimal Milestones

### Milestone 1
Implement baseline reverse crawl:
- `getSignaturesForAddress`
- `getTransaction`
- chronological reconstruction

### Milestone 2
Implement Helius full-mode chronological pipeline:
- `getTransactionsForAddress`
- `transactionDetails: "full"`
- `sortOrder: "asc"`

### Milestone 3
Implement optimized signatures-first mode:
- `transactionDetails: "signatures"`
- refinement fetches
- delta extraction

### Milestone 4
Implement parallel range partitioning:
- slot/block-time partitions
- shard merge
- dedupe

### Milestone 5
Benchmark and tune:
- concurrency
- partition width
- retry policy
- full vs signatures thresholds

## Project Success Criteria

The project is successful if it produces:

- exact SOL balance history
- deterministic ordering
- no missing transactions
- no duplicate transactions
- materially lower latency than the baseline reverse-crawl method

The architectural reason this is feasible is that Helius `getTransactionsForAddress` adds capabilities absent from the standard reverse-crawl flow: chronological sorting, advanced filtering, single-call full transaction retrieval, and keyset pagination.

## References

- Helius RPC: `getTransactionsForAddress`
- Solana RPC: `getSignaturesForAddress`
- Solana RPC: `getTransaction`
- Helius API Reference: `getTransactionsForAddress`
