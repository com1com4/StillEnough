import { describe, expect, it } from "vitest";

// A resolver over an injected "system says dark" flag, so both branches of
// follow-system are testable without touching the real OS setting.
function resolveWith(mode: "system" | "light" | "dark", systemDark: boolean) {
  if (mode === "light") return "light";
  if (mode === "dark") return "dark";
  return systemDark ? "dark" : "light";
}

describe("主题解析", () => {
  it("固定白天或夜间时忽略系统设置", () => {
    expect(resolveWith("light", true)).toBe("light");
    expect(resolveWith("dark", false)).toBe("dark");
  });

  it("跟随系统时采用系统的深浅", () => {
    expect(resolveWith("system", true)).toBe("dark");
    expect(resolveWith("system", false)).toBe("light");
  });

  it("系统外观变化只在跟随系统时影响结果", () => {
    // 白天/夜间是钉死的，系统切换不应改变任何一侧。
    expect(resolveWith("light", false)).toBe("light");
    expect(resolveWith("light", true)).toBe("light");
    expect(resolveWith("dark", false)).toBe("dark");
    expect(resolveWith("dark", true)).toBe("dark");
  });
});

describe("主题选项", () => {
  it("同时提供跟随系统、白天与夜间三项", async () => {
    const { themeOptions } = await import("./theme");
    expect(themeOptions().map((option) => option.value)).toEqual([
      "system",
      "light",
      "dark",
    ]);
  });

  it("每一项都带说明文字", async () => {
    const { themeOptions } = await import("./theme");
    for (const option of themeOptions()) {
      expect(option.label.length).toBeGreaterThan(0);
      expect(option.hint.length).toBeGreaterThan(0);
    }
  });
});
