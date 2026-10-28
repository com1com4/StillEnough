import { describe, expect, it } from "vitest";
import { widgetGlassOpacity } from "./widgetGlassOpacity";

describe("glass widget whole-window opacity", () => {
  it("uses the idle value only when fading is enabled and the widget is idle", () => {
    expect(widgetGlassOpacity(85, 12, true, false)).toBe(12);
    expect(widgetGlassOpacity(85, 12, true, true)).toBe(85);
    expect(widgetGlassOpacity(85, 12, false, false)).toBe(85);
  });
  it("allows the full opacity and the minimum idle value", () => {
    expect(widgetGlassOpacity(100, 1, true, false)).toBe(1);
    expect(widgetGlassOpacity(100, 1, true, true)).toBe(100);
  });
});
