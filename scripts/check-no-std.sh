#!/usr/bin/env bash
# Prove the core builds without std: compile the library with
# default features off for every installed bare-metal target
# (falling back to the host if none is installed — that still
# catches std:: leakage in crate source, just not std-linking
# via dependencies).
set -euo pipefail

cd "$(dirname "$0")/.."

targets=$(rustup target list --installed |
    grep -E -- '-none(-|$)' || true)

if [ -z "$targets" ]; then
    echo "check-no-std: no bare-metal (*-none-*) target" \
        "installed; falling back to host --no-default-features."
    echo "check-no-std: install one with e.g.:" \
        "rustup target add thumbv7em-none-eabihf"
    cargo build --lib --no-default-features
    exit 0
fi

for t in $targets; do
    echo "check-no-std: cargo build --lib" \
        "--no-default-features --target $t"
    cargo build --lib --no-default-features --target "$t"
done
echo "check-no-std: OK"
