#!/usr/bin/env bash
set -euo pipefail

# Fetch through secure-runner's guarded Cargo, then run nextest with Cargo
# offline and without Socket's per-command proxy. Runner hardening stays active.
mode="${1:?expected fetch or test}"
shift
real_cargo_native="$(rustup which cargo)"
real_cargo="$real_cargo_native"
if [[ "${RUNNER_OS:-}" == Windows ]]; then
  real_cargo="$(cygpath -u "$real_cargo")"
fi

case "$mode" in
  fetch)
    test "${TEMPO_SFW_BASH_READY:-}" = true
    # A restored registry cache would skip download screening. Keep this
    # experiment's dependency sources separate from rust-cache and tool installs.
    nextest_home="$(mktemp -d "$RUNNER_TEMP/nextest-cargo.XXXXXX")"
    if [[ "${RUNNER_OS:-}" == Windows ]]; then
      nextest_home="$(cygpath -w "$nextest_home")"
    fi
    export CARGO_HOME="$nextest_home"
    cargo fetch --locked
    "$real_cargo" fetch --frozen
    # Only make the cache available to tests after both checks succeed.
    printf 'NEXTEST_CARGO_HOME=%s\n' "$CARGO_HOME" >> "$GITHUB_ENV"
    ;;
  test)
    export CARGO_HOME="${NEXTEST_CARGO_HOME:?guarded fetch must succeed first}"
    export CARGO_NET_OFFLINE=true
    export CARGO="$real_cargo_native"
    # Select the real Cargo for nextest and any nested Cargo invocation. Disable
    # Bash's automatic shim refresh only within this offline test process tree.
    export PATH="${real_cargo%/*}:$PATH"
    export BASH_ENV=/dev/null
    hash -r
    test "$(type -P cargo)" -ef "$real_cargo"
    printf 'Running nextest with offline Cargo: %s\n' "$real_cargo"
    # --frozen controls Cargo, not build-script/test HTTP requests; those still
    # require the runner's network policy (e.g. Solidity compiler downloads).
    exec "$real_cargo" nextest run --workspace --frozen "$@"
    ;;
  *)
    printf 'Unknown nextest mode: %s\n' "$mode" >&2
    exit 2
    ;;
esac
