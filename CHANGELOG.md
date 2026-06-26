# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.2.7-pre1] - 2026-05-14

### Fix: primarily addresses issues caused by the official restriction on file uploads for the expert model, along with other miscellaneous changes

The root cause is that the web client has a per-request `input_character_limits` cap, so long conversation histories were passed via a single uploaded file. This time, the official backend restricted file uploads for the expert model and silently ignored the upload without returning an error, which prevented the fallback from triggering and caused expert mode to malfunction.

The current solution is:

- default and vision use the original mode, but the history file is only triggered when the input exceeds `input_character_limits * 75 / 100`; otherwise the request proceeds as a single inline payload;
- expert uses a new chunked completion mode, also triggered when the limit is exceeded: completion_1 carries a portion of the history, then immediately calls stop_stream to prevent actual model output, then starts completion_2, and so on until the full conversation history is assembled. This implementation feels somewhat fragile; if you have a better idea, feel free to open an issue or PR.

> Note: some accounts do not yet have vision testing enabled, which may cause empty responses for vision requests.

---

- [x] Merge and fix PR #52, using Co-Authored-By attribution (Web English version)
- [x] Enforce stricter lint checks
- [x] Add test accounts
- [x] Switch default frontend dev runtime to Bun
- [x] Handle PR #63: emit synthetic `message_delta` and `message_stop` when upstream errors after `message_start`, preventing client hangs
- [x] Refactor variable name from `rquest` to `wreq` and update related dependencies
- [x] Merge and handle PR #66
- [x] Align with the latest streaming processing logic and fix several related issues
- [x] Merge and handle PR #67, implementing frontend color theme switching
- [ ] Implement the `v1/files` endpoint described in Issue #65, adding required `model_type` support for vision models
- [x] Fix issues in expert mode
- [x] Implement `model_type: vision` and sync the web API accordingly
  - [x] Test the `search_enabled` parameter to confirm whether the backend silently ignores it
  - [x] Add end-to-end tests
