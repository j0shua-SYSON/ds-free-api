//! Model resolution -- map the OpenAI model field to ds_core capability flags
//!
//! Dynamic mapping from model alias to model_type via an externally injected registry.

use std::collections::HashMap;

use crate::openai_adapter::types::WebSearchOptions;

/// Model resolution result
pub(crate) struct ModelResolution {
    /// model_type used by ds_core
    pub model_type: String,
    pub thinking_enabled: bool,
    pub search_enabled: bool,
}

/// Resolve the model configuration from model_id and extended parameters
///
/// thinking_enabled is enabled when reasoning_effort is not "none".
/// If reasoning_effort is not provided, it defaults to "high" (i.e., reasoning is enabled by default).
/// search_enabled is on by default (the DeepSeek backend injects a stronger system prompt in search mode).
/// Explicitly setting web_search_options overrides this behavior.
pub(crate) fn resolve(
    registry: &HashMap<String, String>,
    model_id: &str,
    reasoning_effort: Option<&str>,
    web_search_options: Option<&WebSearchOptions>,
) -> Result<ModelResolution, String> {
    let key = model_id.to_lowercase();
    let model_type = registry
        .get(&key)
        .cloned()
        .ok_or_else(|| format!("unsupported model: {}", model_id))?;

    let reasoning_effort = reasoning_effort.unwrap_or("high");
    let thinking_enabled = reasoning_effort != "none";

    let search_enabled = web_search_options.map(|_| true).unwrap_or(true);

    Ok(ModelResolution {
        model_type,
        thinking_enabled,
        search_enabled,
    })
}
