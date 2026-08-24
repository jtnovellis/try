# Makefile for try - Fresh directories for every vibe

SHELL := /bin/bash
TRY := target/release/try

.PHONY: help
help: ## Show this help message
	@echo "try - Fresh directories for every vibe"
	@echo ""
	@echo "Available targets:"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

.PHONY: build
build: ## Build release binary
	@echo "Building release binary..."
	cargo build --release
	@echo "Binary at $(TRY)"

.PHONY: test
test: build ## Run all spec compliance tests
	@echo "Running tests..."
	bash spec/tests/runner.sh $(TRY)

.PHONY: lint
lint: ## Check formatting and run clippy (same as CI)
	cargo fmt --check
	cargo clippy --release --all-targets -- -D warnings

.PHONY: fmt
fmt: ## Format the source tree
	cargo fmt

.PHONY: run
run: build ## Run try (e.g. make run ARGS="--help")
	$(TRY) $(ARGS)

.PHONY: install
install: build ## Install try binary to ~/.local/bin
	@echo "Installing to ~/.local/bin..."
	@mkdir -p ~/.local/bin
	@cp $(TRY) ~/.local/bin/try
	@chmod +x ~/.local/bin/try
	@echo "Installed! Add to your shell:"
	@echo '  eval "$$(~/.local/bin/try init ~/src/tries)"'

.PHONY: clean
clean: ## Clean build artifacts
	@echo "Cleaning..."
	cargo clean
	@echo "Clean complete"

.PHONY: all
all: lint build test ## Lint, build and test

# Shortcuts
.PHONY: t
t: test ## Shortcut for test

.PHONY: b
b: build ## Shortcut for build

.PHONY: i
i: install ## Shortcut for install
