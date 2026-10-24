# Lichexternal ♟️🌐

High-performance, cross-platform asynchronous service bridging the [Lichess External Engine API](https://lichess.org/api#tag/External-engine) (`engine.lichess.ovh`) with local and remote chess engines.

Part of the **Chexperts** open-source chess ecosystem.

---

## Highlights

- **100% Generic & Unopinionated**: Zero hardcoded engine logic. Seamlessly drives standard executables (**Stockfish**, **Maia 3**, **Lc0**) via subprocess standard I/O (`stdio`) or persistent local daemons (**Chexpectimax**) via local sockets (`socket`).
- **Zero-Copy SIMD Parser**: Uses `memchr` to parse engine calculation streams (`info depth ... multipv ... score cp/mate ... pv ...`) and `bestmove` tokens without heap allocations.
- **Instant Stream Cancellation**: Detects when a Lichess user cancels analysis or closes the browser tab, immediately signals `stop\n` to the engine, drains the trailing `bestmove`, and resets to polling without killing or restarting the engine process.
- **Cross-Platform & WASM-Ready**:
  - `lichexternal-core` has zero OS/network dependencies, compiling to `wasm32-unknown-unknown`, Windows, Linux, macOS, and Android.
  - `lichexternal-daemon` runs natively on Windows (Named Pipes `\\.\pipe\lichexternald` and `%LOCALAPPDATA%`), Linux/Android (Unix Domain Sockets and `$XDG_CONFIG_HOME`), and macOS.
- **Modern Nix Flake**: RFC 140 compliant (`hostSystem`), offering pre-configured NixOS and Home Manager system modules.

---

## Architecture Overview

```
                      ┌─────────────────────────────────────────┐
                      │               Lichess WAN               │
                      │  https://engine.lichess.ovh (Broker)    │
                      │  https://lichess.org/api (REST API)     │
                      └────────────────────▲────────────────────┘
                                           │
                        HTTPS Long-Poll    │  Chunked UCI Stream
                       & Work Assignment   │  POST /work/{id}
                                           ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                      Lichexternal Daemon (lichexternald)                        │
│                                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │                         Supervisor Pool                                 │   │
│   │                                                                         │   │
│   │   ┌───────────────────────────┐     ┌───────────────────────────────┐   │   │
│   │   │    Stdio Engine Session   │     │      Socket Engine Session    │   │   │
│   │   │     (e.g. Stockfish)      │     │      (e.g. Chexpectimax)      │   │   │
│   │   └─────────────┬─────────────┘     └───────────────┬───────────────┘   │   │
│   └─────────────────┼───────────────────────────────────┼───────────────────┘   │
│                     │ stdin/stdout                      │ IPC Socket / Pipe     │
│                     ▼                                   ▼                       │
│             /usr/bin/stockfish                  /run/user/chexpectimax.sock     │
│                                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │               Local Control IPC Server (interprocess 2.x)               │   │
│   │             \.\pipe\lichexternald  or  /tmp/lichexternald.sock          │   │
│   └─────────────────────────────────────▲───────────────────────────────────┘   │
└─────────────────────────────────────────┼───────────────────────────────────────┘
                                          │ IPC (JSON-RPC)
                                          ▼
                      ┌─────────────────────────────────────────┐
                      │        CLI Client (lichexternal)        │
                      │   status | ping | test-engine | engine   │
                      └─────────────────────────────────────────┘
```

### Crates

| Crate | Purpose | Targets |
| :--- | :--- | :--- |
| **[`lichexternal-core`](crates/lichexternal-core)** | Pure protocol wire models, UCI command generators, zero-copy `memchr` parser, configuration models. Optional `ts-rs` type export for `Lichextension`. | `wasm32-unknown-unknown`, `x86_64`, `aarch64` |
| **[`lichexternal-daemon`](crates/lichexternal-daemon)** | Unattended daemon (`lichexternald`) running the concurrent engine supervisor pool, stdio/socket backends, broker long-polling, and local control IPC server. | Windows, Linux, macOS, Android |
| **[`lichexternal-cli`](crates/lichexternal-cli)** | User-facing management CLI (`lichexternal`) with tabular engine status views, local engine verification, and Lichess profile CRUD. | Windows, Linux, macOS |

---

## Quick Start

### 1. Configure Engines

Copy the documented template:
```bash
cp config.example.toml config.toml
```

Edit `config.toml` to declare your engines:
```toml
log_level = "info"
broker_base_url = "https://engine.lichess.ovh"

# Local Stockfish via process I/O
[[engines]]
id = "stockfish-local"
provider_secret = "YOUR_PROVIDER_SECRET_FROM_LICHESS"
name = "Stockfish 19 (Local)"
type = "stdio"
command = "/usr/bin/stockfish"
max_threads = 16
max_hash = 4096
variants = ["chess"]

# Chexpectimax or Maia via local socket
[[engines]]
id = "chexpectimax-local"
provider_secret = "YOUR_CHEXPECTIMAX_PROVIDER_SECRET"
name = "Chexpectimax (Meta-Engine)"
type = "socket"
socket = "/run/user/1000/chexpectimax.sock"
max_threads = 8
max_hash = 2048
variants = ["chess"]
```

### 2. Verify an Engine Locally

Ensure your engine executable adheres to standard UCI:
```bash
lichexternal test-engine /usr/bin/stockfish
```

### 3. Authenticate with Lichess

#### Option A: Interactive OAuth 2.0 PKCE Login (Recommended)
Log in with your Lichess account directly from the command line without creating or copying tokens:
```bash
lichexternal auth login
```
This automatically launches your default browser to authorize `chexperts-lichexternal` (`engine:read engine:write`), captures the authorization code on a local loopback server, and securely saves the credentials to `$XDG_CONFIG_HOME/lichexternal/credentials.toml` (with Unix `0600` permissions).

Verify your login status at any time:
```bash
lichexternal auth status
```

#### Option B: Personal Access Token (Headless / CI / NixOS)
Alternatively, export a personal access token generated at [lichess.org/account/oauth/token](https://lichess.org/account/oauth/token):
```bash
export LICHESS_API_TOKEN="lip_your_personal_token"
```

### 4. Register an Engine on Lichess

Once authenticated, register an engine profile directly from the CLI:
```bash
lichexternal engine register \
  --name "Stockfish 19 (Local)" \
  --max-threads 16 \
  --max-hash 4096 \
  --variants chess,atomic
```
This prints your newly assigned `id` and `provider_secret`. Paste the `provider_secret` into your `config.toml`.

### 5. Run the Daemon

```bash
lichexternald --config config.toml
```

### 5. Inspect Live Status

From another terminal, inspect live engine calculation states:
```bash
lichexternal status
```

Output:
```
Lichexternal Daemon v0.1.0

+--------------------+------------------------------+---------+-----------+--------------------+-------+
| ID                 | Name                         | Backend | Status    | Active Work        | Depth |
+--------------------+------------------------------+---------+-----------+--------------------+-------+
| stockfish-local    | Stockfish 19 (Local)         | stdio   | analyzing | work-abc-12345     | 24    |
| chexpectimax-local | Chexpectimax (Meta-Engine)   | socket  | idle      | -                  | -     |
+--------------------+------------------------------+---------+-----------+--------------------+-------+
```

---

## Wire Protocol & Security

1. **Authentication Separation**:
   - **REST API** (`https://lichess.org/api/external-engine`): Used only for engine profile CRUD. Authenticated via `Authorization: Bearer <personal_token>`.
   - **Broker Work API** (`https://engine.lichess.ovh/api/external-engine/work`): Used by the background daemon to acquire evaluation jobs. Authenticated via `{"providerSecret": "<secret>"}` in the JSON body.
   - *Security Benefit*: Daemons running on headless servers or untrusted nodes only need the individual engine's `providerSecret`, keeping your personal account credentials completely private.

2. **Chunked HTTP Line Streaming**:
   - Work analysis is streamed to `POST /api/external-engine/work/{id}` with `Content-Type: text/plain`.
   - Real-time `info ...` lines stream chunk-by-chunk as calculated.
   - Closing with `bestmove ...` finishes the HTTP request.

---

## Nix & NixOS Integration

### Running via Nix Flake
```bash
# Run daemon
nix run .#lichexternald -- --config config.toml

# Run CLI
nix run .#lichexternal -- status
```

### NixOS Service Module
Add `Lichexternal` to your `configuration.nix`:
```nix
{
  services.lichexternald = {
    enable = true;
    settings = {
      log_level = "info";
      engines = [
        {
          id = "stockfish-local";
          provider_secret = "secret-xyz";
          name = "Stockfish (NixOS)";
          type = "stdio";
          command = "${pkgs.stockfish}/bin/stockfish";
          max_threads = 8;
          max_hash = 2048;
          variants = [ "chess" ];
        }
      ];
    };
  };
}
```

### Home Manager Module
```nix
{
  services.lichexternald = {
    enable = true;
    settings = {
      engines = [ ... ];
    };
  };
}
```


