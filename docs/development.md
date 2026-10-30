# Development and releases

Run these commands from the repository root.

## Local checks and benchmarks

```sh
just check                        # all quality gates, including coverage and audit
just test                         # tests and doctests for every feature combination
just test-native                  # optimized native CPU tests
just bench                        # Criterion benchmarks
just compare                      # vs fixedbitset, bit-vec, bit-set, bitvec, bitmaps, bv
RUSTFLAGS="-C target-cpu=native" just compare
```

## Releases

Pull requests run `.github/workflows/ci.yml`. Pushes to `main` run the same CI
through `.github/workflows/release-plz.yml`; publishing waits for all checks to
pass. CI checks formatting, Clippy, docs and packaging, uses `cargo hack` to test
every feature combination on Linux, macOS and Windows, checks Rust 1.85 and a
bare-metal target, and runs optimized native CPU tests on Linux and macOS.
All CI checks call recipes in `justfile`, so the same commands run locally.
Install `just` and `cargo-hack` to run the feature checks; `just check` also
requires `cargo-llvm-cov` and `cargo-audit` for its coverage and audit recipes.

After the first manual `cargo publish`, configure the following once:

1. In GitHub **Settings → Actions → General**, enable **Allow GitHub Actions to
   create and approve pull requests**.
2. In the crate's crates.io settings, add a Trusted Publisher for owner
   `msardara`, repository `vbitmap`, workflow `release-plz.yml`, with no
   environment restriction. The publish job uses OIDC; a
   `CARGO_REGISTRY_TOKEN` secret is unnecessary.
3. For CI to start automatically on release PRs, add a repository secret
   `RELEASE_PLZ_TOKEN` containing a fine-grained GitHub token scoped to this
   repository with **Contents: read/write** and **Pull requests: read/write**.
   Without it, release-plz uses `GITHUB_TOKEN`, which cannot trigger PR checks;
   close and reopen the release PR to trigger them. Main-branch CI still runs
   before publishing in either case.

release-plz opens a PR containing the next version and changelog. Review and
merge that PR to publish the crate and create its GitHub tag and release.
Ordinary pushes to `main` prepare release PRs without publishing new versions.

See the [release-plz setup guide](https://release-plz.dev/docs/github/quickstart)
and [GitHub token guidance](https://release-plz.dev/docs/github/token).

