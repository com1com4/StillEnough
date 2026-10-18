import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { applyLanguage, t, translate } from "./i18n";
import { messages, type MessageKey } from "./locales";
import {
  balanceText,
  fetchStatusLabel,
  metricLabel,
  metricState,
  resetLabel,
  shortLabel,
} from "./presentation";
import { localizeProvider, providerName, providerOptionLabel } from "./providerPresentation";
import { AboutPage } from "../components/AboutPage";
import { NetworkFields } from "../components/NetworkFields";
import App from "../App";
import { HistoryPage } from "../components/HistoryPage";
import type { HistorySeries, ProviderDescriptor, QuotaMetric } from "./types";

afterEach(() => {
  applyLanguage("zh-CN");
  vi.unstubAllGlobals();
});

it("renders recorded balance history labels and unknown currencies in the selected language", () => {
  const series: HistorySeries[] = ["可用金额（币种未提供）", "现金余额（币种未提供）"].map(
    (metricLabel, index) => ({
      accountId: "history-fixture",
      alias: "API",
      productName: "MiniMax API",
      metricId: `balance-${index}`,
      metricLabel,
      kind: "balance",
      unit: "currency_unknown",
      observedCount: 1,
      segments: [
        { resetsAt: null, points: [{ observedAt: "2026-10-17T00:00:00Z", value: 12.5, text: "12.50" }] },
      ],
    }),
  );
  const render = () =>
    renderToStaticMarkup(
      <HistoryPage
        series={series}
        loading={false}
        days={7}
        accounts={[]}
        accountId="history-fixture"
        onDaysChange={() => {}}
        onAccountChange={() => {}}
      />,
    );
  applyLanguage("en");
  const english = render();
  expect(english).toContain("Available amount (currency not provided)");
  expect(english).toContain("Cash balance (currency not provided)");
  expect(english).toContain("Balance (Unknown currency)");
  expect(english).toContain("1 observation");
  expect(english).not.toContain("1 observations");
  expect(english).not.toMatch(/currency_unknown|可用金额|现金余额|币种未提供/);
  applyLanguage("zh-CN");
  const chinese = render();
  expect(chinese).toContain("可用金额（币种未提供）");
  expect(chinese).toContain("现金余额（币种未提供）");
  expect(chinese).toContain("余额（未知币种）");
  expect(chinese).not.toContain("currency_unknown");
  expect(series[0].unit).toBe("currency_unknown");
  expect(series[0].metricLabel).toBe("可用金额（币种未提供）");
});

describe("language resources", () => {
  it("provides both languages with matching named parameters", () => {
    const params = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const [key, [zh, en]] of Object.entries(messages)) {
      expect(zh.trim(), key).not.toBe("");
      expect(en.trim(), key).not.toBe("");
      expect(params(en), key).toEqual(params(zh));
      expect(en, key).not.toMatch(/[\u4e00-\u9fff]/);
    }
  });

  it("keeps user text literal and has a deterministic missing-key fallback", () => {
    expect(translate("en", "account.confirmDelete", { alias: "工作账号 {days}" })).toContain(
      "工作账号 {days}",
    );
    expect(translate("en", "missing.key" as MessageKey)).toBe("missing.key");
  });

  it("renders pages and accessible field labels in each language", () => {
    for (const [language, heading, mode] of [
      ["en", "Data &amp; privacy", "Network mode"],
      ["zh-CN", "数据与隐私", "网络方式"],
    ] as const) {
      applyLanguage(language);
      expect(renderToStaticMarkup(<AboutPage />)).toContain(heading);
      const fields = renderToStaticMarkup(
        <NetworkFields value={{ mode: "custom", proxyUrl: "" }} onChange={() => {}} />,
      );
      expect(fields).toContain(mode);
      expect(fields).toContain("http://127.0.0.1:7890");
    }
  });

  it("uses stable page IDs while rendering translated navigation and empty states", () => {
    vi.stubGlobal("window", { location: { search: "" } });
    for (const [language, overview, settings] of [
      ["en", "Overview", "Settings"],
      ["zh-CN", "总览", "设置"],
    ] as const) {
      applyLanguage(language);
      const html = renderToStaticMarkup(<App />);
      expect(html).toContain(`>${overview}<`);
      expect(html).toContain(`>${settings}<`);
      expect(html).not.toContain(">overview<");
      expect(html).not.toContain(">settings<");
    }
  });
});

