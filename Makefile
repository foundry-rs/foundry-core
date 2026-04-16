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
	cargo build --profile "$(PROFILE)"

##@ Test

.PHONY: test-unit
test-unit: ## Run unit tests.
	cargo nextest run -E 'kind(test)'

.PHONY: test-doc
test-doc: ## Run doc tests.
	cargo test --doc --workspace

.PHONY: test
test: ## Run all tests.
	$(MAKE) test-unit && \
	$(MAKE) test-doc

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

##@ Other

.PHONY: clean
clean: ## Clean the project.
	cargo clean

.PHONY: deny
deny: ## Perform a `cargo` deny check.
	cargo deny --all-features check all

.PHONY: pr
pr: ## Run all checks and tests.
	$(MAKE) deny && \
	$(MAKE) lint && \
	$(MAKE) test
