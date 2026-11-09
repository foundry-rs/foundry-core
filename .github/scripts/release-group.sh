#!/usr/bin/env bash
set -euo pipefail

# Maps a release tag such as `compilers-v0.22.0` to its crates and checks that
# every crate's manifest version matches the tag. Prints `packages=...` for
# `$GITHUB_OUTPUT`.

TAG="${1:?usage: release-group.sh <tag>}"
GROUP="${TAG%-v*}"
VERSION="${TAG##*-v}"

case "$GROUP" in
    compilers)
        PACKAGES="foundry-compilers-core foundry-compilers-artifacts-solc foundry-compilers-artifacts-vyper foundry-compilers-artifacts foundry-compilers"
        ;;
    block-explorers)
        PACKAGES="foundry-block-explorers"
        ;;
    fork-db)
        PACKAGES="foundry-fork-db"
        ;;
    wallets)
        PACKAGES="foundry-wallets"
        ;;
    *)
        echo "error: unknown crate group in tag '${TAG}'" >&2
        exit 1
        ;;
esac

metadata="$(cargo metadata --format-version 1 --no-deps --locked)"
for package in $PACKAGES; do
    actual="$(jq -r --arg name "$package" '.packages[] | select(.name == $name) | .version' <<<"$metadata")"
    if [[ "$actual" != "$VERSION" ]]; then
        echo "error: ${package} is ${actual:-missing}, but tag ${TAG} releases ${VERSION}" >&2
        exit 1
    fi
done

echo "packages=${PACKAGES}"
