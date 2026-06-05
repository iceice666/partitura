use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Error, Options, Provider, Result};

#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(REDACTED)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("REDACTED")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub repl: ReplConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub prompt_caching: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplConfig {
    pub prompt_prefix: String,
    pub reply_prefix: String,
    pub streaming: bool,
    pub color: bool,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            prompt_prefix: "you".to_string(),
            reply_prefix: "echo".to_string(),
            streaming: true,
            color: std::env::var_os("NO_COLOR").is_none(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    ApiKey(Secret),
    OAuthToken(Secret),
}

pub fn load_config() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        return Ok(Config::default());
    }
    ensure_private_file(&path)?;
    let body = fs::read_to_string(path)?;
    Ok(toml::from_str(&body)?)
}

pub fn write_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body = toml::to_string_pretty(config)?;
    fs::write(&path, body)?;
    set_private_file(&path)?;
    Ok(())
}

pub fn resolve_default_model(config: &Config) -> Option<String> {
    std::env::var("ECHO_MODEL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| config.default_model.clone())
}

pub fn resolve_openai_org_id(config: &Config) -> Option<String> {
    std::env::var("OPENAI_ORG_ID")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            config
                .providers
                .get(Provider::Openai.config_key())
                .and_then(|provider| provider.org_id.clone())
        })
}

pub fn resolve_credential(provider: Provider, opts: &Options) -> Result<Credential> {
    if let Some(api_key) = opts.api_key.clone() {
        return Ok(Credential::ApiKey(api_key));
    }

    if let Some(env_name) = provider.env_api_key()
        && let Ok(value) = std::env::var(env_name)
        && !value.is_empty()
    {
        return Ok(Credential::ApiKey(Secret::new(value)));
    }

    if provider == Provider::OpenaiChatgpt
        && let Some(token) = read_token(provider)?
    {
        return Ok(Credential::OAuthToken(token));
    }

    let config = load_config()?;
    if let Some(value) = config
        .providers
        .get(provider.config_key())
        .and_then(|provider| provider.api_key.clone())
    {
        return Ok(Credential::ApiKey(Secret::new(value)));
    }

    Err(Error::NoCredentials {
        provider: provider.to_string(),
    })
}

pub fn credential_status(provider: Provider) -> &'static str {
    match resolve_credential(provider, &Options::default()) {
        Ok(_) => "resolved",
        Err(_) => "missing",
    }
}

pub fn resolved_config_view() -> Result<serde_json::Value> {
    let config = load_config()?;
    let providers = config
        .providers
        .iter()
        .map(|(key, value)| {
            let mut object = serde_json::to_value(value).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(map) = object.as_object_mut()
                && map.contains_key("apiKey")
            {
                map.insert("apiKey".to_string(), serde_json::json!("REDACTED"));
            }
            (key.clone(), object)
        })
        .collect::<serde_json::Map<_, _>>();

    Ok(serde_json::json!({
        "defaultModel": resolve_default_model(&config),
        "openaiOrgId": resolve_openai_org_id(&config),
        "providers": providers,
        "repl": config.repl,
    }))
}

pub async fn login(provider: Provider) -> Result<()> {
    match provider {
        Provider::OpenaiChatgpt => {
            crate::ChatGptOAuth::new(crate::ChatGptOAuthOptions::default())
                .login()
                .await
        }
        _ => Err(Error::Provider(format!(
            "provider {provider} does not support OAuth login"
        ))),
    }
}

pub async fn logout(provider: Provider) -> Result<()> {
    if provider == Provider::OpenaiChatgpt {
        let oauth = crate::ChatGptOAuth::new(crate::ChatGptOAuthOptions {
            token_store: TokenStore::default(),
            open_browser: false,
            ..Default::default()
        });
        oauth.logout().await?;
        return Ok(());
    }

    TokenStore::default().logout(provider)?;
    Ok(())
}

fn read_token(provider: Provider) -> Result<Option<Secret>> {
    Ok(TokenStore::default()
        .load(provider)?
        .map(|token| Secret::new(token.access_token)))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OAuthToken {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub last_refresh: DateTime<Utc>,
}

impl OAuthToken {
    pub fn expires_within(&self, duration: Duration) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.expires_at <= now + duration.as_secs() as i64
    }

    pub fn merged_refresh(&self, refresh: crate::OAuthRefreshTokens) -> Self {
        Self {
            id_token: refresh.id_token.unwrap_or_else(|| self.id_token.clone()),
            access_token: refresh
                .access_token
                .unwrap_or_else(|| self.access_token.clone()),
            refresh_token: refresh
                .refresh_token
                .unwrap_or_else(|| self.refresh_token.clone()),
            expires_at: refresh.expires_at.unwrap_or(self.expires_at),
            last_refresh: Utc::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TokenStore {
    root: PathBuf,
}

impl Default for TokenStore {
    fn default() -> Self {
        Self {
            root: config_path()
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("tokens"),
        }
    }
}

impl TokenStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn load(&self, provider: Provider) -> Result<Option<OAuthToken>> {
        let path = self.path(provider);
        if !path.exists() {
            return Ok(None);
        }
        ensure_private_file(&path)?;
        let body = fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&body)?))
    }

    pub fn save(&self, provider: Provider, token: &OAuthToken) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let path = self.path(provider);
        fs::write(&path, serde_json::to_string_pretty(token)? + "\n")?;
        set_private_file(&path)?;
        Ok(())
    }

    pub fn logout(&self, provider: Provider) -> Result<()> {
        let path = self.path(provider);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub async fn load_refreshing<F, Fut>(
        &self,
        provider: Provider,
        refresh_before: Duration,
        refresh: F,
    ) -> Result<Option<OAuthToken>>
    where
        F: FnOnce(OAuthToken) -> Fut,
        Fut: std::future::Future<Output = Result<OAuthToken>>,
    {
        let Some(token) = self.load(provider)? else {
            return Ok(None);
        };

        if !token.expires_within(refresh_before) {
            return Ok(Some(token));
        }

        let refreshed = refresh(token).await?;
        self.save(provider, &refreshed)?;
        Ok(Some(refreshed))
    }

    fn path(&self, provider: Provider) -> PathBuf {
        self.root.join(format!("{provider}.json"))
    }
}

fn config_path() -> PathBuf {
    std::env::var_os("ECHO_CONFIG")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                PathBuf::from(home)
                    .join(".config")
                    .join("echo")
                    .join("config.toml")
            })
        })
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

#[cfg(unix)]
fn ensure_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)?.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(Error::UnsafePermissions {
            path: path.display().to_string(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_private_file(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file(_path: &Path) -> Result<()> {
    Ok(())
}
