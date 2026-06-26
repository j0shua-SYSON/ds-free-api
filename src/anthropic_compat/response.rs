//! Anthropic response mapping -- maps OpenAI ChatCompletion to Anthropic Message
//!
//! Facade module: declares sub-modules, exposes shared types and helper functions.
//! `MessagesResponse` / `Usage` are defined in `types.rs` (in the same module as request types).

mod aggregate;
mod stream;

pub(crate) use aggregate::from_chat_completions;
pub(crate) use stream::from_chat_completion_stream;

/// Response content block -- defined as `ResponseContentBlock` in `types.rs`, aliased here for sub-module compatibility
pub(crate) use crate::anthropic_compat::types::ResponseContentBlock as ContentBlock;

// ============================================================================
// Shared helper functions
// ============================================================================

pub(crate) fn finish_reason_map(reason: &str) -> String {
    match reason {
        "stop" => "end_turn".to_string(),
        "tool_calls" => "tool_use".to_string(),
        _ => reason.to_string(),
    }
}

/// OpenAI id format is chatcmpl-xxx, mapped to msg_xxx
pub(crate) fn map_id(openai_id: &str) -> String {
    openai_id
        .strip_prefix("chatcmpl-")
        .map(|hex| format!("msg_{}", hex))
        .or_else(|| {
            openai_id
                .strip_prefix("call_")
                .map(|suffix| format!("toolu_{}", suffix))
        })
        .unwrap_or_else(|| format!("msg_{}", openai_id))
}
