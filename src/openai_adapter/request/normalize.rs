//! Request validation and default normalization
//!
//! Responsibility: validate required fields and message format, and normalize optional parameters
//! into standardized values for internal use.

use crate::openai_adapter::types::{ChatCompletionsRequest, StopSequence};

pub(crate) struct NormalizedParams {
    pub include_usage: bool,
    pub include_obfuscation: bool,
    pub stop: Vec<String>,
}

/// Normalize and return standardized parameters
///
/// Validation rules:
/// - model must not be empty
/// - messages must not be empty
/// - messages with role=tool must include tool_call_id
/// - messages with role=function must include name
pub(crate) fn apply(req: &ChatCompletionsRequest) -> Result<NormalizedParams, String> {
    if req.model.trim().is_empty() {
        return Err("missing required field 'model'".into());
    }

    if req.messages.is_empty() {
        return Err("missing required field 'messages'".into());
    }

    for (i, msg) in req.messages.iter().enumerate() {
        match msg.role.as_str() {
            "tool" if msg.tool_call_id.is_none() => {
                return Err(format!(
                    "messages[{}] role 'tool' requires 'tool_call_id'",
                    i
                ));
            }
            "function" if msg.name.is_none() => {
                return Err(format!("messages[{}] role 'function' requires 'name'", i));
            }
            _ => {}
        }
    }

    let include_usage = req
        .stream_options
        .as_ref()
        .map(|o| o.include_usage)
        .unwrap_or(false);

    let include_obfuscation = req
        .stream_options
        .as_ref()
        .map(|o| o.include_obfuscation)
        .unwrap_or(true);

    let stop = match &req.stop {
        Some(StopSequence::Single(s)) => vec![s.clone()],
        Some(StopSequence::Multiple(v)) => v.clone(),
        None => Vec::new(),
    };

    Ok(NormalizedParams {
        include_usage,
        include_obfuscation,
        stop,
    })
}
