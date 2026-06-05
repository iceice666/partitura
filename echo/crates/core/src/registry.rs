use std::sync::LazyLock;

use crate::{Cost, Model, Provider, ThinkingLevel, Usage};

static MODELS: LazyLock<Vec<Model>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../model-registry/snapshot.json"))
        .expect("vendored model registry snapshot must be valid")
});

pub fn get_model(provider: Provider, id: &str) -> Option<Model> {
    MODELS
        .iter()
        .find(|model| model.provider == provider && model.id == id)
        .cloned()
}

pub fn get_models(provider: Provider) -> Vec<Model> {
    MODELS
        .iter()
        .filter(|model| model.provider == provider)
        .cloned()
        .collect()
}

pub fn get_providers() -> Vec<Provider> {
    vec![
        Provider::Anthropic,
        Provider::Openai,
        Provider::OpenaiChatgpt,
    ]
}

pub fn calculate_cost(model: &Model, usage: &Usage) -> Cost {
    let cost = Cost {
        input: usage.input as f64 * model.cost.input,
        output: usage.output as f64 * model.cost.output,
        cache_read: usage.cache_read as f64 * model.cost.cache_read,
        cache_write: usage.cache_write as f64 * model.cost.cache_write,
        total: 0.0,
    };
    Cost {
        total: cost.input + cost.output + cost.cache_read + cost.cache_write,
        ..cost
    }
}

pub fn clamp_thinking_level(model: &Model, level: ThinkingLevel) -> ThinkingLevel {
    if model.thinking_levels.contains(&level) {
        return level;
    }

    *model
        .thinking_levels
        .iter()
        .min_by_key(|supported| {
            let lhs = level as i32;
            let rhs = **supported as i32;
            (lhs - rhs).abs()
        })
        .unwrap_or(&ThinkingLevel::Off)
}
