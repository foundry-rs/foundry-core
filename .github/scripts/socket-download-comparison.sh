#!/usr/bin/env bash
set -euo pipefail

# Download-only diagnostic. The direct passes deliberately invoke the underlying
# Cargo binary without Socket; StepSecurity remains active for all three passes.
results="$RUNNER_TEMP/socket-download-comparison"
mkdir -p "$results"
real_cargo="$(rustup which cargo)"
socket_cargo="$(command -v cargo)"
[[ "$real_cargo" = /* && -x "$real_cargo" ]]
[[ "$socket_cargo" == */tempo-sfw-*/bin/cargo ]]
args=(metadata --format-version=1 --all-features --filter-platform x86_64-unknown-linux-gnu --locked)

{
  git rev-parse HEAD
  "$real_cargo" --version --verbose
  rustc --version --verbose
  sha256sum Cargo.lock
  printf 'Direct executable: %s\nSocket shim: %s\n' "$real_cargo" "$socket_cargo"
  printf 'Command: cargo %s\n' "${args[*]}"
} > "$results/environment.txt"
cat "$results/environment.txt"
printf '| Pass | Exit code | Seconds | Cached crate archives |\n| --- | --- | --- | --- |\n' > "$results/summary.md"

failed=0
for mode in direct-before socket direct-after; do
  # mktemp creates a new, empty home for each pass; no cache restore or sharing.
  cargo_home="$(mktemp -d "$RUNNER_TEMP/cargo-comparison-$mode-XXXXXX")"
  report="$results/$mode-socket-report.json"
  command=(env "CARGO_HOME=$cargo_home" "SFW_JSON_REPORT_PATH=$report")
  if [[ "$mode" = socket ]]; then
    command+=("$socket_cargo")
  else
    # Clear proxy overrides and inhibit interception in any descendant Bash.
    command+=(_TEMPO_SFW_ACTIVE=1 CARGO_HTTP_PROXY= HTTP_PROXY= HTTPS_PROXY= ALL_PROXY= http_proxy= https_proxy= all_proxy= "$real_cargo")
  fi
  printf 'Starting %s at %s with empty cache %s\n' "$mode" "$(date -u +%FT%TZ)" "$cargo_home"
  start=$SECONDS
  status=0
  timeout --signal=TERM --kill-after=15s 360 "${command[@]}" "${args[@]}" \
    > "$results/$mode-metadata.json" 2> "$results/$mode.log" || status=$?
  elapsed=$((SECONDS - start))
  archives=0
  if [[ -d "$cargo_home/registry/cache" ]]; then
    archives=$(find "$cargo_home/registry/cache" -name '*.crate' -type f | wc -l)
  fi
  printf '| %s | %s | %s | %s |\n' "$mode" "$status" "$elapsed" "$archives" >> "$results/summary.md"
  printf 'Finished %s: exit=%s seconds=%s crate_archives=%s\n' "$mode" "$status" "$elapsed" "$archives"
  tail -n 20 "$results/$mode.log"
  if [[ "$status" != 0 ]]; then failed=1; fi
done

cat "$results/summary.md"
cat "$results/summary.md" >> "$GITHUB_STEP_SUMMARY"
exit "$failed"
