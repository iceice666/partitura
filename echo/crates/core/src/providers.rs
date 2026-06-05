use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use crate::{Api, Context, EventStream, Model, Options, Result};

pub trait ApiProvider: Send + Sync {
    fn api(&self) -> Api;
    fn compat(&self) -> &ProviderCompat;
    fn build_request(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<HttpRequest>;
    fn stream(&self, model: &Model, ctx: &Context, opts: &Options) -> Result<EventStream>;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCompat {
    pub requires_reasoning_content_on_assistant_messages: bool,
    pub thinking_format: Option<String>,
    pub max_tokens_field: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

type Registry = HashMap<Api, Arc<dyn ApiProvider>>;

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

pub fn register_api_provider(adapter: Arc<dyn ApiProvider>) {
    let registry = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
    registry
        .lock()
        .expect("api provider registry poisoned")
        .insert(adapter.api(), adapter);
}

pub fn get_api_provider(api: Api) -> Option<Arc<dyn ApiProvider>> {
    REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("api provider registry poisoned")
        .get(&api)
        .cloned()
}
