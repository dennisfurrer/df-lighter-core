check: fmt clippy wasm test web features doc

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets --features web -- -D warnings

wasm:
    cargo clippy -p df-lighter-core --no-default-features --features sign,web --target wasm32-unknown-unknown -- -D warnings

# Unit, conformance and mocked suites; the live suite is skipped here.
test:
    cargo test --workspace
    cargo test -p df-lighter-core --features offline --lib -- offline what_is_signed

# Against both venues, read only: every public read, websocket channel and L1 read, each held
# against its recording. Run by hand.
live:
    cargo test -p df-lighter-core --test live -- --ignored --test-threads 1

# Line coverage of the unit, conformance and mocked suites.
cover:
    cargo llvm-cov -p df-lighter-core --summary-only

# The browser transport, run as wasm in Node through wasm-bindgen's test runner.
web:
    cargo test -p df-lighter-core --no-default-features --features sign,web --target wasm32-unknown-unknown --lib

# Without `sign`, nothing pulls in the curve.
features:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo clippy -p df-lighter-core --no-default-features -- -D warnings
    cargo clippy -p df-lighter-core --no-default-features --target wasm32-unknown-unknown -- -D warnings
    if cargo tree -p df-lighter-core --no-default-features -e no-dev --target all | grep -q ' k256 v'; then
        echo "df-lighter-core pulls in k256 without sign" >&2
        exit 1
    fi

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
