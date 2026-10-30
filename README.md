# vbitmap

Fast Rust bitmaps with growable and fixed-size storage, zero-copy views,
SIMD acceleration and `no_std` support.

[API documentation](https://docs.rs/vbitmap) · [crates.io](https://crates.io/crates/vbitmap)

```rust
use vbitmap::NativeBitmap;

let mut bits = NativeBitmap::new();
bits.set_many([1, 3, 64]);

assert!(bits.get(64));
assert_eq!(bits.iter_ones().collect::<Vec<_>>(), [1, 3, 64]);
```

- [Runnable examples](examples/)
- [Benchmarks and library comparisons](BENCHMARKS.md)
- [Development and release setup](docs/development.md)

Licensed under [MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE).
