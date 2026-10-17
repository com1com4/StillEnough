import type { HistoryPoint, HistorySeries } from "./types";
import { currentLanguage } from "./i18n";

export interface ChartSegment {
  resetsAt: string | null;
  points: { x: number; y: number }[];
  values: number[];
}
export interface Chart {
  width: number;
  height: number;
  min: number;
  max: number;
  from: number;
  to: number;
  segments: ChartSegment[];
  firstAt: string | null;
  lastAt: string | null;
}

const time = (value: string) => Date.parse(value);

/**
 * Turn one series into drawable polylines. Each backend segment becomes its own polyline
 * so a reset or an outage is never bridged, and a single observation draws as a dot
 * rather than a flat line.
 */
export function buildChart(series: HistorySeries, width: number, height: number, padding = 28): Chart | null {
  const points: { at: number; value: number }[] = [];
  for (const segment of series.segments) {
    for (const point of segment.points) {
      const at = time(point.observedAt);
      const value = point.value;
      if (!Number.isFinite(at) || value === null || !Number.isFinite(value)) continue;
      points.push({ at, value });
    }
  }
  if (!points.length) return null;
  const from = Math.min(...points.map((point) => point.at));
  const to = Math.max(...points.map((point) => point.at));
  const values = points.map((point) => point.value);
  let min = Math.min(...values);
  let max = Math.max(...values);
  if (min === max) {
    // A flat series still needs a readable band instead of a division by zero.
    const pad = Math.max(Math.abs(min) * 0.05, 1);
    min -= pad;
    max += pad;
  }
  const innerWidth = Math.max(width - padding * 2, 1);
  const innerHeight = Math.max(height - padding * 2, 1);
  const x = (at: number) =>
    padding + (to === from ? innerWidth / 2 : ((at - from) / (to - from)) * innerWidth);
  const y = (value: number) => padding + innerHeight - ((value - min) / (max - min)) * innerHeight;
  const segments: ChartSegment[] = series.segments.map((segment) => {
    const drawn = segment.points
      .map((point: HistoryPoint) => ({ at: time(point.observedAt), value: point.value }))
      .filter(
        (point): point is { at: number; value: number } =>
          Number.isFinite(point.at) && point.value !== null && Number.isFinite(point.value),
      );
    return {
      resetsAt: segment.resetsAt,
      points: drawn.map((point) => ({ x: x(point.at), y: y(point.value) })),
      values: drawn.map((point) => point.value),
    };
  });
  return {
    width,
    height,
    min,
    max,
    from,
    to,
    segments: segments.filter((segment) => segment.points.length > 0),
    firstAt: new Date(from).toISOString(),
    lastAt: new Date(to).toISOString(),
  };
}

export const formatPercent = (value: number) =>
  `${value >= 0 && value < 1 ? value.toFixed(2) : Math.round(value * 10) / 10}%`;

export const formatAmount = (value: number) =>
  String(Math.abs(value) >= 100 ? Math.round(value) : Math.round(value * 100) / 100);

export const formatValue = (value: number, kind: HistorySeries["kind"]) =>
  kind === "quota" ? formatPercent(value) : formatAmount(value);

export const dayLabel = (value: number) =>
  new Intl.DateTimeFormat(currentLanguage(), { month: "2-digit", day: "2-digit" }).format(value);
