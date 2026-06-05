use std::{fs, thread};

use base64::Engine;
use echo::{ChatGptOAuth, ChatGptOAuthOptions, OAuthToken, PkceCodes, Provider, TokenStore};
use tempfile::TempDir;
use tiny_http::{Response, Server};

#[tokio::test]
async fn chatgpt_oauth_callback_refresh_logout_and_redaction_use_private_token_store() {
    let issuer = FakeIssuer::start();
    let temp = TempDir::new().unwrap();
    let store = TokenStore::new(temp.path().join("tokens"));
    let oauth = ChatGptOAuth::new(ChatGptOAuthOptions {
        issuer: issuer.url.clone(),
        client_id: "test-client".to_string(),
        port: 0,
        open_browser: false,
        token_store: store.clone(),
    });

    let server = oauth
        .start_login_server_with(
            PkceCodes {
                code_verifier: "verifier".to_string(),
                code_challenge: "challenge".to_string(),
            },
            "state".to_string(),
        )
        .unwrap();

    let callback = format!(
        "http://localhost:{}/auth/callback?code=auth-code&state=state",
        server.actual_port
    );
    reqwest::get(callback).await.unwrap();
    server.wait().await.unwrap();

    let path = temp.path().join("tokens").join("openai-chatgpt.json");
    let token = store.load(Provider::OpenaiChatgpt).unwrap().unwrap();
    assert_eq!(token.id_token, "id-token");
    assert_eq!(token.access_token, jwt_with_exp(111));
    assert_eq!(token.refresh_token, "refresh-token");
    assert_eq!(token.expires_at, 111);
    assert_private(&path);

    let refreshed = store
        .load_refreshing(
            Provider::OpenaiChatgpt,
            std::time::Duration::from_secs(999),
            |token| {
                let oauth = oauth.clone();
                async move { oauth.refresh_token(&token).await }
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.id_token, "id-token");
    assert_eq!(refreshed.access_token, "access-token-2");
    assert_eq!(refreshed.refresh_token, "refresh-token");
    assert_eq!(refreshed.expires_at, 222);
    assert_private(&path);

    let redacted = echo::resolved_config_view().unwrap();
    let rendered = serde_json::to_string(&redacted).unwrap();
    assert!(!rendered.contains("id-token"));
    assert!(!rendered.contains("access-token-2"));
    assert!(!rendered.contains("refresh-token"));

    oauth.logout().await.unwrap();
    assert!(!path.exists());
}

struct FakeIssuer {
    url: String,
}

impl FakeIssuer {
    fn start() -> Self {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        thread::spawn(move || {
            for _ in 0..3 {
                let mut request = server.recv().unwrap();
                let path = request.url().to_string();
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();

                let response =
                    if path == "/oauth/token" && body.contains("grant_type=authorization_code") {
                        Response::from_string(
                            serde_json::json!({
                                "idToken": "id-token",
                                "accessToken": jwt_with_exp(111),
                                "refreshToken": "refresh-token"
                            })
                            .to_string(),
                        )
                    } else if path == "/oauth/token" && body.contains("refresh_token") {
                        Response::from_string(
                            serde_json::json!({
                                "accessToken": "access-token-2",
                                "expiresAt": 222
                            })
                            .to_string(),
                        )
                    } else if path == "/oauth/revoke" {
                        Response::from_string("{}")
                    } else {
                        Response::from_string("not found").with_status_code(404)
                    };

                request.respond(response).unwrap();
            }
        });
        Self { url }
    }
}

fn jwt_with_exp(exp: i64) -> String {
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::json!({ "exp": exp }).to_string());
    format!("{header}.{payload}.sig")
}

#[cfg(unix)]
fn assert_private(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(not(unix))]
fn assert_private(_path: &std::path::Path) {}

#[allow(dead_code)]
fn _assert_token_shape(_token: OAuthToken) {}
