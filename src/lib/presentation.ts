import type { Account, AlertEvent, QuotaMetric } from "./types";
import { t, currentLanguage } from "./i18n";
import { messages, type MessageKey } from "./locales";
import { providerName } from "./providerPresentation";

/** Keep observed decimal text intact; only the widget hides an unknown currency. */
export function balanceText(metric: QuotaMetric, surface: "main" | "widget" = "main") {
  if (metric.remaining === null) return null;
  if (metric.unit !== "currency_unknown") return metric.remaining;
  return surface === "widget" ? metric.remaining : t("balance.unknownCurrency", { amount: metric.remaining });
}

export function balanceLabel(metric: QuotaMetric) {
  return metricLabel(metric.label.replace(/（(?:币种未提供|未知币种)）/g, ""));
}

export function refreshState(account: Account, now = Date.now()) {
  if (!account.enabled) return { disabled: true, label: t("status.paused") };
  if (account.refreshing) return { disabled: true, label: t("refresh.busy") };
  const attempt = Date.parse(account.lastAttemptAt ?? "");
  const retry = Date.parse(account.nextRetryAt ?? "");
  if (account.fetchStatus === "rate_limited" && retry > now)
    return { disabled: true, label: t("refresh.rateWait", { minutes: Math.ceil((retry - now) / 60000) }) };
  if (attempt + 30000 > now)
    return {
      disabled: true,
      label:
        account.fetchStatus === "ok"
          ? t("refresh.justUpdated")
          : t("refresh.retrySeconds", { seconds: Math.ceil((attempt + 30000 - now) / 1000) }),
    };
  return {
    disabled: false,
    label: account.fetchStatus === "rate_limited" ? t("refresh.rateRetry") : t("refresh.action"),
  };
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
  if (!value) return t("reset.notSet");
  const time = Date.parse(value);
  if (!Number.isFinite(time)) return t("reset.unknown");
  const minutes = Math.ceil((time - now) / 60000);
  if (minutes <= 0) return t("reset.pending");
  if (minutes < 1440) return `${Math.floor(minutes / 60)}h${minutes % 60}m`;
  return new Intl.DateTimeFormat(currentLanguage(), { month: "2-digit", day: "2-digit" }).format(time);
}

export function metricLabel(value: string): string {
  const currencyNote = /^(可用金额|现金余额|可用余额)（(币种未提供|未知币种)）$/.exec(value);
  if (currencyNote) {
    return t(currencyNote[2] === "币种未提供" ? "metric.currencyNotProvided" : "metric.currencyUnknown", {
      label: metricLabel(currencyNote[1]),
    });
  }
  const normalized = value
    .replace(/base_model_inference/g, "备用额度")
    .replace(/(\d+)\s*(?:小时|h)\s*(?:窗口|额度)?/gi, "$1小时额度")
    .replace(/每周额度|周窗口/g, "周额度")
    .replace(/每月额度|月窗口/g, "月额度");
  // Only known source labels are translated; unknown names and model names survive.
  if (currentLanguage() !== "en") return normalized;
  const labels: Record<string, string> = {
    备用额度: t("metric.reserve"),
    周额度: t("metric.weekly"),
    月额度: t("metric.monthly"),
    可用余额: t("metric.availableBalance"),
    现金余额: t("metric.cashBalance"),
    可用金额: t("metric.availableAmount"),
    可用额度: t("metric.availableCredit"),
    账户余额: t("metric.accountBalance"),
    钱包余额: t("metric.wallet"),
    代金券余额: t("metric.voucherBalance"),
    "GLM Token 额度": t("metric.glmTokens"),
    "GLM 工具调用额度": t("metric.glmTools"),
    阿里云账户现金余额: t("metric.cloudCash"),
    "阿里云账户可用额度（含信用）": t("metric.cloudCredit"),
    主窗口: t("metric.primary"),
    次窗口: t("metric.secondary"),
  };
  if (labels[normalized]) return labels[normalized];
  const balance = /^可用余额( · .+)$/.exec(normalized);
  if (balance) return `${t("metric.availableBalance")}${balance[1]}`;
  // Translate only a complete reported window suffix; leave arbitrary names intact.
  const window = /^(.*?)(\d+小时额度|周额度|月额度|\d+(?:\.\d+)? 分钟窗口)$/.exec(normalized);
  if (!window) return normalized;
  const prefix = window[1]
    .replace(/^备用额度(?=\s|·|$)/, t("metric.reserve"))
    .replace(/^OAuth 应用$/, t("metric.oauthApps"));
  const hours = /^(\d+)小时额度$/.exec(window[2]);
  const minutes = /^(\d+(?:\.\d+)?) 分钟窗口$/.exec(window[2]);
  const suffix = hours
    ? t("metric.hours", { hours: hours[1] })
    : minutes
      ? t("metric.minuteWindow", { minutes: minutes[1] })
      : window[2] === "周额度"
        ? t("metric.weekly")
        : t("metric.monthly");
  return `${prefix}${prefix && !/[\s·]$/.test(prefix) ? " " : ""}${suffix}`;
}

/** Translate internal currency sentinels without guessing a real currency. */
export function currencyLabel(unit: string): string {
  return unit === "currency_unknown" ? t("currency.unknown") : unit;
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
    .replace(/(\d+)-hour quota/g, "$1h")
    .replace(/Weekly quota|周额度/g, t("metric.weekShort"))
    .replace(/Monthly quota|月额度/g, t("metric.monthShort"));
}

export function fetchStatusLabel(status: string): string {
  const key = `status.${status}` as MessageKey;
  if (Object.prototype.hasOwnProperty.call(messages, key)) return t(key);
  // Known application validation messages can be translated in either direction.
  const known = Object.entries(messages).find(([, pair]) => pair.some((text) => text === status));
  if (known) return t(known[0] as MessageKey);
  return status;
}

export function metricSourceLabel(source: QuotaMetric["source"]): string {
  return source === "client_endpoint" ? t("source.client") : t("source.official");
}

/** Only one reminder is raised per drop; the rest of the crossed thresholds are listed. */
export function alertHeadline(event: AlertEvent): string {
  return t("alert.headline", {
    label: metricLabel(event.metricLabel),
    percent: Math.round(event.remainingPercent * 10) / 10,
    threshold: event.threshold,
  });
}

export function alertDetail(event: AlertEvent): string {
  const others = event.crossedThresholds.filter((value) => value !== event.threshold);
  const crossed = others.length
    ? t("alert.crossed", { values: others.join(currentLanguage() === "en" ? "% / " : "%、") })
    : "";
  return [
    providerName(undefined, event.productName),
    event.alias,
    crossed,
    event.periodKey.startsWith("fixed") ? t("alert.fixed") : t("alert.rolling"),
  ]
    .filter(Boolean)
    .join(" · ");
}

export function alertTime(value: string, now = Date.now()): string {
  const at = Date.parse(value);
  if (!Number.isFinite(at)) return t("time.unknown");
  const minutes = Math.floor((now - at) / 60000);
  if (minutes < 1) return t("time.justNow");
  if (minutes < 60) return t("time.minutesAgo", { minutes });
  if (minutes < 1440) return t("time.hoursAgo", { hours: Math.floor(minutes / 60) });
  return new Intl.DateTimeFormat(currentLanguage(), {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(at);
}
