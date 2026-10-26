import { describe, expect, it } from "vitest";
import {
  balanceText,
  balanceLabel,
  widgetMetrics,
  shortLabel,
  metricLabel,
  orderedMetrics,
  alertDetail,
  alertHeadline,
  alertTime,
  metricState,
  resetLabel,
  refreshState,
  percentText,
  widgetMetricLabel,
  rotatingWidgetMetrics,
} from "./presentation";
import type { AlertEvent, QuotaMetric } from "./types";
import type { Account } from "./types";

describe("余额币种展示", () => {
  const metric: QuotaMetric = {
    metricId: "cash_balance",
    label: "现金余额（币种未提供）",
    kind: "balance",
    unit: "currency_unknown",
    remaining: "12.50",
    limit: null,
    remainingPercent: null,
    resetsAt: null,
    source: "client_endpoint",
    observedAt: "now",
  };
  it("主窗口提示未知币种，浮窗保留原数值，旧标签不重复提示", () => {
    expect(balanceText(metric)).toBe("12.50 · 未知币种");
    expect(balanceText(metric, "widget")).toBe("12.50");
    expect(balanceLabel(metric)).toBe("现金余额");
    expect(balanceText({ ...metric, remaining: "0.00" })).toBe("0.00 · 未知币种");
    expect(balanceText({ ...metric, remaining: "0.00" }, "widget")).toBe("0.00");
  });
  it("保留已知币种、金额精度和缺失状态", () => {
    const known = { ...metric, unit: "USD", remaining: "128.500 USD" };
    expect(balanceText(known)).toBe("128.500 USD");
    expect(balanceText(known, "widget")).toBe("128.500 USD");
    expect(balanceText({ ...metric, remaining: null })).toBeNull();
    expect(balanceText({ ...metric, remaining: null }, "widget")).toBeNull();
  });
});

describe("手工刷新状态", () => {
  const account: Account = {
    id: "a",
    providerId: "p",
    productName: "p",
    alias: "a",
    enabled: true,
    metrics: [],
    fetchStatus: "ok",
    lastSuccessAt: null,
  };
  const time = Date.parse("2026-10-01T06:00:00Z");
  it("自动刷新时间未到时，30秒之后仍允许手工刷新", () => {
    expect(
      refreshState(
        {
          ...account,
          lastAttemptAt: new Date(time - 31000).toISOString(),
          nextRetryAt: new Date(time + 600000).toISOString(),
        },
        time,
      ).disabled,
    ).toBe(false);
    expect(refreshState({ ...account, lastAttemptAt: new Date(time - 1000).toISOString() }, time).label).toBe(
      "刚刚更新",
    );
  });
  it("网络重试可提前，厂商限流不能提前", () => {
    const retry = {
      ...account,
      lastAttemptAt: new Date(time - 31000).toISOString(),
      nextRetryAt: new Date(time + 120000).toISOString(),
    };
    expect(refreshState({ ...retry, fetchStatus: "network_error" }, time).disabled).toBe(false);
    expect(refreshState({ ...retry, fetchStatus: "rate_limited" }, time).disabled).toBe(true);
    expect(refreshState({ ...retry, fetchStatus: "rate_limited" }, time + 120000).disabled).toBe(false);
  });
  it("查询进行中或账号暂停时不重复刷新", () => {
    expect(refreshState({ ...account, refreshing: true }, time).disabled).toBe(true);
    expect(refreshState({ ...account, enabled: false }, time).disabled).toBe(true);
  });
});

const now = Date.parse("2026-10-01T06:00:00Z");
const sample: QuotaMetric = {
  metricId: "5h",
  label: "5 小时窗口",
  kind: "quota",
  unit: "percent",
  remaining: null,
  limit: null,
  remainingPercent: 18,
  resetsAt: "2026-10-01T08:12:00Z",
  source: "official_api",
  observedAt: "2026-10-01T05:42:00Z",
};
describe("可信额度显示", () => {
  it("保留未知与零额度的区别", () => {
    expect(metricState({ ...sample, remainingPercent: null }, now)).toBe("unknown");
    expect(metricState({ ...sample, remainingPercent: 0 }, now)).toBe("current");
  });
  it("到达重置点后只标过期，不恢复额度", () => {
    expect(metricState(sample, Date.parse(sample.resetsAt!))).toBe("stale");
    expect(resetLabel(sample.resetsAt, Date.parse(sample.resetsAt!))).toBe("待更新");
  });
  it("旧数据和无效观测时间标过期", () => {
    expect(metricState({ ...sample, resetsAt: null }, now + 86400000)).toBe("stale");
    expect(metricState({ ...sample, observedAt: "invalid" }, now)).toBe("stale");
  });
  it("余额不需要百分比，重置倒计时以当前时间计算", () => {
    expect(
      metricState(
        { ...sample, kind: "balance", remaining: "¥128.50", remainingPercent: null, resetsAt: null },
        now,
      ),
    ).toBe("current");
    expect(resetLabel(sample.resetsAt, now)).toBe("2h12m");
  });
  it("真实数据按刷新间隔判过期，错误状态有明确说明", () => {
    expect(metricState({ ...sample, resetsAt: null, staleAfterSeconds: 900 }, now + 20 * 60000)).toBe(
      "stale",
    );
    expect(metricState({ ...sample, resetsAt: null, staleAfterSeconds: 3600 }, now + 20 * 60000)).toBe(
      "current",
    );
  });
});

