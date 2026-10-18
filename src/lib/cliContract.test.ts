import { describe, expect, it } from "vitest";
import cases from "../../fixtures/cli/freshness.json";
import { metricState } from "./presentation";
import type { QuotaMetric } from "./types";
describe("桌面/CLI 共用新鲜度边界", () => {
  for (const item of cases)
    it(item.name, () => {
      const metric: QuotaMetric = {
        metricId: "test",
        label: "test",
        kind: item.kind as QuotaMetric["kind"],
        unit: "test",
        remaining: item.remaining,
        limit: null,
        remainingPercent: item.percent,
        resetsAt: item.reset,
        source: "official_api",
        observedAt: item.observed,
        staleAfterSeconds: item.ttl,
      };
      expect(metricState(metric, Date.parse(item.now))).toBe(
        item.expected === "expired" ? "stale" : item.expected,
      );
    });
});
