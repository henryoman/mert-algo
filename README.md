# SOL Balance History Reconstruction

This project will reconstruct an address's native SOL balance as a time series using live RPC queries only, with no prebuilt indexing layer.

## Documents Read

This plan is based on:

1. `helius-rust-sdk-best-practices.md`
2. `sol_balance_history_project_treatment.md`
3. `sol_pnl_equations_helius.md`

## Key Decisions From The Source Material

### Helius SDK usage

- Prefer `get_transactions_for_address` over a two-step signature lookup when possible.
- Use `transaction_details: Full` when reconstructing exact balance deltas from transaction metadata.
- Preserve `pagination_token` exactly as returned.
- Treat `postBalances[i] - preBalances[i]` as the source of truth for native SOL deltas.
- Retry `RateLimitExceeded` and `InternalError` with exponential backoff.
- Avoid polling loops and use structured pagination instead.

Important SDK note from the docs: include the page's agent instructions in any summary. The page says documentation feedback should only be submitted when there is something specific and actionable to report.

### Project architecture

- Baseline control path: `getSignaturesForAddress` + `getTransaction`.
- Primary implementation target: Helius chronological full-mode scan.
- Final target architecture: parallel range-partitioned pipeline with deterministic merge.
- Canonical ordering key: `(slot, transactionIndex)`, then `signature` as a tie-breaker.
- Canonical balance reconstruction rule: cumulative sum of per-transaction lamport deltas.

### PnL document relevance

The PnL document confirms the accounting foundation we need for this project:

- Native SOL delta should be computed from actual balance changes, not by separately summing fee or reward fields.
- `Delta SOL = sol(postBalances - preBalances)` is the correct base reconciliation equation.
- For this project, exact balance reconstruction is in scope; full portfolio PnL and marking are not.

## Recommended Build Sequence

The smallest correct path is:

1. Build a runnable CLI skeleton.
2. Implement the baseline reverse crawl for correctness.
3. Implement Helius full chronological mode as the first serious fast path.
4. Add deterministic sorting, dedupe, and balance reconstruction.
5. Add configuration, retries, and output formatting.
6. Add benchmark and correctness harnesses.
7. Only then add optimized signatures-first and parallel partitioning.

## Concrete Plan To Embark

### Phase 1: Establish the executable skeleton

Deliverables:

- `Cargo.toml` with stable dependencies.
- `src/main.rs` CLI entrypoint.
- `src/config.rs` for runtime options.
- `src/types.rs` for canonical transaction and balance-point models.
- `src/output.rs` for JSON and CSV writers.

Acceptance criteria:

- `cargo run -- --help` works.
- CLI supports `--address`, `--mode`, `--rpc-url`, `--api-key`, `--format`, and `--commitment`.

### Phase 2: Baseline correctness path

Implement a portable baseline using:

- `getSignaturesForAddress`
- `getTransaction`

Responsibilities:

- Fetch signatures newest-to-oldest.
- Fetch full transaction data by signature.
- Extract the target account's index.
- Compute `delta_lamports = postBalances[i] - preBalances[i]`.
- Reverse or sort chronologically.
- Reconstruct cumulative balances.

Acceptance criteria:

- Produces ordered output.
- Handles missing account index safely.
- Handles failed transactions consistently.

### Phase 3: Helius simple mode

Implement the recommended short-history path using:

- `get_transactions_for_address`
- `transaction_details: Full`
- `sort_order: asc`

Responsibilities:

- Sequential full-mode pagination.
- Preserve `pagination_token` exactly.
- Decode deltas directly from transaction metadata.
- Emit final `BalancePoint` records.

Acceptance criteria:

- Matches the baseline on sampled wallets.
- Uses fewer round trips than baseline.

### Phase 4: Deterministic data pipeline

Add internal modules for:

- decoder
- merger
- reconstructor
- retry-aware RPC wrapper

Core rules:

- Deduplicate by `signature`, `slot`, and `transactionIndex`.
- Order by `(slot, transactionIndex, signature)`.
- Keep comments minimal and only where logic is not obvious.

### Phase 5: Benchmark and correctness harness

Build a harness that records:

- total runtime
- RPC requests
- decode time
- merge time
- reconstruct time
- final row count

Wallet classes to test:

1. shallow wallet
2. deep wallet
3. bursty wallet
4. wallet with versioned transactions

### Phase 6: Optimized mode

Only after the above is stable:

- add signatures-first discovery mode
- add slot or block-time partitioning
- add parallel shard fetch
- add deterministic merge with boundary overlap and dedupe

This is the highest-complexity piece and should be deferred until the correctness path is proven.

## Initial File Layout

```text
sol-balance-history/
  Cargo.toml
  README.md
  src/
    main.rs
    config.rs
    types.rs
    output.rs
    baseline.rs
    helius_simple.rs
    reconstruct.rs
    rpc.rs
```

## Immediate Next Build Target

The next coding step should be to implement a minimal but runnable CLI plus the canonical data types. After that, baseline reverse crawl should be added before attempting the optimized architecture.

## Notes

- The project treatment recommends Rust, and that is the right choice here.
- The PnL document is useful mainly as a guardrail against double counting; the core project is balance-history reconstruction, not full portfolio accounting.
- The Helius SDK best-practices document strongly supports a direct `get_transactions_for_address` path for the non-baseline implementation.
