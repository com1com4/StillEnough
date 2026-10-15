import { expect, it } from "vitest";
import { reorderWidgetAccounts } from "./widgetOrder";
it("dragging moves before or after the target in either direction without changing membership", () => {
  const ids = ["a", "b", "c"];
  expect(reorderWidgetAccounts(ids, "a", "c", true)).toEqual(["b", "c", "a"]);
  expect(reorderWidgetAccounts(ids, "a", "c", false)).toEqual(["b", "a", "c"]);
  expect(reorderWidgetAccounts(ids, "c", "a", false)).toEqual(["c", "a", "b"]);
  expect(reorderWidgetAccounts(ids, "c", "a", true)).toEqual(["a", "c", "b"]);
  expect(ids).toEqual(["a", "b", "c"]);
});
it("dropping on the same account or a removed account preserves the order", () => {
  expect(reorderWidgetAccounts(["a", "b"], "a", "a", true)).toEqual(["a", "b"]);
  expect(reorderWidgetAccounts(["a", "b"], "removed", "b", true)).toEqual(["a", "b"]);
  expect(reorderWidgetAccounts(["a", "b"], "a", "removed", true)).toEqual(["a", "b"]);
});
