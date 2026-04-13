# Mert / Helius SOL Balance Contest Research

Research date: 2026-04-13

This note collects the public material I could verify around Mert's Helius challenge for the lowest-latency SOL balance-over-time / PnL tool using Helius `getTransactionsForAddress`, plus public repos that look like contest replies or submissions.

## Source Thread Context

Primary X access was limited: X itself and public X mirrors exposed profiles/search snippets, but did not reliably expose the full reply tree or a stable direct tweet URL for the `$1,500` contest post.

Verified context:

- Mert's public profile is `@mert`, described by TwStalker as `ceo @helius, @checkprice` with Helius RPC/API/HFT infra in the bio: <https://site.twstalker.com/mert>
- Search-indexed TwStalker snippets for Helius / Helius team accounts show the `getTransactionsForAddress` launch context and Mert/Helius "time to build" framing around the new method.
- Repo descriptions and READMEs created on April 11-12, 2026 repeatedly use the same challenge wording: "lowest latency SOL balance-over-time", "Mert's Solana dev challenge", "Helius Mini Solana Dev Weekend submission", and "only RPC / no indexing".

Working interpretation:

- The contest appears to be the same challenge described in this repo's `AGENTS.md`: build the fastest runtime SOL balance-change / balance-curve tool for a Solana address with Helius `getTransactionsForAddress`, no prebuilt index, no database, no cached history.
- The public artifacts below are the highest-confidence repos I found that match that prompt.

## High-Confidence Contest Repos

### 1. `shariqazeem/sol-pnl`

- URL: <https://github.com/shariqazeem/sol-pnl>
- Raw README: <https://raw.githubusercontent.com/shariqazeem/sol-pnl/main/README.md>
- Created: 2026-04-11 13:09:23 UTC
- Updated / pushed: 2026-04-12
- Language: TypeScript
- GitHub description: "Lowest latency SOL balance-over-time algorithm. Helius Mini Solana Dev Weekend submission."
- README claim: "Lowest-latency SOL balance-over-time using only Helius RPC. No indexing, no databases."
- Reported result: p50 1.41s on a roughly 4,000 transaction wallet, 145 calls, 3,957 balance points.
- Main idea: three-phase adaptive pipeline:
  - R1 probe with parallel asc/desc full/signature calls.
  - R2 signature prefetch with parallel pipes.
  - R3 synthetic pagination tokens at 100-transaction boundaries to parallelize full page fetches.
- Notes relevant to our repo:
  - The README explicitly says failed transactions are included.
  - It uses `maxSupportedTransactionVersion: 0`.
  - It dedupes by signature.
  - The "synthetic pagination token" idea is aggressive and worth verifying against Helius token semantics because our project rules say to preserve Helius `paginationToken` exactly.

### 2. `dogame-art/sol-balance-curve`

- URL: <https://github.com/dogame-art/sol-balance-curve>
- Raw README: <https://raw.githubusercontent.com/dogame-art/sol-balance-curve/main/README.md>
- Created: 2026-04-11 21:33:42 UTC
- Updated / pushed: 2026-04-11
- Language: JavaScript
- GitHub description: "Lowest latency SOL balance-over-time using only RPC. Recursive density-adaptive curve builder for @mert's Solana dev challenge."
- README claim: "Competition submission for @mert's Solana dev challenge: Lowest latency SOL balance-over-time using only RPC - no indexing."
- Reported result:
  - Small wallet: 1,670 txs, 3.1s, 37 RPC calls.
  - Medium wallet: 4,384 txs, 3.8s, 68 RPC calls.
  - Busy wallet: 12,738 txs, 20.3s, 355 RPC calls.
- Main idea: build a full SOL balance curve first, then answer PnL/time-window queries from the in-memory curve.
- Algorithm:
  - Scout with parallel `getTransactionsForAddress` calls from both ends.
  - Use observed density to create mega-chunks.
  - Each chunk runs signature sweep, partition, full fetch, then stitch.
