use std::io::{self, Write};
use stillenough_core::{
    errors,
    protocol::Response,
    providers,
    request::{self, Command, Request},
    storage, transport,
};
fn local(request: &Request) -> Response {
    let mut r = Response::local(request.command.name(), request.command.kind());
    r.request_id = request.request_id.clone();
    if request.command == Command::Version {
        for (k, v) in [
            ("cliVersion", env!("CARGO_PKG_VERSION")),
            ("coreVersion", stillenough_core::CORE_VERSION),
            (
                "protocolVersion",
                stillenough_core::protocol::PROTOCOL_VERSION,
            ),
        ] {
            r.data.metadata.insert(k.into(), v.into());
        }
    } else {
        r.data.providers = providers::descriptors()
            .into_iter()
            .filter(|p| request.provider.as_deref().is_none_or(|id| id == p.id))
            .map(Into::into)
            .collect();
    }
    r
}
#[cfg(windows)]
fn path() -> Result<std::path::PathBuf, &'static str> {
    stillenough_core::windows::database_path()
}
#[cfg(not(windows))]
fn path() -> Result<std::path::PathBuf, &'static str> {
    Err("unsupported")
}
async fn execute(request: &Request) -> Response {
    if matches!(request.command, Command::Version | Command::Providers) {
        return local(request);
    }
    let database = match path() {
        Ok(p) => p,
        Err(c) => return failed(request, c),
    };
    if request.offline {
        return read_offline(database, request).await;
    }
    match transport::query(database.parent().unwrap_or(&database), request).await {
        Ok(r) => r,
        Err("desktop_unavailable")
            if matches!(
                request.command,
                Command::Accounts | Command::Quota | Command::Doctor
            ) =>
        {
            let mut r = read_offline(database, request).await;
            r.warnings.push("desktop_unavailable".into());
            r
        }
        Err(c) => {
            let mut r = failed(request, c);
            if c == "deadline_exceeded" && request.refreshes() {
                r.data.operation_id = Some(request.request_id.clone());
                r.warnings.push("operation_state_unknown".into());
            }
            r
        }
    }
}
async fn read_offline(path: std::path::PathBuf, request: &Request) -> Response {
    let left = chrono::DateTime::parse_from_rfc3339(&request.deadline)
        .ok()
        .and_then(|d| d.signed_duration_since(chrono::Utc::now()).to_std().ok())
        .unwrap_or_default();
    let copy = request.clone();
    match tokio::time::timeout(
        left,
        tokio::task::spawn_blocking(move || storage::offline(&path, &copy)),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => failed(request, "internal_error"),
        Err(_) => failed(request, "deadline_exceeded"),
    }
}
fn failed(request: &Request, code: &str) -> Response {
    let mut r = Response::local(request.command.name(), request.command.kind());
    r.request_id = request.request_id.clone();
    errors::fail(&mut r, code);
    r
}
fn cell(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}
fn render(r: &Response) -> String {
    let mut text = String::new();
    if r.data.kind == "version" && r.outcome == "ok" {
        text.push_str("组件\t版本\n");
        for (k, v) in &r.data.metadata {
            text.push_str(&format!("{}\t{}\n", cell(k), cell(v)));
        }
    }
    if r.data.kind == "providers" {
        text.push_str("ID\t名称\t支持\t能力\t状态\t限制\n");
        for p in &r.data.providers {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                cell(&p.id),
                cell(&p.name),
                p.supported,
                cell(&p.capability),
                cell(&p.status),
                cell(&p.limitation)
            ));
        }
    }
    if !r.data.accounts.is_empty() {
        text.push_str(
            "账号 ID\t产品\t别名\t账号状态\t指标\t数值\t单位\t新鲜度\t观测时间\t重置时间\n",
        );
        for a in &r.data.accounts {
            let status = serde_json::to_value(a.status).unwrap_or_default();
            let status = status.as_str().unwrap_or("unknown");
            if a.metrics.is_empty() {
                text.push_str(&format!(
                    "{}\t{}\t{}\t{}\t未请求或无指标\n",
                    cell(&a.id),
                    cell(&a.provider_id),
                    cell(&a.alias),
                    status
                ));
            }
            for m in &a.metrics {
                let percent = serde_json::to_value(&m.remaining_percent).unwrap_or_default();
                let value = if matches!(m.kind, stillenough_core::protocol::MetricKind::Balance) {
                    m.remaining.clone().unwrap_or("未知".into())
                } else {
                    percent
                        .as_f64()
                        .map(|p| format!("{p}%"))
                        .unwrap_or("未知".into())
                };
                let fresh = match m.freshness {
                    stillenough_core::protocol::Freshness::Current => "当前",
                    stillenough_core::protocol::Freshness::Unknown => "未知",
                    stillenough_core::protocol::Freshness::Expired => "旧观测",
                };
                text.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    cell(&a.id),
                    cell(&a.provider_id),
                    cell(&a.alias),
                    status,
                    cell(&m.label),
                    cell(&value),
                    cell(&m.unit),
                    fresh,
                    cell(&m.observed_at),
                    cell(m.resets_at.as_deref().unwrap_or("未知"))
                ));
            }
        }
    }
    if let Some(id) = &r.data.operation_id {
        text.push_str(&format!(
            "操作 ID\t{}\t{}\n",
            cell(id),
            cell(r.data.operation_state.as_deref().unwrap_or("unknown"))
        ));
    }
    for c in &r.data.checks {
        text.push_str(&format!(
            "检查\t{}\t{}\t{}\n",
            cell(&c.name),
            cell(&c.state),
            cell(c.code.as_deref().unwrap_or(""))
        ));
    }
    for e in &r.errors {
        text.push_str(&format!("错误\t{}\t{}\n", cell(&e.code), cell(&e.message)));
    }
    for warning in &r.warnings {
        text.push_str(&format!("提示\t{}\n", cell(warning)));
    }
    if text.is_empty() {
        text.push_str("无匹配账号\n");
    }
    text
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let (text, code) = if args == ["--help"] {
        ("StillEnough CLI\n命令：version | providers [--provider ID] | accounts list [--provider ID] [--offline] | quota [--account ID | --provider ID] [--offline] [--require-fresh] | refresh (--account ID | --all [--provider ID]) | operations get ID | doctor [--account ID] [--live | --offline]\n所有命令：--json --timeout 1-300（默认 30 秒）--no-color\n查询不隐式刷新；在桌面主窗口开启 Agent 权限。\n".into(),0)
    } else if args == ["--version"] {
        (
            format!("stillenough-cli {}\n", env!("CARGO_PKG_VERSION")),
            0,
        )
    } else {
        let json = args.iter().any(|a| a == "--json");
        let response = match request::parse(&args) {
            Ok(options) => execute(&options.request).await,
            Err(c) => {
                let mut r = Response::invalid_arguments();
                r.errors = vec![errors::safe_error(c, None)];
                r
            }
        };
        let code = errors::exit_code(&response);
        let text = if json {
            format!("{}\n", serde_json::to_string(&response).expect("safe DTO"))
        } else {
            render(&response)
        };
        (text, code)
    };
    if io::stdout().lock().write_all(text.as_bytes()).is_err() {
        std::process::exit(10);
    }
    std::process::exit(code);
}
