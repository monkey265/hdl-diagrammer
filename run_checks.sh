#!/bin/bash
set -e
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
echo "All checks passed"
