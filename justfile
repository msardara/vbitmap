# SPDX-License-Identifier: MIT OR Apache-2.0
# Quality gates shared by local development and CI. Requires cargo-hack.
# `just` lists the recipes, `just check` runs them all.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Bare-metal target used to prove the crate really is no_std.
no_std_target := "thumbv7em-none-eabihf"

default:
    @just --list

# All CI recipes, plus local coverage and dependency auditing.
[doc("All quality gates, including coverage and dependency auditing")]
check: fmt clippy test doc-check examples no-std msrv test-native package coverage audit

# Formatting must be clean (use `cargo fmt` to fix).
[doc("Check formatting (cargo fmt fixes it)")]
fmt:
    cargo fmt --all -- --check

# Clippy with warnings denied. Check the complete feature powerset on the
# library and tests, then benchmarks and examples in their supported modes.
[doc("Clippy with warnings denied, for every feature combination")]
clippy:
    cargo hack clippy --feature-powerset --lib --tests -- -D warnings
    cargo clippy --all-targets --features serde -- -D warnings
    cargo clippy --example no_std --features no_std -- -D warnings

# Tests for every feature combination.
[doc("Tests and doc tests for every feature combination")]
test:
    # Selecting lib/tests avoids building the allocator-free example with alloc.
    cargo hack test --feature-powerset --lib --tests
    cargo hack test --feature-powerset --doc

[doc("Check every feature combination on the Rust version declared in Cargo.toml")]
msrv:
    cargo hack check --rust-version --feature-powerset --no-dev-deps --lib

[doc("Test native CPU paths in release mode, with default and all features")]
test-native:
    RUSTFLAGS="-C target-cpu=native" cargo test --release --lib --tests
    RUSTFLAGS="-C target-cpu=native" cargo test --release --lib --tests --all-features

[doc("Verify the crate package; optional arguments are passed to cargo package")]
package *args:
    cargo package {{args}}

# Build the API documentation and open it in the browser (target/doc). Warnings
# (broken links, bad code blocks) are errors.
[doc("Build the API documentation and open it in the browser")]
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --open

# The documentation must build without warnings for every feature combination;
# its examples run as doc tests under `just test`.
[doc("Documentation must build without warnings, for every feature combination")]
doc-check:
    RUSTDOCFLAGS="-D warnings" cargo hack doc --feature-powerset --no-deps

# Run every example; each one asserts what it prints. The last one is a real
# `#![no_std]` binary.
[doc("Run every example (the last is a real #![no_std] binary)")]
examples:
    cargo run -q --example basic
    cargo run -q --example search
    cargo run -q --example logic
    cargo run -q --example fields
    cargo run -q --example text
    cargo run -q --example words
    cargo run -q --example no_std --features no_std

# The crate must build for a target that has no std at all.
[doc("Build for a target with no std at all")]
no-std:
    rustup target add {{ no_std_target }}
    cargo build --release --lib --target {{ no_std_target }} --no-default-features --features no_std
    cargo hack build --release --lib --target {{ no_std_target }} --feature-powerset --include-features alloc,serde --features no_std

#
# llvm-cov's own line summary counts generic code once per word type, so
# branches that are decided at compile time by `W::BITS` look "missed" in the
# instantiations that do not take them. The lcov data merges instantiations
# per source line, which is what we check here.
# Test coverage must be 100%: every function, and every source line. Needs
# cargo-llvm-cov (`cargo install cargo-llvm-cov`).
[doc("Test coverage must be 100%: every function and every source line")]
coverage:
    cargo llvm-cov --fail-under-functions 100 --lcov --output-path target/lcov.info
    cargo llvm-cov report --summary-only
    awk -F'[:,]' '/^DA:/ { total++; if ($3 == 0) { missed++; print "uncovered: " file ":" $2 } } /^SF:/ { file = substr($0, 4) } END { printf "source lines: %d, uncovered: %d\n", total, missed; exit (missed > 0) }' target/lcov.info

# Known vulnerabilities in the dependency tree, including optional and dev dependencies.
[doc("Known vulnerabilities in the dependency tree (cargo-audit)")]
audit:
    test -f Cargo.lock || cargo generate-lockfile
    cargo audit

# Benchmarks (Criterion). Build with RUSTFLAGS="-C target-cpu=native" for the
# numbers in the README.
[doc("Criterion benchmarks of vbitmap itself")]
bench:
    cargo bench

# Compare against the most used bitmap and bitset crates (benches/compare.rs,
# Criterion). Runs three times with different heap layouts, merges the runs
# (minimum per cell) and prints the scoreboard; the markdown tables are written
# to target/compare/report.md. Pin to one core for stable numbers, e.g. with
# `taskset -c 3 just compare` on Linux.
[doc("Compare against the most used bitmap and bitset crates")]
compare:
    mkdir -p target/compare
    for s in 0 1344 2752; do LAYOUT_SHIFT=$s cargo bench --bench compare -- --noplot; python3 scripts/bench_report.py collect target/compare/run-$s.json; done
    python3 scripts/bench_report.py report --out target/compare/report.md "this machine=target/compare/run-0.json,target/compare/run-1344.json,target/compare/run-2752.json"
