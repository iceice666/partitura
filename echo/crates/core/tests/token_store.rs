use echo::{OAuthToken, Provider, TokenStore};
use std::{fs, time::Duration};
use tempfile::TempDir;

#[tokio::test]
async fn refresh_before_expiry_rewrites_private_token_file() {
    let temp = TempDir::new().unwrap();
    let store = TokenStore::new(temp.path().to_path_buf());
    store
        .save(
            Provider::OpenaiChatgpt,
            &OAuthToken {
                id_token: "id-old".to_string(),
                access_token: "old".to_string(),
                refresh_token: "refresh-old".to_string(),
                expires_at: 0,
                last_refresh: chrono::Utc::now(),
            },
        )
        .unwrap();

    let token = store
        .load_refreshing(
            Provider::OpenaiChatgpt,
            Duration::from_secs(60),
            |mut token| async move {
                token.id_token = "id-new".to_string();
                token.access_token = "new".to_string();
                token.refresh_token = "refresh-new".to_string();
                token.expires_at = i64::MAX;
                Ok(token)
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(token.access_token, "new");
    assert_eq!(token.id_token, "id-new");
    assert_eq!(token.refresh_token, "refresh-new");
    let path = temp.path().join("openai-chatgpt.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn logout_clears_stored_token() {
    let temp = TempDir::new().unwrap();
    let store = TokenStore::new(temp.path().to_path_buf());
    store
        .save(
            Provider::OpenaiChatgpt,
            &OAuthToken {
                id_token: "id".to_string(),
                access_token: "token".to_string(),
                refresh_token: "refresh".to_string(),
                expires_at: i64::MAX,
                last_refresh: chrono::Utc::now(),
            },
        )
        .unwrap();
    store.logout(Provider::OpenaiChatgpt).unwrap();
    assert!(!temp.path().join("openai-chatgpt.json").exists());
}

#[cfg(unix)]
#[test]
fn world_readable_token_file_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let path = temp.path().join("openai-chatgpt.json");
    fs::write(
        &path,
        r#"{"idToken":"id","accessToken":"token","refreshToken":"refresh","expiresAt":9223372036854775807,"lastRefresh":"2026-06-05T00:00:00Z"}"#,
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

    let store = TokenStore::new(temp.path().to_path_buf());
    let err = store.load(Provider::OpenaiChatgpt).unwrap_err();
    assert!(matches!(err, echo::Error::UnsafePermissions { .. }));
}

#[test]
fn refresh_merge_preserves_omitted_token_fields() {
    let original = OAuthToken {
        id_token: "id".to_string(),
        access_token: "access".to_string(),
        refresh_token: "refresh".to_string(),
        expires_at: 10,
        last_refresh: chrono::Utc::now(),
    };

    let merged = original.merged_refresh(echo::OAuthRefreshTokens {
        id_token: None,
        access_token: Some("access-new".to_string()),
        refresh_token: None,
        expires_at: Some(20),
    });

    assert_eq!(merged.id_token, "id");
    assert_eq!(merged.access_token, "access-new");
    assert_eq!(merged.refresh_token, "refresh");
    assert_eq!(merged.expires_at, 20);
}
