#!/usr/bin/env bash
set -euo pipefail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required to summarize benchmark JSON output" >&2
  exit 1
fi

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root_dir"

out_dir="${1:-benchmarks/$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$out_dir"
summary="$out_dir/summary.csv"

cargo build --release

printf '%s\n' \
  'window,target,address,variant,mode,start_slot,end_slot,elapsed_ms,rpc_requests,full_pages,signature_pages,rows,partitions,checksum,output' \
  > "$summary"

windows=(
  'walletmaster_500|walletmaster_sample|7x6qE3DRMW2ZCgT1YQuBLePiheEWw7qjH6rYjj6GDtEd|383732198|385119911'
  'walletmaster_2000|walletmaster_sample|7x6qE3DRMW2ZCgT1YQuBLePiheEWw7qjH6rYjj6GDtEd|383732198|390238037'
  'spl_token_500|spl_token_program|TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA|31303514|31303565'
  'spl_token_2000|spl_token_program|TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA|31303514|31372121'
)

variants=(
  'simple-p100|simple|100|1|1'
  'opt-p8-c8|optimized|100|8|8'
  'opt-p16-c8|optimized|100|16|8'
  'opt-p32-c16|optimized|100|32|16'
  'adaptive-p8-c8|adaptive|100|8|8'
  'adaptive-p16-c8|adaptive|100|16|8'
  'adaptive-p32-c16|adaptive|100|32|16'
  'mapped-p8-c8|mapped|100|8|8'
  'mapped-p16-c8|mapped|100|16|8'
  'mapped-p32-c16|mapped|100|32|16'
  'pipelined-c8|pipelined|100|8|8'
  'pipelined-c16|pipelined|100|16|16'
)

for window in "${windows[@]}"; do
  IFS='|' read -r window_label target address start_slot end_slot <<< "$window"

  for variant in "${variants[@]}"; do
    IFS='|' read -r variant_label mode page_limit partitions concurrency <<< "$variant"
    output="$out_dir/${window_label}_${variant_label}.json"

    echo "running $window_label $variant_label"
    ./target/release/sol-balance-history \
      --address "$address" \
      --mode "$mode" \
      --start-slot "$start_slot" \
      --end-slot "$end_slot" \
      --page-limit "$page_limit" \
      --partitions "$partitions" \
      --concurrency "$concurrency" \
      --format json \
      > "$output"

    jq -r \
      --arg window "$window_label" \
      --arg target "$target" \
      --arg address "$address" \
      --arg variant "$variant_label" \
      --arg mode "$mode" \
      --arg start_slot "$start_slot" \
      --arg end_slot "$end_slot" \
      --arg output "$output" \
      '[
        $window,
        $target,
        $address,
        $variant,
        $mode,
        $start_slot,
        $end_slot,
        (.metrics.elapsed_ms | tostring),
        (.metrics.rpc_requests | tostring),
        (.metrics.full_pages | tostring),
        (.metrics.signature_pages | tostring),
        (.summary.transaction_count | tostring),
        (.metrics.partitions | tostring),
        (.summary.checksum | tostring),
        $output
      ] | @csv' \
      "$output" >> "$summary"
  done
done

echo "wrote $summary"
