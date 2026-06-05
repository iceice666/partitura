use std::{
    collections::HashMap,
    io,
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    sync::Arc,
    thread,
    time::Duration,
};

use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Response, Server};
use tokio::sync::{Notify, mpsc};

use crate::{Error, OAuthToken, Provider, Result, TokenStore};

pub const DEFAULT_CHATGPT_ISSUER: &str = "https://auth.openai.com";
pub const DEFAULT_CHATGPT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const DEFAULT_LOGIN_PORT: u16 = 1455;
pub const FALLBACK_LOGIN_PORT: u16 = 1457;
pub const CHATGPT_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";

#[derive(Debug, Clone)]
pub struct ChatGptOAuthOptions {
    pub issuer: String,
    pub client_id: String,
    pub port: u16,
    pub open_browser: bool,
    pub token_store: TokenStore,
}

impl Default for ChatGptOAuthOptions {
    fn default() -> Self {
        Self {
            issuer: std::env::var("ECHO_OPENAI_CHATGPT_ISSUER")
                .unwrap_or_else(|_| DEFAULT_CHATGPT_ISSUER.to_string()),
            client_id: std::env::var("ECHO_OPENAI_CHATGPT_CLIENT_ID")
                .unwrap_or_else(|_| DEFAULT_CHATGPT_CLIENT_ID.to_string()),
            port: DEFAULT_LOGIN_PORT,
            open_browser: true,
            token_store: TokenStore::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PkceCodes {
    pub code_verifier: String,
    pub code_challenge: String,
}

impl PkceCodes {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 64];
        rand::rng().fill_bytes(&mut bytes);
        let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let digest = Sha256::digest(code_verifier.as_bytes());
        let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        Self {
            code_verifier,
            code_challenge,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatGptOAuth {
    opts: ChatGptOAuthOptions,
    client: reqwest::Client,
}

impl ChatGptOAuth {
    pub fn new(opts: ChatGptOAuthOptions) -> Self {
        Self {
            opts,
            client: reqwest::Client::new(),
        }
    }

    pub async fn login(&self) -> Result<()> {
        let server = self.start_login_server()?;
        if self.opts.open_browser {
            let _ = webbrowser::open(&server.auth_url);
        }
        server.wait().await
    }

    pub fn start_login_server(&self) -> Result<LoginServer> {
        let pkce = PkceCodes::generate();
        let state = generate_state();
        self.start_login_server_with(pkce, state)
    }

    pub fn start_login_server_with(&self, pkce: PkceCodes, state: String) -> Result<LoginServer> {
        let server = bind_server(self.opts.port)?;
        let actual_port = match server.server_addr().to_ip() {
            Some(addr) => addr.port(),
            None => {
                return Err(Error::Provider(
                    "unable to determine login port".to_string(),
                ));
            }
        };
        let redirect_uri = format!("http://localhost:{actual_port}/auth/callback");
        let auth_url = build_authorize_url(
            &self.opts.issuer,
            &self.opts.client_id,
            &redirect_uri,
            &pkce,
            &state,
        );

        let server = Arc::new(server);
        let (tx, mut rx) = mpsc::channel::<tiny_http::Request>(16);
        let receive_server = server.clone();
        let receive_thread = thread::spawn(move || {
            while let Ok(request) = receive_server.recv() {
                if tx.blocking_send(request).is_err() {
                    break;
                }
            }
        });

        let shutdown = Arc::new(Notify::new());
        let shutdown_task = shutdown.clone();
        let server_for_unblock = server.clone();
        let oauth = self.clone();
        let handle = tokio::spawn(async move {
            let result = loop {
                tokio::select! {
                    _ = shutdown_task.notified() => {
                        break Err(Error::Provider("login was not completed".to_string()));
                    }
                    request = rx.recv() => {
                        let Some(request) = request else {
                            break Err(Error::Provider("login was not completed".to_string()));
                        };
                        let url = request.url().to_string();
                        match oauth.handle_callback(&url, &redirect_uri, &pkce, &state).await {
                            CallbackOutcome::Continue(response) => {
                                let _ = tokio::task::spawn_blocking(move || request.respond(response)).await;
                            }
                            CallbackOutcome::Done { response, result } => {
                                let _ = tokio::task::spawn_blocking(move || request.respond(response)).await;
                                break result;
                            }
                        }
                    }
                }
            };
            server_for_unblock.unblock();
            let _ = receive_thread.join();
            result
        });

        Ok(LoginServer {
            auth_url,
            actual_port,
            shutdown,
            handle,
        })
    }

    async fn handle_callback(
        &self,
        url_raw: &str,
        redirect_uri: &str,
        pkce: &PkceCodes,
        expected_state: &str,
    ) -> CallbackOutcome {
        let parsed = match url::Url::parse(&format!("http://localhost{url_raw}")) {
            Ok(parsed) => parsed,
            Err(_) => {
                return CallbackOutcome::Continue(
                    Response::from_string("Bad Request").with_status_code(400),
                );
            }
        };

        match parsed.path() {
            "/auth/callback" => {
                let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
                if params.get("state").map(String::as_str) != Some(expected_state) {
                    return CallbackOutcome::Done {
                        response: Response::from_string("State mismatch").with_status_code(400),
                        result: Err(Error::Provider("OAuth state mismatch".to_string())),
                    };
                }
                let Some(code) = params.get("code").filter(|code| !code.is_empty()) else {
                    return CallbackOutcome::Done {
                        response: Response::from_string("Missing authorization code")
                            .with_status_code(400),
                        result: Err(Error::Provider("missing authorization code".to_string())),
                    };
                };

                match self.exchange_code(redirect_uri, pkce, code).await {
                    Ok(token) => {
                        let result = self.opts.token_store.save(Provider::OpenaiChatgpt, &token);
                        CallbackOutcome::Done {
                            response: html_response("Login complete"),
                            result,
                        }
                    }
                    Err(err) => CallbackOutcome::Done {
                        response: Response::from_string("Token exchange failed")
                            .with_status_code(500),
                        result: Err(err),
                    },
                }
            }
            "/cancel" => CallbackOutcome::Done {
                response: Response::from_string("Login cancelled"),
                result: Err(Error::Provider("Login cancelled".to_string())),
            },
            _ => {
                CallbackOutcome::Continue(Response::from_string("Not Found").with_status_code(404))
            }
        }
    }

    pub async fn exchange_code(
        &self,
        redirect_uri: &str,
        pkce: &PkceCodes,
        code: &str,
    ) -> Result<OAuthToken> {
        let endpoint = format!("{}/oauth/token", self.opts.issuer.trim_end_matches('/'));
        let body = format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
            urlencoding::encode(code),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&self.opts.client_id),
            urlencoding::encode(&pkce.code_verifier),
        );
        let response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|err| Error::Provider(err.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Provider(format!(
                "token endpoint returned status {status}"
            )));
        }
        let tokens: OAuthCodeTokens = response
            .json()
            .await
            .map_err(|err| Error::Provider(err.to_string()))?;
        Ok(tokens.into_oauth_token())
    }

    pub async fn refresh_token(&self, token: &OAuthToken) -> Result<OAuthToken> {
        let endpoint = format!("{}/oauth/token", self.opts.issuer.trim_end_matches('/'));
        let response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/json")
            .json(&RefreshRequest {
                client_id: self.opts.client_id.as_str(),
                grant_type: "refresh_token",
                refresh_token: token.refresh_token.as_str(),
            })
            .send()
            .await
            .map_err(|err| Error::Provider(err.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Provider(format!(
                "token refresh returned status {status}"
            )));
        }
        let refresh: OAuthRefreshTokens = response
            .json()
            .await
            .map_err(|err| Error::Provider(err.to_string()))?;
        Ok(token.merged_refresh(refresh))
    }

    pub async fn logout(&self) -> Result<()> {
        let token = self.opts.token_store.load(Provider::OpenaiChatgpt)?;
        self.opts.token_store.logout(Provider::OpenaiChatgpt)?;
        if let Some(token) = token {
            let _ = self.revoke_token(&token).await;
        }
        Ok(())
    }

    pub async fn revoke_token(&self, token: &OAuthToken) -> Result<()> {
        let endpoint = format!("{}/oauth/revoke", self.opts.issuer.trim_end_matches('/'));
        let response = self
            .client
            .post(endpoint)
            .header("Content-Type", "application/json")
            .json(&RevokeRequest {
                token: token.refresh_token.as_str(),
                token_type_hint: "refresh_token",
                client_id: Some(self.opts.client_id.as_str()),
            })
            .send()
            .await
            .map_err(|err| Error::Provider(err.to_string()))?;
        if response.status().is_success() {
            return Ok(());
        }
        Err(Error::Provider(format!(
            "token revoke returned status {}",
            response.status()
        )))
    }
}

#[derive(Debug)]
pub struct LoginServer {
    pub auth_url: String,
    pub actual_port: u16,
    shutdown: Arc<Notify>,
    handle: tokio::task::JoinHandle<Result<()>>,
}

impl LoginServer {
    pub async fn wait(self) -> Result<()> {
        self.handle
            .await
            .map_err(|err| Error::Provider(format!("login server task failed: {err}")))?
    }

