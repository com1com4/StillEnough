import { providerName } from "../lib/providerPresentation";
import { t, currentLanguage } from "../lib/i18n";
import { currencyLabel, metricLabel } from "../lib/presentation";
import { useMemo, useState } from "react";
import { ClockIcon } from "@phosphor-icons/react";
import type { Account, HistorySeries } from "../lib/types";
import { buildChart, dayLabel, formatValue } from "../lib/history";

const WIDTH = 640;
const HEIGHT = 200;

export function HistoryPage({
  series,
  loading,
  days,
  onDaysChange,
  accounts,
  accountId,
  onAccountChange,
}: {
  series: HistorySeries[];
  loading: boolean;
  days: 7 | 30;
  onDaysChange: (days: 7 | 30) => void;
  accounts: Account[];
  accountId: string;
  onAccountChange: (id: string) => void;
}) {
  const [selected, setSelected] = useState<string>("");
  // 各平台额度窗口口径不同，跨账号比较没有意义，因此一次只看一个账号。
  const visible = useMemo(() => series.filter((entry) => entry.accountId === accountId), [series, accountId]);
  const active = useMemo(
    () => visible.find((entry) => entry.metricId === selected) ?? visible[0] ?? null,
    [visible, selected],
  );
  const chart = useMemo(() => (active ? buildChart(active, WIDTH, HEIGHT) : null), [active]);

  const unit = active?.kind === "quota" ? "" : currencyLabel(active?.unit ?? "");
  const lineCount = chart?.segments.filter((segment) => segment.points.length > 1).length ?? 0;
  const isolatedCount = chart?.segments.filter((segment) => segment.points.length === 1).length ?? 0;

  return (
    <div className="secondary-page">
      <div className="history-controls">
        <div className="segmented" role="group" aria-label={t("history.range")}>
          {[7, 30].map((option) => (
            <button
              key={option}
              className={days === option ? "selected" : ""}
              onClick={() => onDaysChange(option as 7 | 30)}
            >
              {t("history.days", { days: option })}
            </button>
          ))}
        </div>
        {accounts.length > 0 && (
          <select
            aria-label={t("account.label")}
            value={accountId}
            onChange={(event) => {
              onAccountChange(event.target.value);
              setSelected("");
            }}
          >
            {accounts.map((item) => (
              <option key={item.id} value={item.id}>
                {`${providerName(item.providerId, item.productName)} · ${item.alias}`}
              </option>
            ))}
          </select>
        )}
      </div>

      {visible.length > 1 && (
        <div className="history-metrics">
          {visible.map((entry) => (
            <button
              key={entry.metricId}
              className={active?.metricId === entry.metricId ? "selected" : ""}
              onClick={() => setSelected(entry.metricId)}
            >
              {metricLabel(entry.metricLabel)}
            </button>
          ))}
        </div>
      )}

      {loading ? (
        <div className="loading-state">
          <span className="spinner" /> {t("history.loading")}
        </div>
      ) : !active ? (
        <div className="history-empty">
          <ClockIcon className="empty-icon" />
          <h3>{accounts.length ? t("history.emptyDays", { days }) : t("history.noAccounts")}</h3>
          <p>{accounts.length ? t("history.emptyHelp") : t("history.connectHelp")}</p>
        </div>
      ) : (
        <section className="history-chart-card">
          <header>
            <div>
              <h2>{metricLabel(active.metricLabel)}</h2>
              <p>
                {providerName(undefined, active.productName)} · {active.alias} ·{" "}
                {active.kind === "quota" ? t("history.remainingPercent") : t("history.balanceUnit", { unit })}{" "}
                ·{" "}
                {t(active.observedCount === 1 ? "history.oneObservation" : "history.observations", {
                  count: active.observedCount,
                })}
              </p>
            </div>
            <div className="chart-range">
              {(() => {
                const last = chart?.segments[chart.segments.length - 1]?.values.slice(-1)[0];
                if (last === undefined || !chart) return <small>{t("history.noValues")}</small>;
                return (
                  <>
                    <strong>
                      {formatValue(last, active.kind)}
                      {unit && ` ${unit}`}
                    </strong>
                    <small>
                      {dayLabel(chart.from)} – {dayLabel(chart.to)}
                    </small>
                  </>
                );
              })()}
            </div>
          </header>
          {chart ? (
            <>
              <svg
                className="history-chart"
                viewBox={`0 0 ${chart.width} ${chart.height}`}
                role="img"
                aria-label={t("history.chartLabel", { label: metricLabel(active.metricLabel), days })}
              >
                {[0, 0.5, 1].map((ratio) => {
                  const y = 28 + (chart.height - 56) * ratio;
                  const value = chart.max - (chart.max - chart.min) * ratio;
                  return (
                    <g key={ratio}>
                      <line x1={28} x2={chart.width - 28} y1={y} y2={y} className="chart-grid" />
                      <text x={4} y={y + 4} className="chart-axis">
                        {formatValue(value, active.kind)}
                      </text>
                    </g>
                  );
                })}
                {chart.segments.map((segment, index) => (
                  <g key={index}>
                    {segment.points.length > 1 ? (
                      <polyline
                        className="chart-line"
                        vectorEffect="non-scaling-stroke"
                        points={segment.points.map((point) => `${point.x},${point.y}`).join(" ")}
                      />
                    ) : (
                      segment.points.map((point, pointIndex) => (
                        <circle
                          key={pointIndex}
                          className="chart-dot"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth={2.5}
                          vectorEffect="non-scaling-stroke"
                          cx={point.x}
                          cy={point.y}
                          r={0.01}
                        />
                      ))
                    )}
                  </g>
                ))}
                <text x={28} y={chart.height - 6} className="chart-axis">
                  {dayLabel(chart.from)}
                </text>
                <text x={chart.width - 28} y={chart.height - 6} className="chart-axis end">
                  {dayLabel(chart.to)}
                </text>
              </svg>
              <footer className="chart-foot">
                {chart.segments.length > 1
                  ? t("history.segments", { lines: lineCount, points: isolatedCount })
                  : t("history.updated", {
                      time: new Intl.DateTimeFormat(currentLanguage(), {
                        month: "2-digit",
                        day: "2-digit",
                        hour: "2-digit",
                        minute: "2-digit",
                      }).format(chart.to),
                    })}
              </footer>
            </>
          ) : (
            <p className="chart-foot">{t("history.unknownHelp")}</p>
          )}
        </section>
      )}
    </div>
  );
}
