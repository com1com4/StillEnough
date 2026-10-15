import type { Account, AlertEvent, QuotaMetric } from "./types";

/** Keep observed decimal text intact; only the widget hides an unknown currency. */
export function balanceText(metric: QuotaMetric, surface: "main" | "widget" = "main") {
  if (metric.remaining === null) return null;
  if (metric.unit !== "currency_unknown") return metric.remaining;
  return surface === "widget" ? metric.remaining : `${metric.remaining} · 未知币种`;
}

export function balanceLabel(metric: QuotaMetric) {
  return metricLabel(metric.label.replace(/（(?:币种未提供|未知币种)）/g, ""));
}

export function refreshState(account: Account, now = Date.now()) {
  if (!account.enabled) return { disabled: true, label: "采集已暂停" };
  if (account.refreshing) return { disabled: true, label: "正在刷新…" };
  const attempt = Date.parse(account.lastAttemptAt ?? "");
  const retry = Date.parse(account.nextRetryAt ?? "");
  if (account.fetchStatus === "rate_limited" && retry > now)
    return { disabled: true, label: `平台限流 · ${Math.ceil((retry - now) / 60000)} 分钟后可刷新` };
  if (attempt + 30000 > now)
    return {
      disabled: true,
      label:
        account.fetchStatus === "ok" ? "刚刚更新" : `${Math.ceil((attempt + 30000 - now) / 1000)} 秒后可重试`,
    };
  return { disabled: false, label: account.fetchStatus === "rate_limited" ? "平台限流 · 可重试" : "刷新" };
}

export function metricState(metric: QuotaMetric, now = Date.now()) {
  if (metric.kind === "quota" && metric.remainingPercent === null) return "unknown";
  if (metric.kind !== "quota" && metric.remaining === null) return "unknown";
  const observed = Date.parse(metric.observedAt);
  const reset = metric.resetsAt ? Date.parse(metric.resetsAt) : NaN;
  return !Number.isFinite(observed) ||
    now - observed > (metric.staleAfterSeconds ?? 1800) * 1000 ||
    (Number.isFinite(reset) && reset <= now)
    ? "stale"
    : "current";
}

export function resetLabel(value: string | null, now = Date.now()) {
  if (!value) return "未设置重置";
  const time = Date.parse(value);
  if (!Number.isFinite(time)) return "重置时间未知";
  const minutes = Math.ceil((time - now) / 60000);
  if (minutes <= 0) return "待更新";
  if (minutes < 1440) return `${Math.floor(minutes / 60)}h${minutes % 60}m`;
  return new Intl.DateTimeFormat("zh-CN", { month: "2-digit", day: "2-digit" }).format(time);
}

export function metricLabel(value: string) {
  return value
    .replace(/base_model_inference/g, "备用额度")
    .replace(/(\d+)\s*(?:小时|h)\s*(?:窗口|额度)?/gi, "$1小时额度")
    .replace(/每周额度|周窗口/g, "周额度")
    .replace(/每月额度|月窗口/g, "月额度");
}

export function orderedMetrics(metrics: QuotaMetric[]) {
  const rank = (metric: QuotaMetric) =>
    /_codex_primary(?:_window)?$/.test(metric.metricId)
      ? 0
      : /_codex_secondary(?:_window)?$/.test(metric.metricId)
        ? 1
        : 2;
  return [...metrics].sort((a, b) => rank(a) - rank(b));
}

/** Select the visible pool only; this never switches the model or activates Reserve. */
export function widgetMetrics(account: Account, now = Date.now()) {
  const ordered = orderedMetrics(account.metrics);
  const reserve = ordered.filter((m) => /_base_model_inference_/.test(m.metricId));
  if (!reserve.length) return ordered;
  const main = ordered.filter((m) => /_codex_(primary|secondary)(?:_window)?$/.test(m.metricId));
  const exhausted =
    account.fetchStatus === "ok" &&
    main.some((m) => metricState(m, now) === "current" && m.remainingPercent === 0);
  const freshReserve = reserve.filter((m) => metricState(m, now) === "current");
  if (exhausted && freshReserve.length) return freshReserve;
  return ordered.filter((m) => !reserve.includes(m));
}

export function shortLabel(value: string) {
  return metricLabel(value)
    .replace(/(\d+)小时额度/g, "$1h")
    .replace(/周额度/g, "周")
    .replace(/月额度/g, "月");
}

