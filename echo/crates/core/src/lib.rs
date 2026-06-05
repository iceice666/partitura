mod adapters;
mod config;
mod error;
mod events;
mod models;
mod oauth;
mod providers;
mod registry;
mod stream;

use std::sync::{Arc, OnceLock};

pub use adapters::{
    AnthropicMessagesAdapter, ImageFetchPolicy, OpenAiCodexResponsesAdapter,
    OpenAiCompletionsAdapter, OpenAiResponsesAdapter, parse_anthropic_sse,
    parse_openai_completions_sse, parse_openai_responses_sse, simulate_openai_non_streaming,
};
pub use config::{
    Config, Credential, OAuthToken, ProviderConfig, ReplConfig, Secret, TokenStore,
    credential_status, load_config, login, logout, resolve_credential, resolve_default_model,
    resolve_openai_org_id, resolved_config_view, write_config,
};
pub use error::{Error, Result};
pub use events::{
    DoneReason, EchoEventLine, ErrorReason, Event, StopReason, TextDelta, ThinkingDelta,
    ToolCallDelta,
};
pub use models::{
    Api, AssistantMessage, Block, Context, Cost, ImageSource, Message, Modality, Model, Options,
    Provider, ThinkingLevel, TokenCost, Tool, Usage,
};
pub use oauth::{
    ChatGptOAuth, ChatGptOAuthOptions, LoginServer, OAuthCodeTokens, OAuthRefreshTokens, PkceCodes,
};
pub use providers::{
    ApiProvider, HttpRequest, ProviderCompat, get_api_provider, register_api_provider,
};
pub use registry::{calculate_cost, clamp_thinking_level, get_model, get_models, get_providers};
pub use stream::{
    AbortHandle, EventStream, RetryPolicy, complete, enforce_retry_delay, is_context_overflow,
    retry_transient, stream,
};

static DEFAULT_ADAPTERS_REGISTERED: OnceLock<()> = OnceLock::new();

pub fn register_default_adapters() {
    DEFAULT_ADAPTERS_REGISTERED.get_or_init(|| {
        register_api_provider(Arc::new(AnthropicMessagesAdapter::default()));
        register_api_provider(Arc::new(OpenAiResponsesAdapter::default()));
        register_api_provider(Arc::new(OpenAiCompletionsAdapter::default()));
        register_api_provider(Arc::new(OpenAiCodexResponsesAdapter::default()));
    });
}
