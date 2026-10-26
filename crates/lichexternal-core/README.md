# lichexternal-core

Core data structures, protocol models, UCI parsing, and configuration schemas for [Lichess External Engine](https://lichess.org/api/external-engine) bridges.

This crate forms the foundation of the [Lichexternal](https://github.com/Chexperts/Lichexternal) suite, providing pure, lightweight protocol implementations that can be used in servers, background daemons, command-line utilities, or embedded and WebAssembly contexts.

## Features

- **Lichess External Engine Models**: Strongly typed request and response schemas (`EngineProfile`, `AcquireResponse`, `WorkRequest`, `SearchLimit`, `UciVariant`) validated against the official Lichess OpenAPI specification.
- **Universal Chess Interface (UCI) Parser**: Robust, high-speed line parser for engine responses (`uciok`, `readyok`, `bestmove`, `info depth ...`).
- **Config Schema**: Canonical configuration model and filesystem discovery logic for external engine bridges.
- **WebAssembly & `no_std` Capable**: Designed to compile to `wasm32-unknown-unknown` with `--no-default-features` for web workers and browser extensions.

## Feature Flags

| Feature | Description | Default |
| :--- | :--- | :---: |
| `std` | Standard library integration, filesystem path resolution via `etcetera`, and TOML parsing. | Yes |
| `uci` | Universal Chess Interface parser backed by `memchr`. | Yes |
| `typescript` | Enables `ts-rs` derives for automated TypeScript type definitions export. | No |

## Usage Examples

### Parsing UCI Output

```rust
use lichexternal_core::uci::{parse_uci_line, UciOutputLine, UciScore};

let line = "info depth 24 seldepth 32 score cp 45 nodes 1820491 nps 2150000 pv e2e4 e7e5 g1f3";
match parse_uci_line(line) {
    UciOutputLine::Info(info) => {
        assert_eq!(info.depth, Some(24));
        assert_eq!(info.score, Some(UciScore::Cp(45)));
        println!("PV: {:?}", info.pv);
    }
    UciOutputLine::BestMove { bestmove, ponder } => {
        println!("Best move: {bestmove}, ponder: {ponder:?}");
    }
    _ => {}
}
```

### Deserializing Lichess Work Requests

```rust
use lichexternal_core::models::{AcquireResponse, UciVariant};

let json = r#"{
    "id": "dfWmlfdy0caStBY1",
    "work": {
        "sessionId": "1",
        "threads": 4,
        "hash": 2048,
        "infinite": true,
        "variant": "chess",
        "initialFen": "startpos",
        "moves": ["e2e4", "c7c5"]
    }
}"#;

let response: AcquireResponse = serde_json::from_str(json).unwrap();
assert_eq!(response.work.variant, UciVariant::Chess);
assert_eq!(response.work.threads, 4);
```

## License

Licensed under the GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later).
