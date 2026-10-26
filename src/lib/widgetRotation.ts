/** Read current inputs on each tick without restarting the six-second clock. */
export function startWidgetRotation(
  accountIds: () => string[],
  paused: (accountId: string) => boolean,
  advance: (accountIds: string[]) => void,
) {
  const timer = setInterval(() => {
    const active = accountIds().filter((id) => !paused(id));
    if (active.length) advance(active);
  }, 6000);
  return () => clearInterval(timer);
}
