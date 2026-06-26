# Code Style Conventions

## Comment Style

### Module Documentation (//!)
- First line: module responsibility — concrete description
- After blank line: key design decisions or constraints

```rust
//! Account pool management — multi-account load balancing
//!
//! 1 account = 1 session = 1 concurrency
```

### Public API Documentation (///)
- Use verb-led sentences: "Returns", "Creates", "Sends"
- State side effects explicitly: "auto-releases", "cleans up session"
- Document Panic conditions (if any)

```rust
/// Polls for a free account.
///
/// The returned AccountGuard automatically releases the busy flag on Drop.
pub fn get_account(&self) -> Option<AccountGuard>
```

### Inline Comments (//)
- Explain "why", not "what"
- Note workarounds or external dependencies

```rust
// Order matters: health_check must come before update_title,
// otherwise an empty session will cause EMPTY_CHAT_SESSION errors
```

## Naming Conventions

| Type | Style | Example |
|------|-------|---------|
| Module/file | snake_case | `ds_core`, `accounts.rs` |
| Type/struct | PascalCase | `AccountPool`, `CoreError` |
| Function/method | snake_case | `get_account()`, `compute_pow()` |
| Constant | SCREAMING_SNAKE_CASE | `ENDPOINT_USERS_LOGIN` |
| Enum variant | PascalCase | `AllAccountsFailed` |

## Error Messages

- **Chinese**: user-facing error messages (config validation, account management, etc.) are in Chinese
- **English**: internal library errors (`ds_core`, `client`, `adapter`, `anthropic_compat`) are in English for developer debugging
- Include context: "Account {} initialization failed"
- Avoid leaking sensitive data (tokens: print only the first 8 characters)
- The server-layer `ServerError::Display` passes the adapter's original error message to API clients unchanged

## Enum Variant Naming

- All enum variants use PascalCase (e.g. `AllAccountsFailed`, `BadRequest`)
- Use non-PascalCase only for serde serialization via `#[serde(rename = "...")]`

## Logging Specification

See `docs/logging-spec.md`

## Import Grouping

1. Standard library (`std::`)
2. Third-party crates (`tokio::`, `wreq::`)
3. Internal modules (`crate::`)
4. Local use (super, self)

Separate groups with blank lines.

## Test Code Conventions

- `println!` is allowed inside test functions to print intermediate results for debugging failures
- Library code (non-`#[cfg(test)]` areas in `src/`) still prohibits direct use of `println!` / `eprintln!`
