# CI dependency screening

Rust build and browser-wallet jobs start with the pinned `secure-runner` action and
use Bash. Its startup hook refreshes package-manager interception after toolchain
setup and cache restoration; commands do not need an explicit `sfw` prefix.

## Installation paths

- Feature checks: `cargo install --locked --version 0.6.45 cargo-hack`.
- Regular and nightly tests: `cargo install --locked --version 0.9.143 cargo-nextest`.
- Dependency audit: `cargo install --locked --version 0.20.2 cargo-deny`, followed
  by the existing all-features, locked audit on the host instead of in a container.
- Browser validation and bundle build: `npm install --global pnpm@11.9.0`, then
  `pnpm install --frozen-lockfile` after Node's cache setup.
- Workspace dependency fetches occur through wrapped Cargo commands. Nextest
  retains the guard against Socket reporting an internal error with exit code zero.

The standalone Cargo cooldown workflow and `cooldown.toml` are removed. Crate-age
thresholds and exceptions must be enforced by the active Socket organization
policy; successful installs alone do not verify equivalent cooldown enforcement.
The nightly toolchain setup remains responsible for selecting nightly for fmt
and Clippy, without redundant command-line toolchain selectors.

## Compatibility and limits

sccache starts outside wrapped Cargo so its daemon retains its own proxy
environment. The shared action supplies the Cargo Git CLI default. Jobs retain
explicit Node connection, unknown-host, compiler/explorer endpoint, and Windows
revocation settings. On macOS/Windows the raw GitHub exception covers the entire
host; certificate validation remains enabled.

This is Bash command routing, not runner-wide interception. Rust/Node toolchain
downloads, sccache and typos release binaries, CodeQL tooling, and the separate
workflow-validation workflow are not screened by these Cargo/npm wrappers. Their
existing installation/integrity
mechanisms remain unchanged. Restored dependencies are not rechecked by network
interception, and actions using other shells or direct executable paths may not
use the hook. Fork PRs without OIDC use Socket Free, without organization policy.

The normal PR matrix, nightly trigger, and guarded bundle-commit job are retained.
Full-main-matrix validation and live cooldown-denial checks are deferred to a
separate, independently revertible commit after regular draft-PR CI is working.
