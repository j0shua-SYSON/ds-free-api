# Development Guide

## Requirements

- Rust **1.95.0+** (see `rust-toolchain.toml`)
- Bun **1.3+** (web panel build and development)
- `cmake`, `g++`, `libclang-dev` (required to compile BoringSSL for `wreq`)
- `just` command runner (for `just serve` / `just check` and other shortcuts)

## First Run

```bash
# 1. Copy config
cp config.example.toml config.toml

# 2. Build web frontend (embedded into binary at compile time; rebuild required for every frontend change)
cd web && bun install && bun run build && cd ..

# 3. Run development server
just serve
```

After the server starts, navigate to `http://localhost:22217` — it redirects automatically to the admin panel.

> **Frontend hot-reload development**: Run `cd web && bun run dev` (Vite HMR mode) and `just serve`
> simultaneously. The backend serves static files from `web/dist/` on the filesystem when available.
> No need to rebuild the binary on every frontend change.

## Release Build

```bash
# 1. Build web frontend
cd web && bun install && bun run build && cd ..

# 2. Build release binary
cargo build --release

# 3. Run (can also run the binary directly without the web/dist/ directory)
./target/release/ds-free-api
```

The release binary embeds frontend assets at compile time via `rust_embed`. When the `web/dist/`
directory is absent, the embedded assets are used automatically. No extra files are needed for
distribution.

## CI Build

GitHub Actions (`.github/workflows/release.yml`) triggers automatically on tag push:

```
build-frontend (bun install --frozen-lockfile + bun run build)
  ├── build-linux-gnu (cargo build)     │
  ├── build-linux-musl (musl-cross)     │── release (tar.gz + zip)
  ├── build-macos (cargo build)  │
  └── build-windows (cargo build)│
  └── docker (ghcr.io image)
```

`build-frontend` produces a `web-dist` artifact. Each compile job downloads it and then runs
`cargo build` / `cross build`, ensuring `rust_embed` embeds the real frontend files.

The Docker image is automatically pushed to `ghcr.io/niyueee/ds-free-api:latest`.

## Docker Deployment (Production)

Pull from ghcr.io (recommended):

```bash
# Ensure docker/config/ exists (created automatically, or manually via mkdir)
docker compose -f docker/docker-compose.yaml up -d
```

A minimal config is created automatically on first container startup; no need to prepare
`config.toml` beforehand. Config and data are persisted to the host via bind mounts at
`docker/config/` and `docker/data/`.

Build a local Docker image from source:

```bash
# 1. Build frontend + cross-compile binary
cd web && bun install && bun run build && cd ..
cargo zigbuild --release --target x86_64-unknown-linux-gnu

# 2. Build Docker image
docker build -f docker/Dockerfile -t ds-free-api .

# 3. Export and transfer to server
docker save ds-free-api | gzip > ds-free-api.tar.gz
scp ds-free-api.tar.gz user@server:/tmp/

# 4. Load and start on server
ssh user@server
docker load < /tmp/ds-free-api.tar.gz
docker compose -f docker/docker-compose.yaml up -d
```

> On a native x86 server, the build can be run directly on the server for better speed.
> The Docker image contains only the precompiled binary and embedded frontend assets — no
> in-container compilation needed.

## Command Reference

```bash
# One-pass check (check + clippy + fmt + audit + unused deps)
just check

# Run tests
cargo test --lib

# Run HTTP server
just serve

# Unified protocol debug CLI (built-in modes: chat/compare/concurrent, etc.)
just adapter-cli

# Start server with e2e-specific config
just e2e-serve
```

## e2e Tests

`py-e2e-tests/` is a JSON scenario-driven end-to-end test framework with no pytest dependency. It has three tiers:

| Tier | Command | Coverage |
| ---- | ------- | -------- |
| **Basic** | `just e2e-basic` | Core feature scenarios (both endpoints: OpenAI + Anthropic), safe concurrency |
| **Repair** | `just e2e-repair` | Tool call malformed-format repair (OpenAI endpoint only), safe concurrency |
| **Stress** | `just e2e-stress` | All scenarios x 3 iterations, safe concurrency + 1 |

Start the server first:

```bash
just e2e-serve
```

Then run e2e tests in another terminal:

```bash
# Basic scenario tests
just e2e-basic

# Tool repair tests
just e2e-repair
```

Scenario files are organized by type under `scenarios/`:

```
py-e2e-tests/
├── scenarios/
│   ├── basic/
│   │   ├── openai/         # 7 basic scenarios (chat, reasoning, streaming, tool call, file upload, image upload, HTTP link)
│   │   └── anthropic/      # 7 basic scenarios (chat, reasoning, streaming, tool call, document upload, image upload, HTTP link)
│   └── repair/             # 10 malformed tool call format scenarios
├── runner.py               # Single-run entry point
├── stress_runner.py        # Multi-iteration stress test entry point
└── config.toml             # e2e-specific server config
```

Each scenario is an independent JSON file containing request parameters and validation rules:

```json
{
  "name": "scenario name",
  "endpoint": "openai|anthropic",
  "category": "basic|repair",
  "models": ["deepseek-default", "deepseek-expert", "deepseek-vision"],
  "messages": [{"role": "user", "content": "..."}],
  "tools": [...],
  "tool_choice": "auto",
  "request": {"stream": false},
  "checks": {
    "has_tool_calls": true,
    "tool_names": ["get_weather"],
    "finish_reason": "tool_calls",
    "no_error": true
  }
}
```

### e2e CLI Parameters

**`just e2e-basic` and `just e2e-repair` (single run):**

| Parameter | Description |
|-----------|-------------|
| `scenario_dir` | Scenario directory, e.g. `scenarios/basic` or `scenarios/repair` |
| `--endpoint` | Endpoint filter: `openai` / `anthropic` |
| `--model` | Model filter: `deepseek-default` / `deepseek-expert` |
| `--filter` | Scenario name keyword filter (space-separated for multiple, e.g. `--filter file image`) |
| `--parallel` | Concurrency, default `account_count / 2` |
| `--show-output` | Show model response summary, tool calls, and finish reason |
| `--report` | Output path for JSON report |

**`just e2e-stress` (stress test):**

| Parameter | Description |
|-----------|-------------|
| `--iterations` | Iterations per scenario, default 3 |
| `--models` | Model list filter |
| `--filter` | Scenario name keyword filter (space-separated for multiple) |
| `--parallel` | Concurrency, default `account_count / 2 + 1` |
| `--show-output` | Show model output |
| `--report` | Output path for JSON report |

Examples:

```bash
# Quickly validate newly-added file upload scenarios
just e2e-basic --filter file image --show-output

# Check only the expert model on the OpenAI endpoint
just e2e-basic --endpoint openai --model deepseek-expert

# Serial debugging
just e2e-basic --endpoint openai --parallel 1 --show-output

# Stress test: tool call repair scenarios x 5 iterations
just e2e-stress --filter repair --iterations 5

# Output JSON report
just e2e-basic --report result.json
```

## Further Documentation

- [Code Style](code-style.md)
- [Logging Spec](logging-spec.md)
- [Prompt Injection Strategy](deepseek-prompt-injection.md)
