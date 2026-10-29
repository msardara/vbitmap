// SPDX-License-Identifier: MIT OR Apache-2.0
// `has_alloc` is set when `Bitmap`, formatting and parsing are available: always
// in the default (std) build, and with the `alloc` feature in a `no_std` build.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_alloc)");
    println!("cargo:rerun-if-changed=build.rs");
    let no_std = std::env::var_os("CARGO_FEATURE_NO_STD").is_some();
    let alloc = std::env::var_os("CARGO_FEATURE_ALLOC").is_some();
    if !no_std || alloc {
        println!("cargo:rustc-cfg=has_alloc");
    }
}