export function fetchStatusLabel(status: string): string {
  const labels: Record<string, string> = {
    ok: "已更新",
    paused: "采集已暂停",
    auth_required: "需更新连接凭据",
    cli_authorization_required: "需授权本地登录来源查询",
    codex_cli_not_found: "未找到官方 Codex CLI，请安装后重启应用",
    codex_cli_incompatible: "Codex CLI 不支持所需协议或版本校验失败，请更新官方 CLI",
    local_login_required: "本地客户端尚未登录，请先登录订阅账号",
    local_login_expired: "本地订阅登录已失效，请通过官方客户端重新登录后重试",
    local_credentials_unavailable: "无法读取本地订阅登录，请检查官方客户端登录和配置目录",
    claude_subscription_required: "本地来源没有 Claude 订阅 OAuth 登录；API Key 不能查询订阅额度",
    codex_account_unavailable: "本地登录缺少有效账号标识，请重新登录或使用 app-server 方式",
    codex_account_mismatch: "返回账号与本地登录不一致，已停止更新，请重新确认登录来源",
    codex_usage_blocked: "服务端限制当前额度使用，可能涉及窗口、工作区额度或成员预算；请查看官方用量页面",
    codex_subscription_required: "当前 Codex 登录不是 ChatGPT 订阅账号",
    cli_not_found: "未在 PATH 中找到百炼 CLI；请检查安装与环境变量",
    cli_install_invalid: "百炼 CLI 安装不完整或 npm 入口无效，请修复安装",
    cli_node_missing: "已找到 npm CLI，但未找到 Node.js；请配置 Node 的 PATH",
    cli_node_incompatible: "Node.js 版本不兼容；百炼 CLI 至少需要 18.17.0",
    cli_launch_failed: "已找到 CLI，但无法启动；请检查文件权限与运行环境",
    cli_identity_mismatch: "找到的命令无法确认为百炼 CLI，请检查 PATH 中同名命令",
    cli_version_unsupported: "CLI 缺少所需 Token Plan 命令或参数，请更新百炼 CLI",
    cli_detection_timeout: "CLI 检测超时；请检查 PATH 中的 CLI 是否能正常启动",
    cli_quota_unavailable: "CLI 未返回可用 Token Plan 额度；请核对当前登录账号、套餐类型及百炼控制台",
    cli_auth_required: "CLI 未登录或登录已失效，请运行 bl auth login --console",
    cli_query_failed: "CLI 查询失败，请在 PowerShell 运行 bl usage token-plan --output json 检查登录与权限",
    cli_timeout: "CLI 查询超时，请检查网络后重试",
    permission_denied: "权限不足",
    rate_limited: "平台限流 · 等待重试",
    network_error: "网络读取失败，请检查目标服务和当前代理",
    network_timeout: "查询超时，请检查网络和代理后重试",
    proxy_auth_required: "代理要求账号密码认证，当前版本暂不支持",
    proxy_config_invalid: "网络配置无效，请检查查询网络设置",
    cli_proxy_unsupported: "当前本地客户端不支持所选代理方式，请检查该产品的网络说明",
    schema_changed: "接口字段变化",
    unsupported: "尚未支持",
    redirect_rejected: "已拒绝接口重定向",
  };
  return labels[status] ?? status;
}

export function metricSourceLabel(source: QuotaMetric["source"]): string {
  return source === "client_endpoint" ? "客户端接口（可能变化）" : "官方 API";
}

/** Only one reminder is raised per drop; the rest of the crossed thresholds are listed. */
export function alertHeadline(event: AlertEvent): string {
  return `${metricLabel(event.metricLabel)} 剩余 ${Math.round(event.remainingPercent * 10) / 10}%，已跌破 ${
    event.threshold
  }%`;
}

export function alertDetail(event: AlertEvent): string {
  const others = event.crossedThresholds.filter((value) => value !== event.threshold);
  const crossed = others.length ? `同次跨过 ${others.join("%、")}%` : "";
  return [
    event.productName,
    event.alias,
    crossed,
    event.periodKey.startsWith("fixed") ? "固定周期额度" : "滚动周期额度",
  ]
    .filter(Boolean)
    .join(" · ");
}

export function alertTime(value: string, now = Date.now()): string {
  const at = Date.parse(value);
  if (!Number.isFinite(at)) return "时间未知";
  const minutes = Math.floor((now - at) / 60000);
  if (minutes < 1) return "刚刚";
  if (minutes < 60) return `${minutes} 分钟前`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)} 小时前`;
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(at);
}
