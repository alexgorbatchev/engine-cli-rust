mod engine_library 'crates/engine-library/justfile'

default:
    @just --list

build:
    cargo build --package engine-cli --release --bin engine-cli
    mkdir -p bin
    cp target/release/engine-cli bin/engine-cli

[private]
_test:
    cargo test --package engine-cli --all-targets

[private]
_lint:
    cargo clippy --package engine-cli --all-targets -- -D warnings

[private]
_fmt:
    cargo fmt --package engine-cli

[private]
_fmt-check:
    cargo fmt --package engine-cli -- --check

coverage:
    mkdir -p .tmp
    cargo llvm-cov --package engine-cli --exclude-from-report engine-library --all-targets --fail-under-lines 90 --json --output-path .tmp/coverage-final.json

check: _fmt-check _lint _test coverage
    just engine_library check

run *args:
    cargo run --package engine-cli --bin engine-cli -- {{ args }}

run-ai *args:
    AGENT=1 cargo run --package engine-cli --bin engine-cli -- {{ args }}

test: _test
    just engine_library test

lint: _lint
    just engine_library lint

fmt: _fmt
    just engine_library fmt

fmt-check: _fmt-check
    just engine_library fmt-check
