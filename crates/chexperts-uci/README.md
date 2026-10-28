# chexperts-uci

[![crates.io](https://img.shields.io/crates/v/chexperts-uci.svg)](https://crates.io/crates/chexperts-uci)
[![docs.rs](https://docs.rs/chexperts-uci/badge.svg)](https://docs.rs/chexperts-uci)
[![license](https://img.shields.io/crates/l/chexperts-uci.svg)](https://github.com/Chexperts/Lichexternal)

Fast, zero-copy, 100% specification-compliant parser and serializer for the **Universal Chess Interface (UCI)** protocol in Rust.

Part of the **Chexperts** chess toolkit.

## Features

- **100% UCI Specification (2000) Compliant**: Supports all GUI-to-Engine commands (`uci`, `debug`, `isready`, `setoption`, `register`, `ucinewgame`, `position`, `go`, `stop`, `ponderhit`, `quit`) and Engine-to-GUI responses (`id`, `uciok`, `readyok`, `bestmove`, `copyprotection`, `registration`, `info`, `option`).
- **Zero-Copy Parsing**: Extracts string slices (`&'a str`) directly from line buffers with zero memory allocations on hot evaluation paths.
- **`#![no_std]` Compatible**: Designed for constrained targets, WebAssembly, and embedded systems (requires `alloc`).
- **Fast SIMD-accelerated Tokenization**: Optional acceleration with `memchr`.
