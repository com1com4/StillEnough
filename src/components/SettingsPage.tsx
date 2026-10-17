import { providerName } from "../lib/providerPresentation";
import { t, currentLanguage } from "../lib/i18n";
import { NetworkSettings } from "./NetworkSettings";
import { useRef, useState } from "react";
import { reorderWidgetAccounts } from "../lib/widgetOrder";
import { DownloadSimpleIcon, TrashIcon } from "@phosphor-icons/react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "../lib/ipc";
import type { Account, AppSettings, WidgetOptions } from "../lib/types";
import { themeOptions, resolveTheme } from "../lib/theme";
import type { ThemeMode } from "../lib/theme";
import type { LanguagePreference } from "../lib/i18n";

const THRESHOLD_OPTIONS = [50, 30, 20, 10, 5, 3];
const RETENTION_OPTIONS = [30, 60, 90];

export function SettingsPage({
  settings,
  accounts,
  onRefreshInterval,
  onToggleWidget,
  onPauseChange,
  onAutostartChange,
  onWidgetOptions,
  onThemeChange,
  onLanguageChange,
  onNotice,
  onError,
  onCleared,
}: {
  settings: AppSettings;
  accounts: Account[];
  onRefreshInterval: (minutes: number) => Promise<void>;
  onToggleWidget: () => Promise<void>;
  onPauseChange: (paused: boolean) => Promise<void>;
  onAutostartChange: (enabled: boolean) => Promise<void>;
  onWidgetOptions: (options: WidgetOptions) => Promise<void>;
  onThemeChange: (theme: ThemeMode) => Promise<void>;
  onLanguageChange: (language: LanguagePreference) => Promise<void>;
  onNotice: (message: string) => void;
  onError: (message: string) => void;
  onCleared: () => void;
}) {
  const [thresholds, setThresholds] = useState<number[]>(settings.alertThresholds);
  const [alertEnabled, setAlertEnabled] = useState(settings.alertEnabled);
  const [retention, setRetention] = useState(settings.historyRetentionDays);
  const [busy, setBusy] = useState(false);
  const [widgetOrderBusy, setWidgetOrderBusy] = useState(false);
  const [widgetDrag, setWidgetDrag] = useState<{ id: string; target: string; after: boolean } | null>(null);
  const widgetDragRef = useRef<typeof widgetDrag>(null);

  function updateWidgetDrag(next: typeof widgetDrag) {
    widgetDragRef.current = next;
    setWidgetDrag(next);
  }

  const selectedWidgetAccounts = (settings.widgetAccounts ?? []).filter((id) =>
    accounts.some((account) => account.id === id),
  );
  const widgetAccountChoices = [
    ...selectedWidgetAccounts.flatMap((id) => accounts.filter((account) => account.id === id)),
    ...accounts.filter((account) => !selectedWidgetAccounts.includes(account.id)),
  ];

  async function saveWidgetAccounts(ids: string[] | null) {
    setWidgetOrderBusy(true);
    try {
      await onWidgetOptions({
        collapsed: settings.widgetCollapsed,
        pinned: settings.widgetPinned,
        opacity: settings.widgetOpacity,
        accounts: ids,
      });
    } finally {
      setWidgetOrderBusy(false);
    }
  }

  function moveWidgetAccount(index: number, direction: -1 | 1) {
    const next = [...selectedWidgetAccounts];
    const target = index + direction;
    if (index < 0 || target < 0 || target >= next.length) return;
    [next[index], next[target]] = [next[target], next[index]];
    void saveWidgetAccounts(next);
  }

  const [clearText, setClearText] = useState("");
  const confirmClear = clearText === (currentLanguage() === "en" ? "DELETE ALL DATA" : "删除全部数据");
  const [languageBusy, setLanguageBusy] = useState(false);

  function toggleThreshold(value: number) {
    setThresholds((current) =>
      current.includes(value)
        ? current.filter((item) => item !== value)
        : [...current, value].sort((a, b) => b - a),
    );
  }

  async function saveMonitoring() {
    setBusy(true);
    try {
      const saved = await api.updateMonitoringSettings({
        alertEnabled,
        alertThresholds: thresholds,
        historyRetentionDays: retention,
      });
      setThresholds(saved.alertThresholds);
      setAlertEnabled(saved.alertEnabled);
      setRetention(saved.historyRetentionDays);
      onNotice(t("monitoring.saved"));
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }

  async function runExport(format: "json" | "csv") {
    setBusy(true);
    try {
      const summary = await api.exportData(format);
      onNotice(
        t("data.exported", {
          accounts: summary.accountCount,
          rows: summary.rowCount,
          file: summary.fileName,
        }),
      );
      try {
        await revealItemInDir(summary.path);
      } catch {
        // The file exists either way; failing to open Explorer must not fail the export.
      }
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : t("error.export"));
    } finally {
      setBusy(false);
    }
  }

  async function clearAll() {
    if (!confirmClear) return;
    if (!window.confirm(t("data.confirm"))) {
      setClearText("");
      return;
    }
    setBusy(true);
    try {
      const removed = await api.clearAllData("删除全部数据");
      setClearText("");
      onNotice(t("data.deleted", { count: removed }));
      onCleared();
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : t("error.delete"));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="secondary-page">
      <section className="settings-card">
        <div className="settings-row">
          <div>
            <strong>{t("language.title")}</strong>
            <span>{t("language.help")}</span>
          </div>
          <select
            aria-label={t("language.title")}
            value={settings.language}
            disabled={languageBusy}
            onChange={(event) => {
              setLanguageBusy(true);
              void onLanguageChange(event.target.value as LanguagePreference).finally(() =>
                setLanguageBusy(false),
              );
            }}
          >
            <option value="system">{t("common.system")} / System</option>
            <option value="zh-CN">简体中文</option>
            <option value="en">English</option>
          </select>
        </div>
      </section>
      <NetworkSettings accounts={accounts} />
      <div className="settings-card">
        <div className="settings-row column">
          <div>
            <strong>{t("theme.title")}</strong>
            <span>
              {t(settings.theme === "system" ? "theme.systemStatus" : "theme.fixedStatus", {
                mode: resolveTheme(settings.theme) === "dark" ? t("theme.dark") : t("theme.light"),
              })}
            </span>
          </div>
          <div className="theme-picker" role="group" aria-label={t("theme.title")}>
            {themeOptions().map((option) => (
              <button
                key={option.value}
                className={settings.theme === option.value ? "selected" : ""}
                aria-pressed={settings.theme === option.value}
                onClick={() => void onThemeChange(option.value)}
              >
                <strong>{option.label}</strong>
                <small>{option.hint}</small>
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="settings-card">
        <div className="settings-row">
          <div>
            <strong>{t("refresh.interval")}</strong>
            <span>{t("refresh.intervalHelp")}</span>
          </div>
          <select
            value={settings.refreshMinutes}
            onChange={(event) => void onRefreshInterval(Number(event.target.value))}
          >
            <option value={5}>{t("refresh.every5")}</option>
            <option value={10}>{t("refresh.every10")}</option>
            <option value={15}>{t("refresh.every15")}</option>
            <option value={30}>{t("refresh.every30")}</option>
          </select>
        </div>
        <div className="settings-row">
          <div>
            <strong>{t("collection.pause")}</strong>
            <span>{t("collection.help")}</span>
          </div>
          <label className="switch">
            <input
              type="checkbox"
              checked={settings.collectionPaused}
              onChange={(event) => void onPauseChange(event.target.checked)}
            />
            <span>{settings.collectionPaused ? t("collection.paused") : t("collection.active")}</span>
          </label>
        </div>
        <div className="settings-row">
          <div>
            <strong>{t("autostart.title")}</strong>
            <span>{t("autostart.help")}</span>
          </div>
          <label className="switch">
            <input
              type="checkbox"
              checked={settings.autostart}
              onChange={(event) => void onAutostartChange(event.target.checked)}
            />
            <span>{settings.autostart ? t("common.enabled") : t("common.disabled")}</span>
          </label>
        </div>
        <div className="settings-row">
          <div>
            <strong>{t("widget.title")}</strong>
            <span>{t("widget.help")}</span>
          </div>
          <button className="button button-quiet" onClick={() => void onToggleWidget()}>
            {settings.widgetVisible ? t("widget.hide") : t("widget.show")}
          </button>
        </div>
        <div className="settings-row">
          <div>
            <strong>{t("widget.opacity")}</strong>
            <span>{t("widget.opacityHelp")}</span>
          </div>
          <div className="settings-inline">
            <input
              type="range"
              aria-label={t("widget.opacity")}
              min={50}
              max={100}
              step={5}
              value={settings.widgetOpacity}
              onChange={(event) =>
                void onWidgetOptions({
                  collapsed: settings.widgetCollapsed,
                  pinned: settings.widgetPinned,
                  opacity: Number(event.target.value),
                  accounts: settings.widgetAccounts,
                })
              }
            />
            <span className="settings-hint">{settings.widgetOpacity}%</span>
          </div>
        </div>
        <div className="settings-row column">
          <div>
            <strong>{t("widget.content")}</strong>
            <span>{t("widget.contentHelp")}</span>
          </div>
          <div className="settings-inline">
            {(
              [
                ["showAlias", t("account.alias")],
                ["showProgress", t("widget.progress")],
                ["showReset", t("metric.resetTime")],
              ] as const
            ).map(([key, label]) => (
              <label className="switch" key={key}>
                <input
                  type="checkbox"
                  checked={settings.widgetDisplay[key]}
                  onChange={(event) =>
                    void onWidgetOptions({
                      collapsed: settings.widgetCollapsed,
                      pinned: settings.widgetPinned,
                      opacity: settings.widgetOpacity,
                      accounts: settings.widgetAccounts,
                      display: { ...settings.widgetDisplay, [key]: event.target.checked },
                    })
                  }
                />
                <span>{label}</span>
              </label>
            ))}
            <label>
              {t("widget.accountCount")}{" "}
              <select
                aria-label={t("widget.accountCountLabel")}
                value={settings.widgetDisplay.maxAccounts}
                onChange={(event) =>
                  void onWidgetOptions({
                    collapsed: settings.widgetCollapsed,
                    pinned: settings.widgetPinned,
                    opacity: settings.widgetOpacity,
                    accounts: settings.widgetAccounts,
                    display: { ...settings.widgetDisplay, maxAccounts: Number(event.target.value) },
                  })
                }
              >
                {[1, 2, 3, 4, 5, 6, 7, 8].map((count) => (
                  <option key={count} value={count}>
                    {count} {t("common.countUnit")}
                  </option>
                ))}
              </select>
            </label>
          </div>
        </div>
        <div className="settings-row column">
          <div>
            <strong>{t("widget.accounts")}</strong>
            <span>{t("widget.accountsHelp")}</span>
          </div>
          <div
            className="widget-account-picker widget-account-list"
            role="group"
            aria-label={t("widget.accounts")}
          >
            {widgetAccountChoices.map((account) => {
              const selected = selectedWidgetAccounts;
              const index = selected.indexOf(account.id);
              const checked = index >= 0;
              return (
                <div
                  className={`widget-account-option widget-account-order ${widgetDrag?.id === account.id ? "is-dragging" : ""} ${widgetDrag?.target === account.id && widgetDrag.id !== account.id ? (widgetDrag.after ? "drop-after" : "drop-before") : ""}`}
                  key={account.id}
                  data-widget-account-id={checked ? account.id : undefined}
                >
                  <button
                    type="button"
                    className="widget-drag-handle"
                    disabled={!checked || widgetOrderBusy || selected.length < 2}
                    aria-label={t("widget.dragAccount", { alias: account.alias })}
                    title={t("widget.dragHelp")}
                    onPointerDown={(event) => {
                      if (event.button !== 0 || !checked || widgetOrderBusy) return;
                      event.preventDefault();
                      event.currentTarget.focus();
                      event.currentTarget.setPointerCapture(event.pointerId);
                      updateWidgetDrag({ id: account.id, target: account.id, after: false });
                    }}
                    onPointerMove={(event) => {
                      const drag = widgetDragRef.current;
                      if (!drag || drag.id !== account.id) return;
                      const row = document
                        .elementFromPoint(event.clientX, event.clientY)
                        ?.closest<HTMLElement>("[data-widget-account-id]");
                      const target = row?.dataset.widgetAccountId;
                      if (!row || !target) {
                        updateWidgetDrag({ ...drag, target: drag.id, after: false });
                        return;
                      }
                      const bounds = row.getBoundingClientRect();
                      updateWidgetDrag({
                        ...drag,
                        target,
                        after: event.clientY > bounds.top + bounds.height / 2,
                      });
                    }}
                    onPointerUp={(event) => {
                      const drag = widgetDragRef.current;
                      updateWidgetDrag(null);
                      if (event.currentTarget.hasPointerCapture(event.pointerId))
                        event.currentTarget.releasePointerCapture(event.pointerId);
                      if (!drag || drag.id === drag.target) return;
                      const next = reorderWidgetAccounts(selected, drag.id, drag.target, drag.after);
                      if (next.some((id, position) => id !== selected[position]))
                        void saveWidgetAccounts(next);
                    }}
                    onPointerCancel={() => updateWidgetDrag(null)}
                    onLostPointerCapture={() => updateWidgetDrag(null)}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") updateWidgetDrag(null);
                    }}
                  >
                    ⠿
                  </button>
                  <label>
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={widgetOrderBusy || widgetDrag !== null}
                      onChange={(event) => {
                        const current = new Set(selected);
                        if (event.target.checked) current.add(account.id);
                        else current.delete(account.id);
                        // Every option cleared returns to the automatic behaviour.
                        const next = current.size ? [...current] : null;
                        void saveWidgetAccounts(next);
                      }}
                    />
                    <span>
                      {checked ? `${index + 1}. ` : ""}
                      {account.alias}
                      <small>{providerName(account.providerId, account.productName)}</small>
                    </span>
                  </label>
                  {checked && (
                    <div className="widget-order-actions">
                      <button
                        type="button"
                        className="button button-quiet"
                        aria-label={t("widget.moveUpAccount", { alias: account.alias })}
                        disabled={widgetOrderBusy || index === 0}
                        onClick={() => moveWidgetAccount(index, -1)}
                      >
                        {t("common.moveUp")}
                      </button>
                      <button
                        type="button"
                        className="button button-quiet"
                        aria-label={t("widget.moveDownAccount", { alias: account.alias })}
                        disabled={widgetOrderBusy || index === selected.length - 1}
                        onClick={() => moveWidgetAccount(index, 1)}
                      >
                        {t("common.moveDown")}
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
            {!accounts.length && <span className="settings-hint">{t("account.empty")}</span>}
          </div>
          {settings.widgetAccounts !== null && (
            <button
              className="button button-quiet"
              disabled={widgetOrderBusy}
              onClick={() => void saveWidgetAccounts(null)}
            >
              {t("widget.autoOrder")}
            </button>
          )}
        </div>
      </div>

      <div className="settings-card">
        <div className="settings-row column">
          <div>
            <strong>{t("alerts.lowQuota")}</strong>
            <span>{t("alerts.help")}</span>
          </div>
          <div className="settings-inline">
            <label className="switch">
              <input
                type="checkbox"
                checked={alertEnabled}
                onChange={(event) => setAlertEnabled(event.target.checked)}
              />
              <span>{alertEnabled ? t("common.enabled") : t("common.disabled")}</span>
            </label>
            <div className="threshold-picker" role="group" aria-label={t("alerts.threshold")}>
              {THRESHOLD_OPTIONS.map((value) => (
                <button
                  key={value}
                  className={thresholds.includes(value) ? "selected" : ""}
                  disabled={!alertEnabled}
                  onClick={() => toggleThreshold(value)}
                >
                  {value}%
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="settings-row">
          <div>
            <strong>{t("history.retention")}</strong>
            <span>{t("history.retentionHelp")}</span>
          </div>
          <select value={retention} onChange={(event) => setRetention(Number(event.target.value))}>
            {RETENTION_OPTIONS.map((value) => (
              <option key={value} value={value}>
                {t("history.retain", { days: value })}
              </option>
            ))}
          </select>
        </div>
        <div className="settings-row">
          <span className="settings-hint">{t("history.cleanupHelp")}</span>
          <button className="button button-primary" disabled={busy} onClick={() => void saveMonitoring()}>
            {t("monitoring.save")}
          </button>
        </div>
      </div>

      <div className="settings-card">
        <div className="settings-row">
          <div>
            <strong>{t("data.export")}</strong>
            <span>{t("data.exportHelp")}</span>
          </div>
          <div className="settings-inline">
            <button className="button button-quiet" disabled={busy} onClick={() => void runExport("json")}>
              <DownloadSimpleIcon /> {t("data.exportJson")}
            </button>
            <button className="button button-quiet" disabled={busy} onClick={() => void runExport("csv")}>
              <DownloadSimpleIcon /> {t("data.exportCsv")}
            </button>
          </div>
        </div>
      </div>

      <div className="settings-card danger">
        <div className="settings-row column">
          <div>
            <strong>{t("data.deleteLocal")}</strong>
            <span>{t("data.deleteHelp")}</span>
          </div>
          <div className="settings-inline">
            <input
              aria-label={t("data.confirmLabel")}
              placeholder={t("data.confirmPlaceholder")}
              value={clearText}
              onChange={(event) => {
                setClearText(event.target.value);
              }}
            />
            <button
              className="button button-danger"
              disabled={!confirmClear || busy}
              onClick={() => void clearAll()}
            >
              <TrashIcon /> {t("data.delete")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
