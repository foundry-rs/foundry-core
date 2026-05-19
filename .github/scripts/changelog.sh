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

# Incremental mode: if the CHANGELOG already has at least one release section,
# only prepend the new --unreleased section. This keeps historical sections
# frozen so we never re-process pre-migration commits (which can reference PRs
# in upstream repos like foundry-rs/foundry, foundry-rs/compilers, etc.).
#
# Full-regen mode: if no prior release exists in the file, generate the entire
# CHANGELOG. Run manually to bootstrap or rebuild from scratch.
if [ -f "$group_root/CHANGELOG.md" ] && grep -q '^## \[[0-9]' "$group_root/CHANGELOG.md"; then
    # Strip any stale `## [Unreleased]` section so it doesn't linger above the
    # new release entry that --prepend is about to insert.
    if grep -q '^## \[Unreleased\]' "$group_root/CHANGELOG.md"; then
        run_unless_dry_run awk '
            /^## \[Unreleased\]/ { skip = 1; next }
            skip && /^## \[/ { skip = 0 }
            !skip { print }
        ' "$group_root/CHANGELOG.md" > "$group_root/CHANGELOG.md.tmp" \
            && run_unless_dry_run mv "$group_root/CHANGELOG.md.tmp" "$group_root/CHANGELOG.md"
    fi

    run_unless_dry_run git cliff \
        --workdir "$root" \
        --config "$group_root/cliff.toml" \
        --unreleased \
        "${@}" \
        --prepend "$group_root/CHANGELOG.md"
else
    run_unless_dry_run git cliff \
        --workdir "$root" \
        --config "$group_root/cliff.toml" \
        "${@}" \
        --output "$group_root/CHANGELOG.md"
fi
