# Heavily inspired by:
# - Lighthouse: https://github.com/sigp/lighthouse/blob/693886b94176faa4cb450f024696cb69cda2fe58/Makefile
# - Reth: https://github.com/paradigmxyz/reth/blob/1f642353ca083b374851ab355b5d80207b36445c/Makefile
.DEFAULT_GOAL := help

# Cargo profile for builds.
PROFILE ?= dev

##@ Help

.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

##@ Build

.PHONY: build
build: ## Build the project.
	cargo build --locked --profile "$(PROFILE)"

##@ Test

.PHONY: test-unit
test-unit: ## Run unit tests.
	cargo nextest run --workspace --locked

.PHONY: test-doc
test-doc: ## Run doc tests.
	cargo test --doc --workspace --locked

.PHONY: test
test: ## Run all tests.
	$(MAKE) test-unit && \
	$(MAKE) test-doc

.PHONY: test-coverage
test-coverage: ## Run unit and doc tests with coverage and open the report.
	cargo +nightly llvm-cov --no-report nextest --locked --workspace && \
	cargo +nightly llvm-cov --no-report --doc --locked && \
	cargo +nightly llvm-cov report --doctests --open

##@ Linting

.PHONY: fmt
fmt: ## Run all formatters.
	cargo +nightly fmt

.PHONY: lint-clippy
lint-clippy: ## Run clippy on the codebase.
	cargo +nightly clippy \
	--workspace \
	--all-targets \
	--all-features \
	--locked \
	-- -D warnings

.PHONY: lint-clippy-fix
lint-clippy-fix: ## Run clippy on the codebase and fix warnings.
	cargo +nightly clippy \
	--workspace \
	--all-targets \
	--all-features \
	--fix \
	--allow-dirty \
	--allow-staged \
	--locked \
	-- -D warnings

.PHONY: lint-typos
lint-typos: ## Run typos on the codebase.
	@command -v typos >/dev/null || { \
		echo "typos not found. Please install it by running the command `cargo install typos-cli` or refer to the following link for more information: https://github.com/crate-ci/typos"; \
		exit 1; \
	}
	typos

.PHONY: lint
lint: ## Run all linters.
	$(MAKE) fmt && \
	$(MAKE) lint-clippy && \
	$(MAKE) lint-typos

##@ Documentation

.PHONY: doc
doc: ## Build the documentation.
	RUSTDOCFLAGS="--cfg docsrs -D warnings -Zunstable-options --show-type-layout --generate-link-to-definition" \
		cargo +nightly doc \
		--workspace \
		--all-features \
		--document-private-items \
		--no-deps \
		--locked

##@ Release
# Prepare a release: update changelog, run semver-checks, bump versions.
# Open a PR with the changes, merge, then push the tag to trigger CI publish.
#
# Usage:
#   make release-compilers version=X.Y.Z
#   # open PR, merge, then:
#   git tag compilers-vX.Y.Z && git push origin compilers-vX.Y.Z

.PHONY: release-compilers
release-compilers: ## Prepare compilers release (version=X.Y.Z).
	@test -n "$(version)" || (echo "usage: make release-compilers version=X.Y.Z" && exit 1)
	cd crates/compilers && git cliff --unreleased --tag "compilers-v$(version)" --prepend CHANGELOG.md
	cargo +stable semver-checks \
	-p foundry-compilers \
	-p foundry-compilers-core \
	-p foundry-compilers-artifacts \
	-p foundry-compilers-artifacts-solc \
	-p foundry-compilers-artifacts-vyper
	cargo release $(version) \
	-p foundry-compilers \
	-p foundry-compilers-core \
	-p foundry-compilers-artifacts \
	-p foundry-compilers-artifacts-solc \
	-p foundry-compilers-artifacts-vyper \
	--no-publish \
	--no-tag \
	--no-push \
	--execute --no-confirm

.PHONY: release-explorers
release-explorers: ## Prepare explorers release (version=X.Y.Z).
	@test -n "$(version)" || (echo "usage: make release-explorers version=X.Y.Z" && exit 1)
	cd crates/explorers && git cliff --unreleased --tag "explorers-v$(version)" --prepend CHANGELOG.md
	cargo +stable semver-checks -p foundry-block-explorers -p foundry-blob-explorers
	cargo release $(version) \
	-p foundry-block-explorers \
	-p foundry-blob-explorers \
	--no-publish \
	--no-tag \
	--no-push \
	--execute --no-confirm

.PHONY: release-fork-db
release-fork-db: ## Prepare fork-db release (version=X.Y.Z).
	@test -n "$(version)" || (echo "usage: make release-fork-db version=X.Y.Z" && exit 1)
	cd crates/fork-db && git cliff --unreleased --tag "fork-db-v$(version)" --prepend CHANGELOG.md
	cargo +stable semver-checks -p foundry-fork-db
	cargo release $(version) \
	-p foundry-fork-db \
	--no-publish \
	--no-tag \
	--no-push \
	--execute --no-confirm

.PHONY: release-wallets
release-wallets: ## Prepare wallets release (version=X.Y.Z).
	@test -n "$(version)" || (echo "usage: make release-wallets version=X.Y.Z" && exit 1)
	cd crates/wallets && git cliff --unreleased --tag "wallets-v$(version)" --prepend CHANGELOG.md
	cargo +stable semver-checks -p foundry-wallets
	cargo release $(version) \
	-p foundry-wallets \
	--no-publish \
	--no-tag \
	--no-push \
	--execute --no-confirm

##@ Other

.PHONY: lock
lock: ## Update the Cargo.lock file with the current dependencies.
	cargo fetch

.PHONY: clean
clean: ## Clean the project.
	cargo clean

.PHONY: deny
deny: ## Perform a `cargo` deny check.
	cargo deny --locked --all-features check all

.PHONY: check
check: ## Run a feature check on all crates and binaries.
	cargo hack check --locked --feature-powerset --depth 1

.PHONY: shear
shear: ## Run `cargo shear` to check for unused dependencies.
	cargo shear --locked

.PHONY: pr
pr: ## Run all checks and tests.
	$(MAKE) deny && \
	$(MAKE) lint && \
	$(MAKE) test && \
	$(MAKE) doc