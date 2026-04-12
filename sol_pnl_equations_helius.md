Use Helius `getTransactionsForAddress` when you want paginated address history with filtering and sorting, or use the enhanced `/v0/addresses/{address}/transactions` endpoint when you want already-parsed history. The enhanced response includes fields such as `fee`, `feePayer`, `nativeTransfers`, `tokenTransfers`, and per-account `nativeBalanceChange` / `tokenBalanceChanges`. For exact reconciliation, raw `getTransaction` / full transaction responses expose `meta.fee`, `preBalances`, `postBalances`, `preTokenBalances`, `postTokenBalances`, and `rewards`. Helius also has official Node.js and Rust SDKs. ([helius.dev](https://www.helius.dev/docs/api-reference/rpc/http/gettransactionsforaddress))

You cannot make a literally finite list of **every** possible SOL-PnL equation, because once you allow different lot-selection rules, mark sources, fee capitalization policies, and deposit/withdrawal classifications, the variants are unbounded. The right way is to write the **general equation family** and then enumerate the standard special cases. That is what follows.

## 1) Core notation

Let:

- `sol(x) = x / 1e9`  
- `W` = all addresses you treat as owned by the same portfolio  
- `k` = a non-SOL asset  
- `q_k(t)` = quantity of asset `k` held at time `t`  
- `p_k^SOL(t)` = price of asset `k` in SOL at time `t`  
- `L(t)` = liabilities marked in SOL at time `t`  
- `E^SOL(t)` = total portfolio equity in SOL  
- `D(0,T)` = total external deposits over `[0,T]`, in SOL terms  
- `X(0,T)` = total external withdrawals over `[0,T]`, in SOL terms  

For transaction `τ`:

- `ΔL_{a,τ}` = lamport delta for owned account `a`
- `Δq_{k,τ}` = token quantity delta for asset `k`
- `fee_τ^SOL` = explicit tx fee in SOL
- `tip_τ^SOL` = Jito / priority / extra explicit SOL outflow, if separated in your model

## 2) Helius-native balance equations

These are the two primary native-SOL delta equations you should use.

### Raw transaction reconciliation
For an owned account `a` in tx `τ`:

\[
\Delta SOL_{a,\tau} = \mathrm{sol}(postBalances_{a,\tau} - preBalances_{a,\tau})
\]

Portfolio-native delta for the tx:

\[
\Delta SOL_{\tau} = \sum_{a \in W} \Delta SOL_{a,\tau}
\]

### Enhanced transaction reconciliation
If you trust the enhanced per-account native change:

\[
\Delta SOL_{a,\tau} = \mathrm{sol}(nativeBalanceChange_{a,\tau})
\]

and again:

\[
\Delta SOL_{\tau} = \sum_{a \in W} \Delta SOL_{a,\tau}
\]

Do **not** add `fee`, `rewards`, and `nativeBalanceChange` together unless you are intentionally decomposing the same already-realized balance movement, because that creates double counting. The safest source of truth is always actual balance deltas from `post - pre` or enhanced `nativeBalanceChange`. Helius and Solana both expose those balance-change fields directly. ([helius.dev](https://www.helius.dev/docs/api-reference/enhanced-transactions/gettransactionsbyaddress))

## 3) Token quantity equations

From raw token balances:

\[
\Delta q_{k,\tau} = q_{k,post,\tau} - q_{k,pre,\tau}
\]

From enhanced transaction parsing, the same idea applies using token balance changes per owner / token account / mint. Solana raw responses expose `preTokenBalances` and `postTokenBalances` with mint, owner, and `uiTokenAmount`; enhanced responses expose token balance change objects keyed to accounts and mints. ([helius.dev](https://www.helius.dev/docs/api-reference/enhanced-transactions/gettransactionsbyaddress))

## 4) SOL normalization equations

### Native SOL + wrapped SOL
If you want SOL basis only, wrapped SOL should usually be collapsed into SOL:

\[
q_{SOL,\mathrm{effective}}(t) = q_{SOL,\mathrm{native}}(t) + q_{wSOL}(t)
\]

### Price conversion into SOL
If you only have USD marks:

\[
p_k^{SOL}(t) = \frac{p_k^{USD}(t)}{p_{SOL}^{USD}(t)}
\]

If you only have a cross price versus another asset `x`:

\[
p_k^{SOL}(t) = p_k^{x}(t) \cdot p_x^{SOL}(t)
\]

That is the definition of “SOL basis only”: everything becomes a SOL-denominated mark.

## 5) Master equity equation

This is the canonical portfolio equity equation:

\[
E^{SOL}(t) = q_{SOL,\mathrm{effective}}(t) + \sum_{k \neq SOL} q_k(t)\,p_k^{SOL}(t) - L^{SOL}(t)
\]

Expanded version for DeFi:

\[
E^{SOL}(t) =
q_{SOL,\mathrm{effective}}(t)
+ \sum_{spot\ tokens} q_k(t)\,p_k^{SOL}(t)
+ V_{NFT}^{SOL}(t)
+ V_{LP}^{SOL}(t)
+ V_{staked}^{SOL}(t)
+ V_{lending\ claims}^{SOL}(t)
- V_{borrowed}^{SOL}(t)
\]

## 6) The single best total PnL equation

This is the most robust equation:

\[
PnL_{net}^{SOL}(0,T) = E^{SOL}(T) - E^{SOL}(0) - D(0,T) + X(0,T)
\]

Meaning:

- ending equity
- minus starting equity
- minus capital you added
- plus capital you removed

This is the correct **economic** PnL equation in SOL terms.

Equivalent recurrence form:

\[
PnL_{net,\tau}^{SOL} = \Delta E_\tau^{SOL} - Deposit_\tau^{SOL} + Withdrawal_\tau^{SOL}
\]

and

\[
PnL_{net}^{SOL}(0,T)=\sum_{\tau} PnL_{net,\tau}^{SOL}
\]

## 7) Gross vs net PnL

If you want gross PnL before explicit fees:

\[
PnL_{gross}^{SOL} = PnL_{net}^{SOL} + \sum_\tau fee_\tau^{SOL} + \sum_\tau tip_\tau^{SOL} + \sum_\tau otherExpensedCosts_\tau^{SOL}
\]

If instead you capitalize fees into basis, then net vs gross changes at the trade level, not at the portfolio-equity level.

## 8) Generalized family that covers almost all variants

Define four policy choices:

- `M` = mark source for `p_k^SOL(t)`  
- `σ` = lot-selection rule  
- `φ` = fee policy  
- `χ` = flow-classification policy  

Then the full generalized PnL family is:

\[
PnL^{M,\sigma,\phi,\chi}(0,T)
=
E^{M,SOL}(T)
-
E^{M,SOL}(0)
-
D^\chi(0,T)
+
X^\chi(0,T)
\]

where

\[
E^{M,SOL}(t)=q_{SOL,\mathrm{effective}}(t)+\sum_{k \neq SOL} q_k(t)\,p_{k,M}^{SOL}(t)-L_M^{SOL}(t)
\]

This one equation family covers nearly every practical implementation.

## 9) Spot trade equations: buy with SOL

Suppose you buy asset `k` using SOL.

### Gross basis added
\[
BasisAdd_k^{SOL} = SOL_{spent} + Fees_{capitalized}^{SOL}
\]

### Unit cost
\[
c_k^{SOL} = \frac{BasisAdd_k^{SOL}}{q_{k,bought}}
\]

### If fees are expensed instead of capitalized
\[
BasisAdd_k^{SOL} = SOL_{spent}
\]

and the fee hits current-period PnL separately.

## 10) Spot trade equations: sell into SOL

Suppose you sell quantity `Q` of asset `k` for SOL.

### Net proceeds
\[
Proceeds_k^{SOL} = SOL_{received} - Fees_{expensed}^{SOL}
\]

### Realized PnL
\[
RealizedPnL_k^{SOL} = Proceeds_k^{SOL} - Basis_k^{SOL}(Q)
\]

Everything now reduces to how `Basis_k^{SOL}(Q)` is chosen.

## 11) Generic lot-selection basis equation

Let open lots for asset `k` be indexed by `j`, each with quantity `n_j` and unit cost `c_j`.

For a disposal of `Q` units, pick consumed lot portions `m_j` such that:

\[
0 \le m_j \le n_j,\qquad \sum_j m_j = Q
\]

Then the generic basis is:

\[
Basis_k^{SOL}(Q;\sigma) = \sum_j m_j\,c_j
\]

This is the master cost-basis equation.

### Special cases

#### FIFO
Oldest lots first.

\[
Basis_{FIFO}(Q)=\sum_{j \in oldest} m_j c_j
\]

#### LIFO
Newest lots first.

\[
Basis_{LIFO}(Q)=\sum_{j \in newest} m_j c_j
\]

#### HIFO
Highest unit-cost lots first.

\[
Basis_{HIFO}(Q)=\sum_{j \in highest\ cost} m_j c_j
\]

#### LOFO
Lowest unit-cost lots first.

\[
Basis_{LOFO}(Q)=\sum_{j \in lowest\ cost} m_j c_j
\]

#### Specific identification
You choose exact lots:

\[
Basis_{SpecID}(Q)=\sum_{j \in selected} m_j c_j
\]

#### Weighted average cost
Let open inventory before the disposal be:

\[
Q_{open} = \sum_j n_j
\]

\[
B_{open}^{SOL} = \sum_j n_j c_j
\]

Then average cost per unit is:

\[
\bar c = \frac{B_{open}^{SOL}}{Q_{open}}
\]

and disposal basis is:

\[
Basis_{WAC}(Q)=Q\bar c
\]

## 12) Unrealized PnL equations

### Lot-based unrealized PnL
\[
UnrealizedPnL_k^{SOL}(T)=\sum_{j \in open\ lots} n_j\left(p_k^{SOL}(T)-c_j\right)
\]

### Average-cost unrealized PnL
\[
UnrealizedPnL_k^{SOL}(T)=q_k(T)\left(p_k^{SOL}(T)-\bar c_k\right)
\]

### Total unrealized
\[
UnrealizedPnL^{SOL}(T)=\sum_k UnrealizedPnL_k^{SOL}(T)
\]

## 13) Realized + unrealized decomposition

If you use a consistent mark and basis policy:

\[
TotalPnL^{SOL}
=
RealizedPnL^{SOL}
+
UnrealizedPnL^{SOL}
+
IncomePnL^{SOL}
-
ExpensePnL^{SOL}
\]

At the full-portfolio level, this should reconcile back to:

\[
E_T - E_0 - D + X
\]

## 14) Token-to-token swap equations in SOL terms

Suppose you swap `a -> b`.

You need a SOL valuation for the execution.

### Value using received side
\[
V_{swap}^{SOL} = q_{b,recv}\,p_b^{SOL}(t_{exec})
\]

### Value using sent side
\[
V_{swap}^{SOL} = q_{a,sent}\,p_a^{SOL}(t_{exec})
\]

### New basis for acquired token
If fees are capitalized:

\[
BasisAdd_b^{SOL}=V_{swap}^{SOL}+Fees_{capitalized}^{SOL}
\]

### Realized PnL on disposed token
\[
RealizedPnL_a^{SOL}=V_{swap}^{SOL}-Fees_{expensed}^{SOL}-Basis_a^{SOL}(q_{a,sent})
\]

This is the cleanest token-to-token SOL accounting.

## 15) External deposits and withdrawals

### External deposit of SOL
\[
D_{\tau}^{SOL} = SOL_{received\ from\ outside}
\]

### External withdrawal of SOL
\[
X_{\tau}^{SOL} = SOL_{sent\ to\ outside}
\]

### External deposit of non-SOL asset `k`
Convert at chosen SOL mark:

\[
D_{\tau}^{SOL} = q_{k,deposit}\,p_k^{SOL}(t_\tau)
\]

### External withdrawal of non-SOL asset `k`
\[
X_{\tau}^{SOL} = q_{k,withdraw}\,p_k^{SOL}(t_\tau)
\]

These are **not** trading PnL. They are capital flows.

## 16) Internal-transfer equations

If both source and destination are owned:

\[
Deposit_\tau^{SOL}=0,\qquad Withdrawal_\tau^{SOL}=0,\qquad PnL_\tau^{SOL}= - Fees_\tau^{SOL}
\]

Ignoring fees, pure internal transfers are PnL-neutral.

## 17) Staking equations

### Native SOL staking reward
If reward lands as SOL or lamport reward:

\[
Income_{stake,\tau}^{SOL}=Reward_\tau^{SOL}
\]

### Stake principal movement
Delegation, undelegation, or movement between owned wallet and owned stake account:

\[
PnL=0
\]

unless fees or slashing are involved.

Solana transaction metadata includes reward records and Helius raw transaction data exposes rewards and balance changes, so stake-related realized changes should be reconciled from those actual deltas. ([solana.com](https://solana.com/docs/rpc/json-structures))

## 18) Airdrop / rebate / free-token equations

There are two common accounting treatments.

### Zero-basis treatment
\[
BasisAdd_k^{SOL}=0
\]

Then later:

\[
RealizedPnL_k^{SOL}=Proceeds_k^{SOL}
\]

### FMV-at-receipt treatment
At receipt time:

\[
Income_\tau^{SOL}=q_k\,p_k^{SOL}(t_\tau)
\]

and set:

\[
BasisAdd_k^{SOL}=q_k\,p_k^{SOL}(t_\tau)
\]

Then later on sale:

\[
RealizedPnL_k^{SOL}=Proceeds_k^{SOL}-Basis_k^{SOL}
\]

## 19) Rent equations

### Rent paid to create token / ATA / stake-related account
Either expense it:

\[
Expense_{rent,\tau}^{SOL}=RentPaid_\tau^{SOL}
\]

or capitalize it into the basis of the created position:

\[
BasisAdd^{SOL}=BasisAdd^{SOL}+RentPaid_\tau^{SOL}
\]

### Rent refund from closing account
\[
RentRefund_\tau^{SOL}=SOL_{received}
\]

You can treat that as:
- positive PnL, or
- reversal of previously reserved capital

but you must be consistent.

## 20) NFT equations in SOL terms

### Buy NFT with SOL
\[
BasisAdd_{NFT}^{SOL}=SOL_{spent}+Fees^{SOL}
\]

### Sell NFT for SOL
\[
RealizedPnL_{NFT}^{SOL}=SOL_{received}-Fees^{SOL}-Basis_{NFT}^{SOL}
\]

### Unrealized NFT PnL
\[
UnrealizedPnL_{NFT}^{SOL}=Mark_{NFT}^{SOL}(T)-Basis_{NFT}^{SOL}
\]

where `Mark_NFT^SOL` could be floor, last sale, collection bid, or your own model.

## 21) LP equations

### Add liquidity
\[
BasisAdd_{LP}^{SOL}
=
\sum_i q_{i,contributed}\,p_i^{SOL}(t_{add})
+
Fees_{capitalized}^{SOL}
\]

### LP position value
\[
V_{LP}^{SOL}(t)
=
\sum_i q_{i,claimable}(t)\,p_i^{SOL}(t)
\]

### Remove liquidity
\[
Proceeds_{LP}^{SOL}
=
\sum_i q_{i,received}\,p_i^{SOL}(t_{remove})
-
Fees_{expensed}^{SOL}
\]

### Realized PnL on LP exit
\[
RealizedPnL_{LP}^{SOL}=Proceeds_{LP}^{SOL}-Basis_{LP}^{SOL}
\]

### Impermanent loss in SOL
\[
IL^{SOL}(t)=V_{HODL}^{SOL}(t)-V_{LP}^{SOL}(t)
\]

## 22) Farming / rewards equations

### Farming reward
\[
Income_{farm,\tau}^{SOL}=q_{reward,\tau}\,p_{reward}^{SOL}(t_\tau)
\]

or zero-basis if you want all value recognized later on disposal.

## 23) Lending / borrowing equations

### Supplied-asset value
\[
V_{supply}^{SOL}(t)=q_{supply}(t)\,p_{asset}^{SOL}(t)
\]

### Borrow liability
\[
V_{borrow}^{SOL}(t)=q_{borrow}(t)\,p_{debt}^{SOL}(t)
\]

### Net lending equity
\[
NetLendingEquity^{SOL}(t)=V_{supply}^{SOL}(t)-V_{borrow}^{SOL}(t)
\]

### Interest income
If claim grows by `Δq`:
\[
InterestIncome_\tau^{SOL}=\Delta q\,p_{asset}^{SOL}(t_\tau)
\]

### Interest expense
\[
InterestExpense_\tau^{SOL}=\Delta q_{debt}\,p_{debt}^{SOL}(t_\tau)
\]

## 24) Liquidation equation

If a liquidation seizes assets worth `S` SOL while reducing debt worth `R` SOL and charging fees `F`:

\[
LiquidationPnL^{SOL}=R-S-F
\]

Equivalent loss form:

\[
LiquidationLoss^{SOL}=S+F-R
\]

## 25) Perps / derivatives equations in SOL terms

If the venue gives you PnL in USD or USDC, convert it:

\[
PnL_{perp}^{SOL} = \frac{PnL_{perp}^{USD}}{p_{SOL}^{USD}}
\]

If you track size directly:

\[
UPnL_{perp}^{SOL}
=
\frac{Direction \cdot Size \cdot (Mark-Entry)\cdot Multiplier}{p_{SOL}^{USD}}
\]

and net realized:

\[
RPnL_{perp}^{SOL}=CashSettlement^{SOL}-Funding^{SOL}-Fees^{SOL}
\]

For many Solana derivatives positions, transaction history alone is enough for settled cash flows but not enough for open-position unrealized PnL unless you also read current position state and marks.

## 26) Reconciliation equations

### Equity bridge
\[
E_T^{SOL}=E_0^{SOL}+PnL_{net}^{SOL}+D-X
\]

### Period bridge
\[
PnL_{net}^{SOL}=\Delta E^{SOL}-D+X
\]

### Inventory bridge
For each asset:
\[
q_k(T)=q_k(0)+\sum_\tau \Delta q_{k,\tau}
\]

### Native SOL bridge
\[
q_{SOL,\mathrm{effective}}(T)
=
q_{SOL,\mathrm{effective}}(0)
+
\sum_\tau \Delta SOL_\tau
\]

## 27) What I would actually use

For a real implementation, use these three equations as the backbone:

### Best total PnL
\[
PnL_{net}^{SOL}=E_T^{SOL}-E_0^{SOL}-D+X
\]

### Best realized trade PnL
\[
RealizedPnL=Proceeds-Basis
\]

with `Basis` chosen by `FIFO`, `HIFO`, or `WAC`.

### Best unrealized PnL
\[
UnrealizedPnL=\sum_{open\ positions} q\,(mark^{SOL}-cost^{SOL})
\]

## 28) Practical rule set for Helius

Use Helius history for:
- actual realized native SOL changes
- token quantity changes
- explicit tx fees
- transfer classification
- reward detection
- swap / transfer parsing

Use a separate pricing function for:
- marking open positions in SOL
- converting non-SOL external deposits / withdrawals into SOL
- token-to-token swap valuation when the execution itself is not directly against SOL

Helius gives you the transaction history, parsed transfers, and balance-change data; the docs for those endpoints are about transaction retrieval and parsing rather than live portfolio marking, so unrealized SOL-PnL needs a separate mark source on top of that history. ([helius.dev](https://www.helius.dev/docs/api-reference/rpc/http/gettransactionsforaddress))

The cleanest single answer is this:

\[
\boxed{
PnL_{net}^{SOL}=E_T^{SOL}-E_0^{SOL}-ExternalDeposits^{SOL}+ExternalWithdrawals^{SOL}
}
\]

Everything else is just the decomposition.

Next step would be a Rust reducer that consumes Helius history and computes:
1. net portfolio PnL in SOL,  
2. realized PnL in SOL by lot method,  
3. unrealized PnL in SOL by current mark,  
4. fee/rent/reward breakdown in SOL.
