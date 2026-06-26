//! Anthropic protocol compatibility layer -- provides Anthropic API-compatible interface based on openai_adapter
//!
//! This module does not access ds_core directly; all data is fetched through openai_adapter and format-mapped.
//! Request flow: Anthropic JSON -> ChatCompletionsRequest -> openai_adapter -> response mapped back to Anthropic format.

mod models;
pub(crate) mod request;
pub(crate) mod response;
pub(crate) mod types;

pub use types::{MessagesRequest, MessagesResponse, MessagesResponseChunk};

/// Anthropic streaming response type (struct stream)
pub type ChunkStream =
    Pin<Box<dyn Stream<Item = Result<MessagesResponseChunk, AnthropicCompatError>> + Send>>;

/// Anthropic streaming response type (SSE byte stream)
pub type StreamResponse = Pin<Box<dyn Stream<Item = Result<Bytes, AnthropicCompatError>> + Send>>;

use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use futures::Stream;
use log::debug;

use crate::openai_adapter::{ChatOutput, ChatResult, OpenAIAdapter, OpenAIAdapterError};

/// Anthropic unified output (counterpart to openai_adapter's ChatOutput)
pub enum AnthropicOutput {
    Stream(ChunkStream),
    Json(MessagesResponse),
}

/// Anthropic compatibility layer
pub struct AnthropicCompat {
    openai_adapter: Arc<OpenAIAdapter>,
}

impl AnthropicCompat {
    /// Create a compatibility layer instance
    pub fn new(openai_adapter: Arc<OpenAIAdapter>) -> Self {
        Self { openai_adapter }
    }

    /// POST /v1/messages (unified entry point)
    ///
    /// Maps the Anthropic request to a ChatCompletionsRequest, delegates to openai_adapter,
    /// then maps the result back to Anthropic format based on OpenAI stream output.
    pub async fn messages(
        &self,
        req: MessagesRequest,
        request_id: &str,
    ) -> Result<ChatResult<AnthropicOutput>, AnthropicCompatError> {
        debug!(target: "anthropic_compat", "received messages request");
        let chat_req = request::into_chat_completions(req);
        let result = self
            .openai_adapter
            .chat_completions(chat_req, request_id)
            .await?;
        let data = match result.data {
            ChatOutput::Stream(stream) => {
                AnthropicOutput::Stream(response::from_chat_completion_stream(stream))
            }
            ChatOutput::Json(json) => {
                let msg = response::from_chat_completions(&json);
                AnthropicOutput::Json(msg)
            }
        };
        Ok(ChatResult {
            data,
            account_id: result.account_id,
            prompt_tokens: result.prompt_tokens,
        })
    }

    /// GET /v1/models
    ///
    /// Returns a model list in Anthropic format.
    pub async fn list_models(&self) -> models::AnthropicModelList {
        debug!(target: "anthropic_compat", "received model list request");
        models::list(&self.openai_adapter.list_models().await)
    }

    /// GET /v1/models/{model_id}
    ///
    /// Returns the Anthropic format details for the specified model.
    pub async fn get_model(&self, model_id: &str) -> Option<models::AnthropicModel> {
        debug!(target: "anthropic_compat", "querying model: {}", model_id);
        models::get(&self.openai_adapter.list_models().await, model_id)
    }
}

/// Anthropic compatibility layer error type
#[derive(Debug, thiserror::Error)]
pub enum AnthropicCompatError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("service overloaded")]
    Overloaded,
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<OpenAIAdapterError> for AnthropicCompatError {
    fn from(e: OpenAIAdapterError) -> Self {
        match e {
            OpenAIAdapterError::BadRequest(msg) => Self::BadRequest(msg),
            OpenAIAdapterError::Overloaded => Self::Overloaded,
            OpenAIAdapterError::ProviderError(msg)
            | OpenAIAdapterError::Internal(msg)
            | OpenAIAdapterError::ToolCallRepairNeeded(msg) => Self::Internal(msg),
        }
    }
}

impl AnthropicCompatError {
    /// Returns the corresponding HTTP status code
    #[must_use]
    pub fn status_code(&self) -> u16 {
        match self {
            Self::BadRequest(_) => 400,
            Self::Overloaded => 429,
            Self::Internal(_) => 500,
        }
    }
}
