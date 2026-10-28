/** Pick the whole-window opacity for the glass widget without changing saved preferences. */
export function widgetGlassOpacity(normal: number, idle: number, fadeWhenIdle: boolean, interacting: boolean): number {
  return fadeWhenIdle && !interacting ? idle : normal;
}
