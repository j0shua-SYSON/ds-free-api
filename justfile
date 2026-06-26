# Justfile for ai-free-api

set positional-arguments

# Run all checks: type check, lint, format, audit, unused deps
# Prerequisites: cargo install cargo-audit && cargo install cargo-machete && cargo install cargo-outdated
check:
  cargo fmt --check      
  cargo check            
  cargo clippy -- -D warnings  
  cargo audit --deny warnings
  cargo outdated --exit-code 1 --root-deps-only
  cargo outdated -p ds_core --exit-code 1 --root-deps-only
  cargo machete          

# Build + lint frontend (bun install --frozen-lockfile, bun run typecheck + build + lint)
check-web:
  cd web && bun install --frozen-lockfile && bun run typecheck && bun run build && bun run lint


# Run unified protocol debug CLI (replaces ds-core-cli / openai-adapter-cli)
# Defaults to py-e2e-tests/config.toml; override with -c <path>
adapter-cli *ARGS:
  cargo run --example adapter_cli -- -c py-e2e-tests/config.toml "$@"

# Run openai_adapter/request submodule tests
test-adapter-request *ARGS:
  cargo test openai_adapter::request -- "$@"

# Run openai_adapter/response submodule tests
test-adapter-response *ARGS:
  cargo test openai_adapter::response -- "$@"

# Run HTTP server (builds latest frontend then starts backend)
serve *ARGS:
  (cd web && bun run build) && cargo run -- "$@"

# Basic: core feature tests (both endpoints)
e2e-basic *ARGS:
  cd py-e2e-tests && uv run python runner.py scenarios/basic "$@"

# Repair: tool call malformed-format repair tests
e2e-repair *ARGS:
  cd py-e2e-tests && uv run python runner.py scenarios/repair "$@"

# Stress: multi-iteration concurrent stress test (all basic + repair scenarios)
e2e-stress *ARGS:
  cd py-e2e-tests && uv run python stress_runner.py "$@"

# Oversized: long-context fallback test (expert chunked + default/vision file upload)
e2e-oversized *ARGS:
  cd py-e2e-tests && uv run python test_oversized.py "$@"

# Start server with e2e test config
e2e-serve:
  (cd web && bun run build) && cargo run -- -c py-e2e-tests/config.toml