    pub fn cancel(&self) {
        self.shutdown.notify_one();
        let _ = send_cancel_request(self.actual_port);
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthCodeTokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
}

impl OAuthCodeTokens {
    pub fn into_oauth_token(self) -> OAuthToken {
        let expires_at = jwt_expiration(&self.access_token).unwrap_or(0);
        OAuthToken {
            id_token: self.id_token,
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at,
            last_refresh: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthRefreshTokens {
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Serialize)]
struct RefreshRequest<'a> {
    client_id: &'a str,
    grant_type: &'a str,
    refresh_token: &'a str,
}

#[derive(Serialize)]
struct RevokeRequest<'a> {
    token: &'a str,
    token_type_hint: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<&'a str>,
}

enum CallbackOutcome {
    Continue(Response<std::io::Cursor<Vec<u8>>>),
    Done {
        response: Response<std::io::Cursor<Vec<u8>>>,
        result: Result<()>,
    },
}

pub fn build_authorize_url(
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkceCodes,
    state: &str,
) -> String {
    let query = [
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scope", CHATGPT_SCOPE),
        ("code_challenge", pkce.code_challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
    ]
    .into_iter()
    .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
    .collect::<Vec<_>>()
    .join("&");
    format!("{}/oauth/authorize?{query}", issuer.trim_end_matches('/'))
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn bind_server(port: u16) -> Result<Server> {
    let preferred = format!("127.0.0.1:{port}");
    match Server::http(&preferred) {
        Ok(server) => Ok(server),
        Err(err) if port == DEFAULT_LOGIN_PORT && is_addr_in_use(err.as_ref()) => {
            let fallback = format!("127.0.0.1:{FALLBACK_LOGIN_PORT}");
            Server::http(&fallback).map_err(|err| Error::Provider(err.to_string()))
        }
        Err(err) => Err(Error::Provider(err.to_string())),
    }
}

fn is_addr_in_use(err: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    err.downcast_ref::<io::Error>()
        .is_some_and(|err| err.kind() == io::ErrorKind::AddrInUse)
}

fn send_cancel_request(port: u16) -> io::Result<()> {
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    use std::io::Write;
    stream.write_all(b"GET /cancel HTTP/1.1\r\n")?;
    stream.write_all(format!("Host: 127.0.0.1:{port}\r\n").as_bytes())?;
    stream.write_all(b"Connection: close\r\n\r\n")?;
    Ok(())
}

fn html_response(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let response = Response::from_string(body);
    match Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]) {
        Ok(header) => response.with_header(header),
        Err(_) => response,
    }
}

fn jwt_expiration(jwt: &str) -> Option<i64> {
    let mut parts = jwt.split('.');
    let (_, payload, _) = (parts.next()?, parts.next()?, parts.next()?);
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("exp").and_then(serde_json::Value::as_i64)
}

#[allow(dead_code)]
fn _codex_token_path_for_doc(_path: PathBuf) {}