describe("额度提醒文案", () => {
  const event: AlertEvent = {
    id: 1,
    accountId: "a",
    alias: "主账号",
    productName: "DeepSeek API",
    metricId: "quota_5h",
    metricLabel: "5 小时窗口",
    kind: "quota",
    unit: "%",
    threshold: 5,
    crossedThresholds: [30, 20, 10, 5],
    remainingPercent: 4.2,
    periodKey: "rolling",
    triggeredAt: "2026-10-01T00:00:00Z",
    readAt: null,
  };
  it("一次下跌只把最严重的一档放进标题，其余作为补充", () => {
    expect(alertHeadline(event)).toBe("5小时额度 剩余 4.2%，已跌破 5%");
    expect(alertDetail(event)).toContain("同次跨过 30%、20%、10%");
    expect(alertDetail(event)).toContain("滚动周期额度");
  });
  it("固定周期与未读时间如实显示", () => {
    expect(alertDetail({ ...event, periodKey: "fixed:2026-10-01T05" })).toContain("固定周期额度");
    const now = Date.parse("2026-10-01T00:30:00Z");
    expect(alertTime(event.triggeredAt, now)).toBe("30 分钟前");
    expect(alertTime("2026-10-01T00:29:30Z", now)).toBe("刚刚");
    expect(alertTime("不是时间", now)).toBe("时间未知");
  });
});

it("额度展示保留来源名称并将主窗口排在附加池前，且不修改原数组", () => {
  const metrics = [
    { metricId: "codex_http_base_model_inference_primary_window" },
    { metricId: "codex_http_codex_secondary_window" },
    { metricId: "codex_http_codex_primary_window" },
  ] as QuotaMetric[];
  expect(orderedMetrics(metrics).map((m) => m.metricId)).toEqual([
    "codex_http_codex_primary_window",
    "codex_http_codex_secondary_window",
    "codex_http_base_model_inference_primary_window",
  ]);
  expect(metrics[0].metricId).toContain("base_model_inference");
  expect(metricLabel("base_model_inference · 周额度")).toBe("备用额度 · 周额度");
  expect(metricLabel("自定义额度池")).toBe("自定义额度池");
});

it("浮窗仅在新鲜主额度耗尽后显示备用，失败或过期时不切换", () => {
  const now = Date.parse("2026-10-10T00:00:00Z");
  const metric = (id: string, percent: number): QuotaMetric => ({
    metricId: id,
    label: id,
    kind: "quota",
    unit: "percent",
    remaining: null,
    limit: null,
    remainingPercent: percent,
    resetsAt: null,
    source: "client_endpoint",
    observedAt: new Date(now).toISOString(),
    staleAfterSeconds: 1200,
  });
  const main = metric("codex_http_codex_primary_window", 30);
  const reserve = metric("codex_http_base_model_inference_primary_window", 100);
  const account: Account = {
    id: "a",
    providerId: "codex_http",
    alias: "a",
    productName: "Codex",
    enabled: true,
    fetchStatus: "ok",
    lastSuccessAt: null,
    metrics: [reserve, main],
  };
  expect(widgetMetrics(account, now)).toEqual([main]);
  main.remainingPercent = 0;
  expect(widgetMetrics(account, now)).toEqual([reserve]);
  expect(widgetMetrics({ ...account, fetchStatus: "network_error" }, now)).toEqual([main]);
  expect(widgetMetrics(account, now + 1201000)).toEqual([main]);
  expect(widgetMetrics({ ...account, metrics: [reserve] }, now)).toEqual([]);
});

it("只轮换月度额度，固定短时窗口并保留原始指标", () => {
  const metrics = ["five_hour", "monthly_total", "monthly_code"].map((metricId) => ({
    metricId,
  })) as QuotaMetric[];
  const account = { metrics } as Account;
  expect(rotatingWidgetMetrics(account, 0).map((m) => m.metricId)).toEqual(["five_hour", "monthly_total"]);
  expect(rotatingWidgetMetrics(account, 1).map((m) => m.metricId)).toEqual(["five_hour", "monthly_code"]);
  expect(rotatingWidgetMetrics(account, 2).map((m) => m.metricId)).toEqual(["five_hour", "monthly_total"]);
  expect(account.metrics).toHaveLength(3);
  expect(rotatingWidgetMetrics({ metrics: metrics.slice(0, 2) } as Account, 1)).toEqual(metrics.slice(0, 2));
});

it("百分比消除浮点尾数，保留一位有效显示小数", () => {
  expect(percentText(99.729999999)).toBe("99.7%");
  expect(percentText(99.30000000000001)).toBe("99.3%");
  expect(percentText(0)).toBe("0%");
  expect(percentText(100)).toBe("100%");
  expect(widgetMetricLabel("Kimi 5小时额度")).toBe("5h");
  expect(widgetMetricLabel("Kimi Code 月额度")).toBe("Code 月");
  expect(widgetMetricLabel("Kimi 会员共享月额度")).toBe("共享月");
});

it("各渠道额度名称统一为主窗口中文与浮窗短名称", () => {
  for (const label of ["5小时额度", "5 小时额度", "5小时窗口", "5 小时窗口", "5h"]) {
    expect(metricLabel(label)).toBe("5小时额度");
    expect(shortLabel(label)).toBe("5h");
  }
  expect(metricLabel("Sonnet 每周额度")).toBe("Sonnet 周额度");
  expect(shortLabel("备用额度 · 周额度")).toBe("备用额度 · 周");
  expect(shortLabel("月额度")).toBe("月");
  expect(metricLabel("钱包余额")).toBe("钱包余额");
});
