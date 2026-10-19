import { describe, expect, it } from "vitest";
import { pickerEntry, filterPickerProviders } from "./providerPicker";
import type { ProviderDescriptor } from "./types";
const provider = (id: string, name: string): ProviderDescriptor => ({
  id,
  name,
  supported: true,
  authMethod: "api_key",
  baseUrls: [],
  capability: "",
  status: "",
  limitation: "",
  setupInstructions: "",
  authorizationLabel: "",
  networkNote: "",
});
describe("product picker registry", () => {
  it("groups regional variants without conflating plan and wallet", () => {
    const domestic = pickerEntry(provider("minimax_token_plan", "MiniMax Token Plan"));
    const international = pickerEntry(provider("minimax_token_plan_intl", "MiniMax Token Plan"));
    const wallet = pickerEntry(provider("minimax_api", "MiniMax API"));
    expect(domestic.platform).toBe(international.platform);
    expect(domestic.product).toBe(international.product);
    expect(domestic.region).not.toBe(international.region);
    expect(domestic.product).not.toBe(wallet.product);
  });
  it("matches platform, product and region together and handles empty results", () => {
    const providers = [
      provider("kimi_api_cn", "Kimi 国内 API"),
      provider("kimi_coding_intl", "Kimi Code 海外"),
    ];
    expect(filterPickerProviders(providers, " KIMI api 国内 ").map((p) => p.id)).toEqual(["kimi_api_cn"]);
    expect(filterPickerProviders(providers, "DeepSeek")).toEqual([]);
    expect(filterPickerProviders(providers, "")).toHaveLength(2);
  });
  it("keeps newly registered unknown products available", () => {
    const future = provider("future_provider", "Future Provider");
    expect(pickerEntry(future).productLabel).toBe("Future Provider");
    expect(filterPickerProviders([future], "future")).toEqual([future]);
  });
});