- Notes relevant to our repo:
  - Strong alignment with our current `mapped` / `adaptive` direction.
  - "Observed density" partition sizing is directly applicable to horizon or slot-range scheduling.
  - The README mentions comparing against `getSignaturesForAddress`; for this challenge, our production path must not replace the core historical source with `getSignaturesForAddress`.

### 3. `hitman-kai/darkpnl`

- URL: <https://github.com/hitman-kai/darkpnl>
- Raw README: <https://raw.githubusercontent.com/hitman-kai/darkpnl/main/README.md>
- Created: 2026-04-12 04:32:38 UTC
- Updated / pushed: 2026-04-12
- Language: Python
- GitHub description: "Lowest latency SOL balance-over-time solver - by @hitmannolimit"
- README claim: "Lowest latency SOL balance-over-time solver using Helius getTransactionsForAddress."
- Main idea: bidirectional pincer pagination:
  - Round trip 1: fire asc and desc simultaneously, 100 transactions each.
  - Round trip 2 onward: paginate both directions until the sides meet.
- Reported complexity: roughly `O(n / 200)` RTTs instead of `O(n / 100)` sequential.
- Notes relevant to our repo:
  - Useful as a simple competitive baseline above naive pagination.
  - Less sophisticated than density partitioning for deep wallets because it still depends on many serial-ish pagination rounds.

## Related / Low-Confidence Repos

These showed up in broader GitHub searches for Helius + Solana + PnL, but I would not treat them as confirmed replies/submissions to Mert's contest without additional evidence.

### `bellathebot/solana-trading-dashboard`

- URL: <https://github.com/bellathebot/solana-trading-dashboard>
- Created: 2026-03-17
- Language: JavaScript
- Description: "Terminal dashboard for Solana trading activity and PNL tracking using Jupiter and Helius CLIs"
- Why low-confidence: created before the apparent April 2026 contest window and looks like a general dashboard, not the `getTransactionsForAddress` latency challenge.

### `lawphilly/TrenchJournal`

- URL: <https://github.com/lawphilly/TrenchJournal>
- Created: 2025-01-03
- Language: Python
- Description says it analyzes Solana wallet transactions using Helius and calculates basic PnL.
- Why low-confidence: much older than the contest and product-shaped, not a runtime latency challenge submission.

## Gists

No public `gist.github.com` links were found in the indexed searches I ran for combinations of:

- `getTransactionsForAddress` + `gist.github.com`
- `Mert` + `Helius` + `gist.github.com`
- `SOL balance-over-time` + `gist.github.com`
- `Solana dev challenge` + `gist.github.com`

Because X reply access was incomplete, this means "no indexed gists found", not "no gists exist in the reply tree".

## Search Notes

Searches performed:

- Web search for the Mert / Helius `$1500` contest post and `getTransactionsForAddress` challenge language.
- X mirror lookup through TwStalker for Mert / Helius profiles and snippets.
- GitHub repository search through public GitHub API for:
  - `helius solana pnl`
  - `solana pnl helius`
  - `SOL balance-over-time`
  - `Helius Mini Solana Dev Weekend`
  - `Mert's Solana dev challenge`
  - `lowest latency SOL balance-over-time`
- Raw README fetches for the three high-confidence repos.

Limitations:

- X and X mirror pages did not expose a complete reply tree in this environment.
- GitHub unauthenticated API rate limiting kicked in after several searches.
- I did not find a stable direct URL for the original contest tweet; the strongest public evidence is the cluster of April 11-12 repo submissions that explicitly name the challenge.

## Takeaways For This Repo

- The competitive frontier is not `simple`; all serious submissions are doing some form of parallel range discovery, density classification, or bidirectional pagination.
- The best ideas to consider next are:
  - observed-density partition sizing,
  - full balance-curve construction as the primitive,
  - explicit per-partition metrics,
  - bidirectional fetching as a fallback for small and medium wallets,
  - careful validation of any synthetic pagination-token approach before using it.
- Keep this repo's correctness policy intact:
  - use raw `postBalances[i] - preBalances[i]`,
  - include failed transactions,
  - dedupe by signature,
  - keep lamport arithmetic integer-only,
  - do not claim external-flow-adjusted economic PnL.
