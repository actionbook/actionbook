.PHONY: check build test lint e2e coverage

check:
	cargo check --all-targets

build:
	cargo build --release

test:
	cargo test --lib

lint:
	cargo clippy --all-targets -- -D warnings -A dead_code -A unused-imports -A deprecated
	cargo fmt --check

e2e:
	cargo test --test '*' -- --test-threads=1

coverage:
	mkdir -p target/llvm-cov
	bash -c 'set -o pipefail; cargo llvm-cov nextest --lib --test cli_test --test cdp_message_parsing --profile ci --no-fail-fast 2>&1 | tee target/llvm-cov/coverage-summary.txt'
	cargo llvm-cov report --lcov --output-path target/llvm-cov/lcov.info
	@echo "Coverage summary: target/llvm-cov/coverage-summary.txt"
	@echo "LCOV output: target/llvm-cov/lcov.info"
