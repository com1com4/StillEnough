use crate::protocol::{Response, SafeError, Status};
/// 固定白名单；外部异常、响应体与本地路径不得透传。
pub fn classify(code: &str) -> (&'static str, Status, i32) {
    use Status::*;
    match code {
        "ok" => ("ok", Ok, 0),
        "idle" | "unknown" => ("unknown", Unknown, 0),
        "cli_quota_unavailable" => ("cli_quota_unavailable", Unknown, 5),
        "invalid_arguments" => ("invalid_arguments", Error, 2),
        "desktop_unavailable" => ("desktop_unavailable", Unknown, 3),
        "not_initialized" => ("not_initialized", Unknown, 3),
        "cache_unavailable" => ("cache_unavailable", Error, 3),
        "agent_access_denied" => ("agent_access_denied", Forbidden, 4),
        "local_login_expired" => ("local_login_expired", Expired, 4),
        "auth_required" => ("auth_required", AuthRequired, 4),
        "cli_auth_required" => ("cli_auth_required", AuthRequired, 4),
        "local_login_required" => ("local_login_required", AuthRequired, 4),
        "local_credentials_unavailable" => ("local_credentials_unavailable", AuthRequired, 4),
        "cli_authorization_required" => ("cli_authorization_required", Forbidden, 4),
        "permission_denied" => ("permission_denied", Forbidden, 5),
        "codex_usage_blocked" => ("codex_usage_blocked", Forbidden, 5),
        "schema_changed" => ("schema_changed", ApiChanged, 5),
        "unsupported" => ("unsupported", Unsupported, 5),
        "claude_subscription_required" => ("claude_subscription_required", Unsupported, 5),
        "codex_account_mismatch" => ("codex_account_mismatch", Error, 10),
        "codex_account_unavailable" => ("codex_account_unavailable", Unknown, 5),
        "codex_subscription_required" => ("codex_subscription_required", Unsupported, 5),
        "codex_cli_incompatible" => ("codex_cli_incompatible", Unsupported, 5),
        "cli_version_unsupported" => ("cli_version_unsupported", Unsupported, 5),
        "cli_node_incompatible" => ("cli_node_incompatible", Unsupported, 5),
        "cli_proxy_unsupported" => ("cli_proxy_unsupported", Unsupported, 5),
        "cli_not_found" => ("cli_not_found", Unsupported, 5),
        "codex_cli_not_found" => ("codex_cli_not_found", Unsupported, 5),
        "rate_limited" => ("rate_limited", RateLimited, 6),
        "refresh_cooldown" => ("refresh_cooldown", Busy, 6),
        "refresh_in_progress" => ("refresh_in_progress", Busy, 6),
        "account_disabled" => ("account_disabled", Paused, 6),
        "database_busy" => ("database_busy", Busy, 6),
        "deadline_exceeded" => ("deadline_exceeded", NetworkError, 7),
        "network_error" => ("network_error", NetworkError, 7),
        "network_timeout" => ("network_timeout", NetworkError, 7),
        "cli_timeout" => ("cli_timeout", NetworkError, 7),
        "cli_detection_timeout" => ("cli_detection_timeout", NetworkError, 7),
        "proxy_config_invalid" => ("proxy_config_invalid", NetworkError, 7),
        "proxy_auth_required" => ("proxy_auth_required", NetworkError, 7),
        "redirect_rejected" => ("redirect_rejected", NetworkError, 7),
        "partial_failure" => ("partial_failure", Error, 8),
        "version_mismatch" => ("version_mismatch", Error, 9),
        "schema_incompatible" => ("schema_incompatible", Error, 9),
        "migration_required" => ("migration_required", Error, 9),
        "owner_conflict" => ("owner_conflict", Error, 9),
        "database_error" => ("database_error", Error, 10),
        "internal_error" => ("internal_error", Error, 10),
        "cli_identity_mismatch" => ("cli_identity_mismatch", Error, 10),
        "cli_launch_failed" => ("cli_launch_failed", Error, 10),
        "cli_query_failed" => ("cli_query_failed", Error, 10),
        "fresh_required" => ("fresh_required", Unknown, 11),
        "not_found" => ("not_found", Unknown, 12),
        "operation_lost" => ("operation_lost", Unknown, 12),
        _ => ("internal_error", Error, 10),
    }
}
pub fn safe_error(code: &str, account: Option<String>) -> SafeError {
    let (code, status, exit) = classify(code);
    SafeError {
        code: code.into(),
        status,
        scope: match exit {
            3 | 9 => "transport",
            4 => "local-policy",
            5..=7 => "provider",
            10 => "storage",
            _ => "request",
        }
        .into(),
        account_id: account,
        retryable: matches!(exit, 3 | 6 | 7),
        next_retry_at: None,
        message: match code {
            "invalid_arguments" => "参数无效；使用 --help 查看用法",
            "agent_access_denied" => "请在 StillEnough 主窗口设置中开启相应 Agent 权限",
            "desktop_unavailable" => "请启动 StillEnough 桌面后台",
            "fresh_required" => "所选指标包含未知或旧观测，请刷新后重试",
            "local_login_expired"
            | "auth_required"
            | "cli_auth_required"
            | "local_login_required"
            | "local_credentials_unavailable" => "请在主窗口核对连接并通过官方客户端重新登录",
            _ => "操作未完成；请根据错误码在主窗口核查",
        }
        .into(),
    }
}
pub fn fail(response: &mut Response, code: &str) {
    response.outcome = "error".into();
    response.errors.push(safe_error(code, None));
}
pub fn exit_code(r: &Response) -> i32 {
    if r.outcome == "partial" {
        return 8;
    }
    if r.outcome == "ok" {
        return 0;
    }
    let codes: Vec<_> = r.errors.iter().map(|e| classify(&e.code).2).collect();
    for c in [10, 9, 4, 5, 7, 6, 12, 3, 11, 2, 8] {
        if codes.contains(&c) {
            return c;
        }
    }
    10
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_messages_never_escape() {
        let e = safe_error("SECRET_PATH_AND_BODY", None);
        assert_eq!(e.code, "internal_error");
        assert!(!serde_json::to_string(&e).unwrap().contains("SECRET"));
        assert_eq!(classify("local_login_expired").1, Status::Expired);
        assert_eq!(classify("auth_required").1, Status::AuthRequired);
    }
}
