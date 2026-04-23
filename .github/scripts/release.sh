#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<EOF
Usage: ./release.sh <crate-group> <version> [--execute]

Crate groups: compilers, explorers, fork-db, wallets

Without --execute, runs in dry-run mode (no publish, no tag, no push).
With --execute, publishes to crates.io, tags, pushes, and creates a GitHub release.

Requires: cargo login, gh auth login

Examples:
  ./release.sh wallets 0.2.0             # dry-run
  ./release.sh wallets 0.2.0 --execute   # real release
EOF
    exit 1
}

confirm() {
    echo ""
    read -rp "Continue? [y/N] " answer
    [[ "$answer" == "y" || "$answer" == "Y" ]] || { echo "Aborted."; exit 1; }
    echo ""
}

[[ $# -lt 2 ]] && usage

GROUP="$1"
VERSION="$2"
EXECUTE=false
[[ "${3:-}" == "--execute" ]] && EXECUTE=true

# Crate group definitions: tag-prefix, changelog dir, and packages.
case "$GROUP" in
    compilers)
        TAG="compilers-v${VERSION}"
        CLIFF_DIR="crates/compilers"
        PACKAGES=(
            foundry-compilers-core
            foundry-compilers-artifacts-solc
            foundry-compilers-artifacts-vyper
            foundry-compilers-artifacts
            foundry-compilers
        )
        ;;
    explorers)
        TAG="explorers-v${VERSION}"
        CLIFF_DIR="crates/explorers"
        PACKAGES=(
            foundry-block-explorers
            foundry-blob-explorers
        )
        ;;
    fork-db)
        TAG="fork-db-v${VERSION}"
        CLIFF_DIR="crates/fork-db"
        PACKAGES=(
            foundry-fork-db
        )
        ;;
    wallets)
        TAG="wallets-v${VERSION}"
        CLIFF_DIR="crates/wallets"
        PACKAGES=(
            foundry-wallets
        )
        ;;
    *)
        echo "error: unknown crate group '${GROUP}'"
        usage
        ;;
esac

# Build -p flags.
PKG_FLAGS=()
for pkg in "${PACKAGES[@]}"; do
    PKG_FLAGS+=(-p "$pkg")
done

echo "==> Generating changelog preview"
(cd "$CLIFF_DIR" && git cliff --unreleased --tag "$TAG")

echo ""
echo "--- Changelog preview ---"
echo ""
(cd "$CLIFF_DIR" && git cliff --unreleased --tag "$TAG")
echo ""
echo "---"
echo ""
echo "This will be prepended to ${CLIFF_DIR}/CHANGELOG.md"
confirm

echo "==> Writing changelog"
(cd "$CLIFF_DIR" && git cliff --unreleased --tag "$TAG" --prepend CHANGELOG.md)

echo "==> Next: semver-checks"
confirm

cargo +stable semver-checks "${PKG_FLAGS[@]}"

if $EXECUTE; then
    echo "==> Next: publish to crates.io and tag ${TAG}"
    confirm

    cargo release "$VERSION" "${PKG_FLAGS[@]}" --no-push --execute --no-confirm

    echo "==> Next: push branch and tag to origin"
    confirm

    git push origin HEAD "$TAG"

    echo "==> Next: create GitHub release for ${TAG}"
    confirm

    gh release create "$TAG" --generate-notes
else
    echo "==> Next: cargo release dry-run"
    confirm

    cargo release "$VERSION" "${PKG_FLAGS[@]}" --no-publish --no-tag
fi

echo "==> Done"
