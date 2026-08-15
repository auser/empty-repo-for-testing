set shell := ["bash", "-cu"]

default: check

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --locked --workspace --all-targets

clippy:
    cargo clippy --locked --workspace --all-targets -- -D warnings

test:
    cargo test --locked --workspace --all-targets

ci: fmt-check check clippy test

run *args:
    cargo run -p pire-cli -- {{args}}

doctor:
    cargo run -p pire-cli -- doctor
