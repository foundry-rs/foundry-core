#!/usr/bin/env bash
set -eo pipefail

# Called by cargo-release as a pre-release-hook.
# Environment variables set by cargo-release:
#   $DRY_RUN        - "true" if this is a dry run
#   $WORKSPACE_ROOT  - the workspace root directory
#   $CRATE_ROOT      - the crate being released

run_unless_dry_run() {
    if [ "$DRY_RUN" = "true" ]; then
        echo "skipping due to dry run: $*" >&2
    else
        "$@"
    fi
}

root=$WORKSPACE_ROOT
crate=$CRATE_ROOT

# Find the crate group root (where cliff.toml lives) by walking up from the crate.
group_root="$crate"
while [ "$group_root" != "$root" ]; do
    if [ -f "$group_root/cliff.toml" ]; then
        break
    fi
    group_root="$(dirname "$group_root")"
done

if [ ! -f "$group_root/cliff.toml" ]; then
    echo "error: no cliff.toml found for $crate" >&2
    exit 1
fi

run_unless_dry_run git cliff \
    --workdir "$root" \
    --config "$group_root/cliff.toml" \
    "${@}" \
    --output "$group_root/CHANGELOG.md"
