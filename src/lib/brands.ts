export const brands = [
  { match: /claude/i, name: "Claude", icon: "claude", color: "#c96b49" },
  { match: /codex|openai/i, name: "Codex", icon: "openai", color: "var(--ink)" },
  { match: /minimax/i, name: "MiniMax", icon: "minimax", color: "#ee6b64" },
  { match: /kimi/i, name: "Kimi", icon: "kimi", color: "#3478b8" },
  { match: /deepseek/i, name: "DeepSeek", icon: "deepseek", color: "#3478b8" },
];
export function productBrand(product: string) {
  return brands.find((brand) => brand.match.test(product));
}
