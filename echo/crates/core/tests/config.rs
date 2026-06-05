use echo::{
    Config, Options, Provider, Secret, resolve_credential, resolve_default_model,
    resolve_openai_org_id,
};
use std::{env, fs, sync::Mutex};
use tempfile::TempDir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn secret_debug_and_display_are_redacted() {
    let secret = Secret::new("plaintext");
    assert!(!format!("{secret:?}").contains("plaintext"));
    assert!(!format!("{secret}").contains("plaintext"));
}

#[test]
fn env_credentials_precede_config_file() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp = TempDir::new().unwrap();
    let config = temp.path().join("config.toml");
    fs::write(
        &config,
        r#"
[providers.anthropic]
api_key = "from-config"
"#,
    )
    .unwrap();
    set_private(&config);

    unsafe {
        env::set_var("ECHO_CONFIG", &config);
        env::set_var("ANTHROPIC_API_KEY", "from-env");
    }

    let credential = resolve_credential(Provider::Anthropic, &Options::default()).unwrap();
    match credential {
        echo::Credential::ApiKey(secret) => assert_eq!(secret.expose(), "from-env"),
        _ => panic!("expected api key"),
    }

    unsafe {
        env::remove_var("ECHO_CONFIG");
        env::remove_var("ANTHROPIC_API_KEY");
    }
}

#[test]
fn write_config_creates_private_file_on_first_write() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp = TempDir::new().unwrap();
    let config = temp.path().join("nested").join("config.toml");
    unsafe {
        env::set_var("ECHO_CONFIG", &config);
    }

    echo::write_config(&Config {
        default_model: Some("anthropic/claude-opus-4-8".to_string()),
        ..Default::default()
    })
    .unwrap();

    assert!(config.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    unsafe {
        env::remove_var("ECHO_CONFIG");
    }
}

#[test]
fn env_model_and_openai_org_override_config_values() {
    let _lock = ENV_LOCK.lock().unwrap();
    let mut config = Config {
        default_model: Some("anthropic/claude-opus-4-8".to_string()),
        ..Default::default()
    };
    config.providers.insert(
        "openai".to_string(),
        echo::ProviderConfig {
            org_id: Some("org-config".to_string()),
            ..Default::default()
        },
    );

    unsafe {
        env::set_var("ECHO_MODEL", "openai/gpt-5");
        env::set_var("OPENAI_ORG_ID", "org-env");
    }

    assert_eq!(resolve_default_model(&config).unwrap(), "openai/gpt-5");
    assert_eq!(resolve_openai_org_id(&config).unwrap(), "org-env");

    unsafe {
        env::remove_var("ECHO_MODEL");
        env::remove_var("OPENAI_ORG_ID");
    }
}

#[test]
fn missing_credentials_fail_before_network_surface() {
    let _lock = ENV_LOCK.lock().unwrap();
    let temp = TempDir::new().unwrap();
    let config = temp.path().join("missing.toml");
    unsafe {
        env::set_var("ECHO_CONFIG", &config);
        env::remove_var("ANTHROPIC_API_KEY");
    }

    let err = resolve_credential(Provider::Anthropic, &Options::default()).unwrap_err();
    assert!(matches!(err, echo::Error::NoCredentials { .. }));

    unsafe {
        env::remove_var("ECHO_CONFIG");
    }
}

#[cfg(unix)]
#[test]
fn world_readable_config_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let _lock = ENV_LOCK.lock().unwrap();
    let temp = TempDir::new().unwrap();
    let config = temp.path().join("config.toml");
    fs::write(&config, "default_model = \"anthropic/claude-opus-4-8\"\n").unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o644)).unwrap();

    unsafe {
        env::set_var("ECHO_CONFIG", &config);
    }

    let err = echo::load_config().unwrap_err();
    assert!(matches!(err, echo::Error::UnsafePermissions { .. }));

    unsafe {
        env::remove_var("ECHO_CONFIG");
    }
}

#[cfg(unix)]
fn set_private(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn set_private(_path: &std::path::Path) {}