describe("quota semantics survive language changes", () => {
  const metric: QuotaMetric = {
    metricId: "fixture",
    label: "现金余额（币种未提供）",
    kind: "balance",
    unit: "currency_unknown",
    remaining: "0.000",
    limit: null,
    remainingPercent: null,
    resetsAt: null,
    source: "official_api",
    observedAt: "2026-10-17T00:00:00Z",
  };
  it("preserves amount precision, unknown currency and missing values", () => {
    applyLanguage("en");
    expect(balanceText(metric)).toBe("0.000 · Unknown currency");
    expect(balanceText(metric, "widget")).toBe("0.000");
    expect(balanceText({ ...metric, remaining: null })).toBeNull();
    expect(metricState({ ...metric, remaining: null })).toBe("unknown");
    expect(resetLabel(null)).toBe("No reset reported");
    expect(resetLabel("2026-10-17T00:00:00Z", Date.parse("2026-10-17T00:00:00Z"))).toBe("Awaiting update");
    expect(metric.remaining).toBe("0.000");
  });

  it("translates known windows and status codes without changing unknown names", () => {
    applyLanguage("en");
    expect(metricLabel("base_model_inference · 5 小时窗口")).toBe("Reserve quota · 5-hour quota");
    expect(metricLabel("Sonnet 周额度")).toBe("Sonnet Weekly quota");
    expect(metricLabel("60 分钟窗口")).toBe("60-minute window");
    expect(metricLabel("可用余额 · USD")).toBe("Available balance · USD");
    expect(metricLabel("自定义额度池")).toBe("自定义额度池");
    expect(metricLabel("自定义周额度池")).toBe("自定义周额度池");
    expect(shortLabel("周额度")).toBe("Week");
    expect(fetchStatusLabel("rate_limited")).toBe("Rate limited · waiting to retry");
    expect(fetchStatusLabel("账号不存在")).toBe("Account does not exist");
    expect(fetchStatusLabel("future_status")).toBe("future_status");
    applyLanguage("zh-CN");
    expect(t("nav.settings")).toBe("设置");
  });
});

it("localizes all registered provider copy without altering connection configuration", () => {
  applyLanguage("en");
  const ids = [
    "codex",
    "codex_http",
    "claude_code",
    "deepseek_api",
    "minimax_api_intl",
    "minimax_token_plan_intl",
    "glm_coding_cn",
    "glm_coding_intl",
    "kimi_coding_cn",
    "kimi_coding_intl",
    "kimi_api_cn",
    "kimi_api_intl",

    "minimax_api",
    "minimax_token_plan",
    "bailian_api",
    "bailian_token_plan",
  ];
  for (const id of ids) {
    const source: ProviderDescriptor = {
      id,
      name: id,
      supported: true,
      authMethod: "api_key",
      baseUrls: ["https://example.invalid"],
      capability: "",
      status: "",
      limitation: "",
      setupInstructions: "",
      authorizationLabel: "",
      networkNote: "",
    };
    const localized = localizeProvider(source);
    expect(localized.name, id).not.toBe(id);
    expect(localized.capability, id).not.toBe("");
    expect(localized.limitation, id).not.toBe("");
    expect(localized.baseUrls).toEqual(source.baseUrls);
    expect(localized.authMethod).toBe(source.authMethod);
    expect(source.capability).toBe("");
  }
});

it("translates channel names and preserves experimental badges in both languages", () => {
  const source: ProviderDescriptor = {
    id: "bailian_api",
    name: "阿里百炼 API（阿里云账户余额）",
    supported: true,
    authMethod: "access_key_pair",
    baseUrls: ["https://business.aliyuncs.com"],
    capability: "",
    status: "",
    limitation: "",
    setupInstructions: "",
    authorizationLabel: "",
    networkNote: "",
  };
  applyLanguage("en");
  const english = "Alibaba Bailian API (Alibaba Cloud account balance)";
  expect(localizeProvider(source).name).toBe(english);
  expect(providerOptionLabel(source)).toBe(`${english} · Experimental`);
  expect(providerOptionLabel(localizeProvider(source))).toBe(`${english} · Experimental`);
  expect(providerName(source.id, source.name)).toBe(english);
  expect(providerName(undefined, source.name)).toBe(english);
  expect(providerName("unknown", "自定义渠道")).toBe("自定义渠道");
  expect(source.name).toBe("阿里百炼 API（阿里云账户余额）");
  applyLanguage("zh-CN");
  expect(providerName(undefined, english)).toBe(source.name);
  expect(providerOptionLabel(source)).toBe(`${source.name} · 实验性`);
});
