import { afterEach, expect, it, vi } from "vitest";
import { startWidgetRotation } from "./widgetRotation";

afterEach(() => vi.useRealTimers());

it("账号每秒更新仍在六秒切换，读取最新账号并可清理", () => {
  vi.useFakeTimers();
  let ids = ["initial"];
  const advance = vi.fn();
  const stop = startWidgetRotation(
    () => ids,
    () => false,
    advance,
  );
  for (let second = 1; second <= 6; second++) {
    ids = [`account-${second}`];
    vi.advanceTimersByTime(1000);
  }
  expect(advance).toHaveBeenCalledExactlyOnceWith(["account-6"]);
  vi.advanceTimersByTime(6000);
  expect(advance).toHaveBeenCalledTimes(2);
  stop();
  vi.advanceTimersByTime(6000);
  expect(advance).toHaveBeenCalledTimes(2);
});

it("暂停只阻止对应账号，解除暂停后继续切换", () => {
  vi.useFakeTimers();
  let paused = true;
  const advance = vi.fn();
  const stop = startWidgetRotation(
    () => ["kimi", "other"],
    (id) => paused && id === "kimi",
    advance,
  );
  vi.advanceTimersByTime(6000);
  expect(advance).toHaveBeenLastCalledWith(["other"]);
  paused = false;
  vi.advanceTimersByTime(6000);
  expect(advance).toHaveBeenLastCalledWith(["kimi", "other"]);
  stop();
});
