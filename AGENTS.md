# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project Overview

foundry-core is a Rust workspace of standalone compiler, block explorer,
fork database, and wallet libraries.

## Commands

```bash
cargo build --workspace --locked
cargo fmt --all
cargo clippy --workspace --all-targets --all-features --locked
cargo nextest run --workspace --locked
cargo test --workspace --doc --all-features --locked
```

## Architecture

- `crates/compilers/`: compiler abstraction, artifacts, and compiler utilities.
- `crates/block-explorers/`: Etherscan and other block explorer API clients.
- `crates/fork-db/`: fork database and remote state access.
- `crates/wallets/`: wallet management and signing.

## Testing

- Add regression coverage in the affected crate's existing tests.
- Compiler integration tests live in `crates/compilers/crates/compilers/tests/`;
  fixtures live in `crates/compilers/test-data/`.
- Reuse existing test helpers and snapshot assertions.
- Run the affected package's tests first; use the workspace suite for broader changes.

## Configuration

Keep compiler settings and artifact serialization compatible with the supported
compiler formats. Cover changed defaults and serialization in existing tests.

## Commit and PR Style

Use conventional commits and PR titles: `type: description`, with an optional
scope. Keep subjects under 50 characters where practical. Explain what changed
and why in a short PR description; omit templates and validation boilerplate.
Disclose AI assistance and its scope, as required by `CONTRIBUTING.md`.

### Performance PRs

Compare against `main` or the requested base using the same benchmark inputs and
build settings. Include only measured results from the affected path.

## Notes

- Follow the workspace MSRV in `Cargo.toml`.
- Generate release notes from pull requests; do not add changelog fragments.

## Code Style

- Follow existing patterns.
- Comments end with periods, except URLs.
- Files use LF and end with a newline.
- Never expose secrets.

### Rust

- Generally add new Rust functions, methods, `impl` blocks, modules, imports, Cargo dependencies, and other items at the bottom of the relevant scope, section, or group. Constructors usually go at the top of an `impl` block. First check where similar items sit in the file and match the existing order, grouping, and style, including alphabetical order where used.
- Put doc comments before attributes, always: `/// ...` comes before `#[derive]`, `#[inline]`, `#[cfg]`, and every other attribute.
- Put module documentation at the top of the module file with inner doc comments (`//! ...`), not on the `mod` item in the parent module.
- NEVER put imports inside functions unless required for `#[cfg(...)]` gating. All imports go at the top of the file.
- Group all `use` imports together. Keep `pub use` imports in a separate group. For local module re-exports, write `mod x;` before `pub use x;`; for re-exporting another module or external crate, use `use x;`, then a blank line, then `pub use y;`, then a blank line before local `mod my_mod; pub use my_mod::*;`.
- Put imports used only by tests inside the relevant `#[cfg(test)]` module, merging them into its ordinary imports instead of adding `#[cfg(test)] use` items to the parent. Keep any additional feature or platform conditions on those imports. Retain parent-level test-gated imports or re-exports only when test-only helpers or multiple test modules need them there.
- Keep crate-level dependency anchors such as `#[cfg(test)] use cc as _;` at crate scope.
- Put conditional imports after unconditional imports, separated by a blank line. Group imports with the same `#[cfg(...)]` condition together, with a blank line between different conditions. Apply this to every feature, platform, and test gate, including imports in nested modules. Within each group, merge imports from the same crate when their conditions and other attributes match. Keep the full condition on each `use`; do not introduce import-only modules or macros to avoid repeated attributes. Apply the same ordering within the separate `pub use` group, keeping local re-exports after their module declarations.
- In test modules, always import the parent module with `use super::*`.
- In `Cargo.toml`, generally group optional dependencies for a feature together. Put a comment immediately above the group containing only the feature name, for example `# jit`.
- Prefer `let Some(x) = x else { return };` / `let Ok(x) = x else { return };` over `match x { Some(x) => x, _ => return }`.
- Use `let ... else` only for a single early-exit guard. When multiple conditions or patterns gate the same block, prefer a combined `if let` / `let` chain instead of several sequential `let ... else` statements.
- Use combined `if let` chains (`if let Some(x) = x && let Some(y) = y { ... }`) instead of nesting (`if let Some(x) = x { if let Some(y) = y { ... } }`).
- In loops, prefer an `if let` chain around the loop body over multiple `let ... else { continue };` statements when the body only runs if all patterns match.
- NEVER use `ref` / `ref mut` in patterns as the first resort. Always prefer borrowing the expression with `&` / `&mut` instead.
- Prefer map entry APIs such as `entry`, `or_insert`, and `or_insert_with` when multiple consecutive operations would otherwise look up and then insert or update the same key.
- Avoid specifying type hints in variables unless absolutely necessary (e.g. `HashMap<_, Vec<_>>` for `x.entry(y).or_default().push(z)` where type inference won't work). Rely on the compiler.
- When type hints are needed, prefer turbofish (`let x = Type::<X, Y>::new()`) over annotation (`let x: Type<X, Y> = Type::new()`).
- In tests, avoid `.contains` assertions for error/output strings when the project has snapshot testing support such as `snapbox`. Prefer exact snapshot assertions (`stderr_eq`, `stdout_eq`, `assert_data_eq!`, etc.) and use redactions only for genuinely variable parts.
- Always leave a blank line in between module doc-comments, items or item categories, unless in rare exceptions: it's a one-shot struct with one single impl block, or it's a list of impls that are all very similar. But in general blank line in between items is the norm but it's just unenforced. Items includes imports (together) too. The previous rules apply first.
