import type { ProviderDescriptor } from "./types";
import { t } from "./i18n";

// Presentation metadata lives beside provider localization, never in business pages.
const entries: Record<string, [string, string, string]> = {
  codex: ["OpenAI", "Codex · app-server", "default"],
  codex_http: ["OpenAI", "Codex · HTTP", "default"],
  claude_code: ["Anthropic", "Claude Code", "default"],
  deepseek_api: ["DeepSeek", "balance", "default"],
  minimax_api: ["MiniMax", "balance", "cn"],
  minimax_api_intl: ["MiniMax", "balance", "intl"],
  minimax_token_plan: ["MiniMax", "Token Plan", "cn"],
  minimax_token_plan_intl: ["MiniMax", "Token Plan", "intl"],
  bailian_api: ["bailian", "balance", "cn"],
  bailian_token_plan: ["bailian", "Token Plan", "cn"],
  glm_coding_cn: ["GLM", "Coding Plan", "cn"],
  glm_coding_intl: ["GLM", "Coding Plan", "intl"],
  kimi_coding_cn: ["Kimi", "Code", "cn"],
  kimi_coding_intl: ["Kimi", "Code", "intl"],
  kimi_api_cn: ["Kimi", "balance", "cn"],
  kimi_api_intl: ["Kimi", "balance", "intl"],
};

export function pickerEntry(provider: ProviderDescriptor) {
  const [platform, product, region] = entries[provider.id] ?? [provider.name, provider.name, "default"];
  return {
    provider,
    platform,
    platformLabel: platform === "bailian" ? t("picker.bailian") : platform,
    product,
    productLabel: product === "balance" ? t("picker.balance") : product,
    region,
    regionLabel:
      region === "cn" ? t("picker.cn") : region === "intl" ? t("picker.intl") : t("picker.default"),
  };
}

export function filterPickerProviders(providers: ProviderDescriptor[], query: string) {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return providers.filter((p) => {
    const entry = pickerEntry(p);
    const text = [p.id, p.name, entry.platformLabel, entry.productLabel, entry.regionLabel]
      .join(" ")
      .toLocaleLowerCase();
    return terms.every((term) => text.includes(term));
  });
}
