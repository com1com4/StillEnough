import { describe, expect, it } from "vitest";
import { buildChart, dayLabel, formatAmount, formatValue } from "./history";
import type { HistorySeries } from "./types";

function series(overrides: Partial<HistorySeries> = {}): HistorySeries {
  return {
    accountId: "a",
    alias: "别名",
    productName: "DeepSeek API",
    metricId: "quota_5h",
    metricLabel: "5 小时窗口",
    kind: "quota",
    unit: "%",
    segments: [
      {
        resetsAt: "2026-10-02T00:00:00Z",
        points: [
          { observedAt: "2026-10-01T00:00:00Z", value: 60, text: "60.0%" },
          { observedAt: "2026-10-01T01:00:00Z", value: 30, text: "30.0%" },
        ],
      },
    ],
    observedCount: 2,
    ...overrides,
  };
}

describe("历史曲线", () => {
  it("把每个分段画成独立折线，不跨重置连接", () => {
    const chart = buildChart(
      series({
        segments: [
          {
            resetsAt: "2026-10-01T05:00:00Z",
            points: [
              { observedAt: "2026-10-01T00:00:00Z", value: 60, text: null },
              { observedAt: "2026-10-01T01:00:00Z", value: 40, text: null },
            ],
          },
          {
            resetsAt: "2026-10-02T05:00:00Z",
            points: [
              { observedAt: "2026-10-02T00:00:00Z", value: 90, text: null },
              { observedAt: "2026-10-02T01:00:00Z", value: 80, text: null },
            ],
          },
        ],
        observedCount: 4,
      }),
      640,
      200,
    );
    expect(chart).not.toBeNull();
    expect(chart!.segments).toHaveLength(2);
    expect(chart!.segments[0].points).toHaveLength(2);
    // 后一段的起点必须落在右半区，不能接在前一段末尾。
    expect(chart!.segments[1].points[0].x).toBeGreaterThan(chart!.segments[0].points[1].x);
  });

  it("未知额度不画点，也不当作 0%", () => {
    const chart = buildChart(
      series({
        segments: [
          {
            resetsAt: null,
            points: [{ observedAt: "2026-10-01T00:00:00Z", value: null, text: "未知" }],
          },
        ],
        observedCount: 1,
      }),
      640,
      200,
    );
    expect(chart).toBeNull();
  });

  it("只有一个观测时画成一个点，不连成直线", () => {
    const chart = buildChart(
      series({
        segments: [
          {
            resetsAt: null,
            points: [{ observedAt: "2026-10-01T00:00:00Z", value: 42, text: null }],
          },
        ],
        observedCount: 1,
      }),
      640,
      200,
    );
    expect(chart!.segments[0].points).toHaveLength(1);
    expect(chart!.min).toBeLessThan(42);
    expect(chart!.max).toBeGreaterThan(42);
  });

  it("数值相同的序列也能画出可读区间", () => {
    const chart = buildChart(
      series({
        kind: "balance",
        unit: "USD",
        segments: [
          {
            resetsAt: null,
            points: [
              { observedAt: "2026-10-01T00:00:00Z", value: 10, text: "10.00 USD" },
              { observedAt: "2026-10-01T01:00:00Z", value: 10, text: "10.00 USD" },
            ],
          },
        ],
      }),
      640,
      200,
    );
    expect(chart!.min).toBeLessThan(10);
    expect(chart!.max).toBeGreaterThan(10);
  });

  it("数值格式区分百分比与余额", () => {
    expect(formatValue(40, "quota")).toBe("40%");
    expect(formatValue(0.4567, "quota")).toBe("0.46%");
    expect(formatValue(12.5, "balance")).toBe("12.5");
    expect(formatAmount(1234.56)).toBe("1235");
    expect(dayLabel(Date.parse("2026-10-01T00:00:00Z"))).toMatch(/10/);
  });
});
