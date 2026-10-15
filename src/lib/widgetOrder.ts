/** Move one selected account before/after another without changing membership. */
export function reorderWidgetAccounts(
  ids: string[],
  source: string,
  target: string,
  after: boolean,
): string[] {
  if (source === target || !ids.includes(source) || !ids.includes(target)) return [...ids];
  const next = ids.filter((id) => id !== source);
  next.splice(next.indexOf(target) + (after ? 1 : 0), 0, source);
  return next;
}
