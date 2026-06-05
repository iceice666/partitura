use std::sync::Arc;

use echo::{
    Api, ApiProvider, Context, EventStream, HttpRequest, Options, Provider, ProviderCompat,
    register_api_provider,
};

#[derive(Debug)]
struct DummyProvider {
    api: Api,
    compat: ProviderCompat,
}

impl ApiProvider for DummyProvider {
    fn api(&self) -> Api {
        self.api
    }

    fn compat(&self) -> &ProviderCompat {
        &self.compat
    }

    fn build_request(
        &self,
        model: &echo::Model,
        _ctx: &Context,
        _opts: &Options,
    ) -> echo::Result<HttpRequest> {
        Ok(HttpRequest {
            method: "POST".to_string(),
            url: format!("{}/{}", model.base_url, model.id),
            headers: vec![(
                "x-reasoning-content".to_string(),
                self.compat
                    .requires_reasoning_content_on_assistant_messages
                    .to_string(),
            )],
            body: Vec::new(),
        })
    }

    fn stream(
        &self,
        model: &echo::Model,
        _ctx: &Context,
        opts: &Options,
    ) -> echo::Result<EventStream> {
        Ok(echo::stream(model, &Context::default(), opts))
    }
}

#[test]
fn registry_dispatches_by_api_without_changing_caller_surface() {
    register_api_provider(Arc::new(DummyProvider {
        api: Api::OpenaiResponses,
        compat: ProviderCompat::default(),
    }));

    let adapter = echo::get_api_provider(Api::OpenaiResponses).unwrap();
    let model = echo::get_model(Provider::Openai, "gpt-5").unwrap();
    let request = adapter
        .build_request(&model, &Context::default(), &Options::default())
        .unwrap();
    assert_eq!(request.url, "https://api.openai.com/gpt-5");
}

#[test]
fn provider_quirk_is_compat_data_not_a_new_adapter() {
    register_api_provider(Arc::new(DummyProvider {
        api: Api::OpenaiCompletions,
        compat: ProviderCompat {
            requires_reasoning_content_on_assistant_messages: true,
            thinking_format: Some("reasoning_content".to_string()),
            max_tokens_field: Some("max_completion_tokens".to_string()),
        },
    }));

    let adapter = echo::get_api_provider(Api::OpenaiCompletions).unwrap();
    assert!(
        adapter
            .compat()
            .requires_reasoning_content_on_assistant_messages
    );
    assert_eq!(
        adapter.compat().max_tokens_field.as_deref(),
        Some("max_completion_tokens")
    );
}
