use std::process::{Command, Output};
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stillenough-cli"))
        .args(args)
        .env("OPENAI_API_KEY", "SECRET_SENTINEL_KEY_9b3")
        .env("CODEX_HOME", "SECRET_SENTINEL_PATH_9b3")
        .env("CLAUDE_CONFIG_DIR", "SECRET_SENTINEL_PATH_9b3")
        .env("HTTPS_PROXY", "SECRET_SENTINEL_PROXY_9b3")
        .output()
        .unwrap()
}
#[test]
fn local_commands_and_exit_codes_never_echo_secrets() {
    for args in [
        vec!["version", "--json"],
        vec!["providers", "--json"],
        vec!["--json", "providers", "--provider", "codex"],
        vec!["providers", "--no-color"],
        vec!["--help"],
        vec!["--version"],
    ] {
        let out = cli(&args);
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(!text.contains("SECRET_SENTINEL"));
        assert!(!text.contains('\u{1b}'));
        if args.contains(&"--json") {
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["schemaVersion"], 1);
            assert_eq!(v["transport"], "local");
            assert!(v["data"]["accounts"].as_array().unwrap().is_empty());
        }
    }
    for args in [
        vec![],
        vec!["unknown-command", "--json"],
        vec!["version", "--token", "SECRET_SENTINEL_ARG_9b3", "--json"],
        vec![
            "providers",
            "--provider",
            "SECRET_SENTINEL_PROVIDER_9b3",
            "--json",
        ],
        vec!["version", "--timeout", "0"],
        vec!["version", "--timeout", "301"],
        vec!["version", "--timeout"],
        vec!["version", "providers"],
        vec!["version", "--provider", "codex"],
        vec!["version", "--bogus"],
    ] {
        let out = cli(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stderr.is_empty());
        assert!(!String::from_utf8(out.stdout)
            .unwrap()
            .contains("SECRET_SENTINEL"));
    }
    for timeout in ["1", "30", "300"] {
        assert!(cli(&["version", "--timeout", timeout]).status.success());
    }
}