- [ ] Investigate the stray characters reported in Issue #58
- [ ] Treat `<|End▁of▁sentence|>` from Issue #53 as an internal forced stop token to fix model hallucinations where it generates the user's reply
- [ ] Address the qwenpaw-related issue in Issue #56; expected fix is to replace the OpenAI adapter's empty tool-call keepalive with an empty thinking block
- [ ] Add project update notifications
- [ ] Include the [DeepSeek status page](https://status.deepseek.com/) in the notification list
- [ ] Implement the response endpoint

## [0.2.6] - 2026-05-05

### Added
- **Web admin panel**: SPA built with Vite + React + shadcn/ui, including login, Dashboard overview page, and config editor page.
  `PUT /admin/api/config` replaces the 6 scattered legacy endpoints for keys/accounts CRUD, reload, and relogin.
  The config editor supports seven sections: Server, DeepSeek, model types, tool call tags, proxy, accounts, and API Keys,
  with accounts and Keys expanded by default and all other sections collapsed.
- **Admin security**: `auth.rs` JWT issuance/verification (HMAC + SHA256), admin password setup and login,
  password bcrypt hash storage, and login rate limiting.
- **Config management enhancements**:
  - Auto-creation: when the config file does not exist, a minimal configuration is generated and written to disk
  - `Config::save()` atomic write (tmp + rename + 0600 permissions)
  - `Config` changed to `Arc<RwLock<Config>>`, mutable at runtime with admin panel changes automatically persisted
  - `DS_CONFIG_PATH` environment variable; priority order: `-c` > `DS_CONFIG_PATH` > default `config.toml`
  - Config merge: `admin.json` and `api_keys.json` merged into the `[admin]` / `[[api_keys]]` sections of `config.toml`
  - PUT config merge protection: when a password or key field is `***` or empty, the current value is automatically preserved
- **Docker deployment**: `docker/Dockerfile` (alpine:3.21, musl static build, ~20 MB image),
  `docker/docker-compose.yaml`, and `docker/config.example.toml` (host = 0.0.0.0, empty accounts).
  Image published to ghcr.io.
- **Full retry logging**: `try_chat()` emits a WARN log on every Overloaded backoff retry (including attempt count and wait time),
  INFO on successful retry, and a final WARN on total failure.
- **WAF hint**: when an AWS WAF Challenge is detected, a clear bilingual hint is printed instead of the previous opaque error.
- **Automatic account deduplication**: on startup, accounts are deduplicated by email (preferred) or mobile.
- **`X-Client-Locale` header**: `DeepSeekConfig` gains a `client_locale` field, defaulting to `zh_CN`.
- **Proxy configuration**: `[proxy]` config section supporting HTTP/HTTPS/SOCKS5.
- **CI build-frontend independent job**: artifacts are consumed by backend check/test jobs, ensuring the binary embeds the real frontend files.
- **GPL-3.0 license**

### Changed
- **HTTP client**: `reqwest` (rustls) -> `rquest` (BoringSSL + Chrome 136 TLS fingerprint emulation).
  After the switch, TLS handshake fingerprints emulate Chrome 136, combined with Android request headers to bypass WAF fingerprint detection.
- **Default port**: `5317` -> `22217`, avoiding the Win10 Hyper-V dynamic port reservation range (5000-6000).
- **Default headers**: fully switched to DeepSeek Android client format —
  `User-Agent: DeepSeek/2.0.4 Android/35`, `X-Client-Version: 2.0.4`, `X-Client-Platform: android`
- **wasmtime**: 43.0.0 -> 44.0.0, fixing security advisory RUSTSEC-2026-0114.
- **`model_aliases` type**: `HashMap<String, String>` -> `Vec<String>`, aligned to `model_types` by index.
- **`/` root path**: changed from a JSON endpoint list to a 302 redirect to `/admin`.
- **Colored stderr logs**: TRACE=purple, INFO=green, WARN=yellow, ERROR=red, DEBUG=blue; enabled only when a terminal is attached.
- **handler/store refactor**:
  - `chat_completions` / `anthropic_messages` stats logging extracted into `AppState::record_request()`
  - `admin_setup` / `admin_login` compressed from ~50 lines each to ~12 lines
  - `admin_reload_config` compressed from ~70 lines to ~10 lines
  - `StoreManager` changed from reading/writing independent JSON files to delegating to the shared `Arc<RwLock<Config>>`
- **CI build refactor**:
  - `build-frontend` as an independent job; check/test jobs depend on it via `needs`
  - `cross` upgraded to 0.2.5; aarch64-linux-gnu/musl migrated to native ARM runner (`ubuntu-24.04-arm`)
  - `actions-rust-lang/setup-rust-toolchain` replaces `dtolnay/rust-toolchain`
  - `just check-web` added as a frontend validation command (npm ci + build + lint)
- **Stale code removal**:
  - Removed 6 scattered admin endpoints (keys CRUD / accounts CRUD / reload / relogin)
  - Removed `sse_stream()` / `SseSerializer` (streaming responses now use `inspect`/`map`/`TokenGuardStream` throughout)
  - Removed `StopStream` / repetition detection
  - Removed `.dockerignore`, root-level `Dockerfile` / `docker-compose.yml`
  - Removed `web/config.toml` and other obsolete files

### Removed
- `reqwest` dependency
- `admin.json` and `api_keys.json` as standalone files (merged into `config.toml`)
- `accounts.is_empty()` check at startup (accounts can now be added via the admin panel)
- `DS_CONFIG` environment variable (replaced by `DS_CONFIG_PATH`)
- `web/config.toml`

### Fixed
- **CI idempotency**: added `command -v` pre-check to `cargo install` steps.
- **client.rs log violation**: 11 `warn!` calls in `print_waf_hint()` now supply the target argument.
- **stats.json empty file**: no longer triggers an EOF parse WARN; downgraded to INFO.
- **e2e port hard-coding**: runner.py / stress_runner.py now read the port dynamically from config.toml.
- **AGENTS.md stale content**: updated `/` endpoint description, `[[server.api_tokens]]` -> `[[api_keys]]`, WASM troubleshooting, etc.

### Docs
- **README / README.en.md**: added environment variable table; design philosophy updated with "avoid introducing extra runtime system dependencies unless necessary"; admin panel screenshots.
- **`docs/en/`**: English documentation directory; all docs available in English.
- **`docs/development.md` / `docs/en/development.md`**: development guide covering builds, Docker, and e2e testing.
- **Prompt injection strategy**: updated the DeepSeek native tag injection strategy documentation in the README.
- **CLAUDE.md / AGENTS.md**: streamlined architecture description; added troubleshooting table, request-tracing grep examples, and `#[allow]` policy notes.

## [0.2.5] - 2026-04-30

### Added
- **File upload**: supports uploading files/images to DeepSeek via the API. The `file` / `image_url` content part for the OpenAI endpoint
  and the `document` / `image` content block for the Anthropic endpoint are both supported. Inline data URLs are uploaded automatically;
  HTTP URLs trigger search mode, where the model fetches the content itself.
- **Native XML `<invoke>` format parsing**: directly parses `<invoke name="..."><parameter>` format tool calls
  without triggering the repair pipeline, resulting in faster responses.
- **Streaming tool call keepalive**: during tool call generation by the model (typically 2-10s), an empty delta chunk is sent every 1s to prevent client timeouts.
  The OpenAI endpoint sends an empty `tool_calls` delta; the Anthropic endpoint sends a `"tool_calls..."` thinking block.
- **User-maintained tool call tags**: `config.toml` gains a `[deepseek.tool_call]` config section
  where users can append newly discovered model hallucination tags at any time without waiting for a code update.

### Changed
- **Prompt format upgrade**: fully migrated from ChatML (`<|im_start|>` / `<|im_end|>`) to the DeepSeek native tag format.
  A `<|end▁of▁sentence|>` is inserted before each `<|User|>` to close the previous turn; tool results are now wrapped with `<|tool▁outputs▁begin|>`;
  the reminder is embedded inside a `<think>` block. After aligning with the DeepSeek official chat_template, instruction-following compliance improved significantly.
- **Tool call primary tag change**: changed from `<|tool_calls_begin|>` to `<|tool▁calls▁begin|>` / `<|tool▁calls▁end|>`
  (using ASCII `|` + `▁`). The probability that the model outputs this tag is significantly higher than the old tag, and hallucination variants are noticeably reduced.
  Default fallback tags cover known variants: `<|tool_calls_begin|>`, `<|tool▁calls_begin|>`, `<|tool_calls▁begin|>`, `<tool_call>`
- **Smart search enabled by default**: in search mode, the system prompt injected by DeepSeek is stronger and improves tool call compliance.

### Fixed
- **Anthropic protocol compatibility**: `message_start` now includes `stop_reason: null` / `stop_sequence: null`;
  `message_delta` always carries `usage.output_tokens`; usage is no longer always 0.
  These fixes resolve compatibility issues with standard Anthropic clients such as Claude Code.
- **File upload error handling**: when the history conversation file upload fails, it automatically falls back to an inline prompt instead of silently losing context;
  external file upload failures now return an explicit error instead of silently skipping.
- **Repair model accuracy**: self-repair requests now automatically include the tool definition list and JSON escaping hints,
  significantly improving the model's ability to infer correct parameters from malformed text.

## [0.2.4] - 2026-04-27

### Added
- **Conversation history as files**: multi-turn conversation history is automatically split and uploaded as separate files, bypassing DeepSeek's per-request input length limit.
  Completely transparent to the adapter layer; upload failures do not affect the main flow and automatically degrade to plain text.
- **Ephemeral session lifecycle**: each request creates an independent session that is automatically cleaned up at the end (stop_stream + delete_session),
  completely eliminating session leaks and TTL expiration residue.
- **Tool call self-repair**: when the model's tool_calls output is malformed, DeepSeek itself is used to repair the broken JSON/XML;
  both streaming and non-streaming paths are covered, greatly improving tool call success rates.
- **Normalized arguments type**: automatically handles the case where arguments is a JSON string instead of an object, preventing double-escaping parse failures on the client side.
- **`input_exceeds_limit` detection**: recognizes input-too-long errors and returns a clear error message instead of failing silently.
- **Full-chain request tracing**: a `req-{n}` identifier threads through all layers from handler to adapter to ds_core;
  the `x-ds-account` response header identifies the processing account, enabling complete grep-based tracing of a single request.
- **TRACE-level byte tracing**: TRACE logs at each stage of the streaming pipeline, allowing observation of the complete transformation of bytes through the SSE pipeline.
- **`/` endpoint**: returns the available endpoint list and project URL without requiring authentication.
- **e2e test refactor**: migrated from pytest to a JSON scenario-driven framework with independently stored scenarios and dynamic config reading.

### Changed
- **Request flow refactor**: upgraded from "persistent session + edit_message" to "ephemeral session + completion + file upload",
  with each request having an independent lifecycle and no longer relying on pre-created persistent sessions.
- **Automatic rate-limit retry**: when rate_limit is detected, automatically retries with exponential backoff (1s->2s->4s->8s->16s, up to 6 attempts),
  transparent to the user, greatly reducing request failures caused by rate limiting.
- **Prompt construction optimization**: the reminder is now inserted before the last conversation turn to ensure the model prioritizes following instructions;
  tool descriptions are formatted in code blocks; tool call results are displayed with Markdown structure.
- **Reasoning control semantic fix**: when disabling thinking, `"none"` is used instead of `"minimal"`, making the semantics clearer.
- **Log level normalization**: account pool exhaustion promoted to `WARN`, normal allocation demoted to `DEBUG`,
  added debug logs for session/upload/PoW, and health_check consolidated into a single log with elapsed time.

### Removed
- Account initialization no longer manages sessions by model_type; session persistence and update_title logic removed.
- Removed the old pytest e2e test directory (replaced by the JSON scenario-driven framework).

### Test Results

#### py-e2e-tests
- **4 accounts + 3 concurrent + 3 iterations**: 17 scenarios x 2 models x 3 runs = 102 requests, 100% success rate, total duration 5.5 minutes
- Scenarios covered: basic chat, deep reasoning, streaming, standard tool calls, and 10 types of malformed tool_calls
  (XML/JSON mixed, inconsistent field names, arguments as string, mismatched/missing brackets,
  name/arguments swapped, parameter overflow, etc.); the repair pipeline handled all cases correctly.

#### claude-code Tests
```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:5317/anthropic
export ANTHROPIC_AUTH_TOKEN=sk-test
export ANTHROPIC_DEFAULT_OPUS_MODEL=deepseek-expert
export ANTHROPIC_DEFAULT_SONNET_MODEL=deepseek-expert
export ANTHROPIC_DEFAULT_HAIKU_MODEL=deepseek-default
claude
```
- Basically stable; it is normal for claude-code to pause briefly during tool parsing. In some cases, the model may fail to follow instructions, causing tool call instruction leakage.
- Other programming tools have not been extensively tested; user feedback is welcome.

## [0.2.3] - 2026-04-24

### Added
- Tool call XML parsing enhancements: added `repair_invalid_backslashes` and `repair_unquoted_keys`
  for lenient repair; automatically repairs and retries when the model's JSON output contains unquoted keys or invalid escape sequences.
- Added `is_inside_code_fence` check: skips tool examples inside markdown code fences to prevent false parsing.
- Added Anthropic protocol stress test script `stress_test_tools_anthropic.py`, symmetric with the OpenAI version.
- Example files orthogonalized: split into separate files under `examples/adapter_cli/` by function:
  `basic_chat`, `stream`, `stop`, `reasoning`, `web_search`, `reasoning_search`, `tool_call`, and others.
- Default adapter-cli config file path now points to `py-e2e-tests/config.toml`.

### Changed
- Account pool selection strategy: changed from **round-robin linear probing** to **longest-idle-first**, maximizing the interval between account reuse.
- Removed fixed cooldown time constants; the selection algorithm naturally prevents accounts from being reused too quickly.
- Updated both Chinese and English READMEs to include concurrency guidance.

### Stress Test Results

Stress test of 70 requests against a 4-account pool (7 scenarios x 2 models x 5 iterations):

| Strategy | Concurrency | Success Rate | Avg Latency |
|------|------|--------|----------|
| Round-robin + no cooldown | 3 | 25.7% | 2.57s |
| Round-robin + 2s cooldown | 3 | 97.1% | 10.46s |
| **Longest-idle-first + no cooldown** | **2** | **100%** | **10.14s** |
| **Longest-idle-first + no cooldown (Anthropic)** | **2** | **100%** | **11.31s** |

Conclusion: stable safe concurrency ~= number of accounts / 2; the longest-idle-first strategy achieves 100% success rate without any cooldown.

## [0.2.2] - 2026-04-22

### Added
- Anthropic Messages API compatibility layer:
  - `/anthropic/v1/messages` streaming + non-streaming endpoints
  - `/anthropic/v1/models` list/get endpoints (Anthropic format)
  - Request mapping: Anthropic JSON -> OpenAI ChatCompletion
  - Response mapping: OpenAI SSE/JSON -> Anthropic Message SSE/JSON
- OpenAI adapter backward compatibility:
  - Deprecated `functions`/`function_call` automatically mapped to `tools`/`tool_choice`
  - `response_format` downgrade: JSON/Schema constraints injected into the ChatML prompt (`text` type is a no-op)
- CI release workflow improvements:
  - tag-triggered release (`push.tags v*`)
  - CHANGELOG version notes extracted automatically
  - pre-release check verifying that the Cargo.toml version matches the tag

### Changed
- Rust toolchain upgraded to 1.95.0; CI workflow updated accordingly.
- justfile adds `set positional-arguments` to safely pass arguments containing spaces.
- Python E2E test suite reorganized into `openai_endpoint/` and `anthropic_endpoint/`.
- Startup logs now display OpenAI and Anthropic base URLs.
- README/README.en.md: added SVG icons, GitHub badges, and synced documentation.
- LICENSE: added copyright notice `Copyright 2026 NIyueeE`.
- CLAUDE.md/AGENTS.md updated accordingly.

### Fixed
- Anthropic streaming tool call protocol: uses `input_json_delta` events to stream tool parameters incrementally.
- Tool use ID mapping consistency: `call_{suffix}` -> `toolu_{suffix}`.
- Anthropic tool definition compatibility: handles missing `type` field (Claude Code client).

## [0.2.1] - 2026-04-15

### Added
- Deep reasoning enabled by default: `reasoning_effort` defaults to `high`; search is disabled by default.
- WASM dynamic detection: `pow.rs` now uses signature-based dynamic export probing instead of hard-coding `__wbindgen_export_0`, reducing the risk of startup failures after DeepSeek updates the WASM.
- Added Python E2E test suite covering auth, models, chat completions, tool calling, and other scenarios.
- Added `tiktoken-rs` dependency for server-side prompt token counting.
- CI: added `cargo audit` and `cargo machete` checks.

### Changed
- Account initialization optimization: the log automatically falls back to showing the email when the phone number is empty.
- Updated `axum`, `cranelift`, and other core dependencies to the latest patch versions.
- Client Version kept at `1.8.0` to match the web client.

### Removed
- Removed unused `tower` dependency.

## [0.2.0] - 2026-04-13

### Added
- Project fully rewritten from Python to Rust, bringing native high performance and cross-platform support.
- OpenAI-compatible API (`/v1/chat/completions`, `/v1/models`).
- Account pool rotation + PoW solving + SSE streaming responses.
- Deep reasoning and smart search support.
- Tool calling (XML parsing).
- GitHub CI + multi-platform release (8 target platforms).
- Compatible with the latest DeepSeek web backend API.
