import { NetworkSettings } from "./NetworkSettings";
import { useRef, useState } from "react";
import { reorderWidgetAccounts } from "../lib/widgetOrder";
import { DownloadSimpleIcon, TrashIcon } from "@phosphor-icons/react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api } from "../lib/ipc";
import type { Account, AppSettings, WidgetOptions } from "../lib/types";
import { themeOptions, resolveTheme } from "../lib/theme";
import type { ThemeMode } from "../lib/theme";

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
  const [confirmClear, setConfirmClear] = useState(false);

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
      onNotice("额度提醒与历史设置已保存");
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
      onNotice(`已导出 ${summary.accountCount} 个账号、${summary.rowCount} 行到 ${summary.fileName}`);
      try {
        await revealItemInDir(summary.path);
      } catch {
        // The file exists either way; failing to open Explorer must not fail the export.
      }
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "导出失败");
    } finally {
      setBusy(false);
    }
  }

  async function clearAll() {
    if (!window.confirm("这会删除本工具保存的全部账号、历史和提醒，并清理系统凭据。确定继续？")) {
      setConfirmClear(false);
      setClearText("");
      return;
    }
    setBusy(true);
    try {
      const removed = await api.clearAllData(clearText);
      setClearText("");
      setConfirmClear(false);
      onNotice(`已删除 ${removed} 个账号及其本工具数据`);
      onCleared();
    } catch (reason) {
      onError(reason instanceof Error ? reason.message : "删除失败");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="secondary-page">
      <NetworkSettings accounts={accounts} />
      <div className="settings-card">
        <div className="settings-row column">
          <div>
            <strong>外观</strong>
            <span>
              {settings.theme === "system"
                ? `跟随系统，当前为${resolveTheme(settings.theme) === "dark" ? "夜间" : "白天"}模式`
                : `已固定为${resolveTheme(settings.theme) === "dark" ? "夜间" : "白天"}模式`}
              。切换后立即生效，不需要重启。
            </span>
          </div>
          <div className="theme-picker" role="group" aria-label="外观">
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
            <strong>自动刷新间隔</strong>
            <span>按此间隔刷新已启用的账号。</span>
          </div>
          <select
            value={settings.refreshMinutes}
            onChange={(event) => void onRefreshInterval(Number(event.target.value))}
          >
            <option value={5}>每 5 分钟</option>
            <option value={10}>每 10 分钟</option>
            <option value={15}>每 15 分钟</option>
            <option value={30}>每 30 分钟</option>
          </select>
        </div>
        <div className="settings-row">
          <div>
            <strong>暂停采集</strong>
            <span>停止后台自动刷新，已记录的额度和历史保留；手工刷新仍可使用。</span>
          </div>
          <label className="switch">
            <input
              type="checkbox"
              checked={settings.collectionPaused}
              onChange={(event) => void onPauseChange(event.target.checked)}
            />
            <span>{settings.collectionPaused ? "已暂停" : "采集中"}</span>
          </label>
        </div>
        <div className="settings-row">
          <div>
            <strong>开机自动启动</strong>
            <span>在当前 Windows 用户下登录时启动，不写入其他用户或系统级配置。</span>
          </div>
          <label className="switch">
            <input
              type="checkbox"
              checked={settings.autostart}
              onChange={(event) => void onAutostartChange(event.target.checked)}
            />
            <span>{settings.autostart ? "已开启" : "已关闭"}</span>
          </label>
        </div>
        <div className="settings-row">
          <div>
            <strong>额度浮窗</strong>
            <span>在桌面置顶查看关注账号。</span>
          </div>
          <button className="button button-quiet" onClick={() => void onToggleWidget()}>
            {settings.widgetVisible ? "隐藏浮窗" : "显示浮窗"}
          </button>
        </div>
        <div className="settings-row">
          <div>
            <strong>浮窗透明度</strong>
            <span>调低后浮窗内容会整体变淡，便于看穿下面的窗口。</span>
          </div>
          <div className="settings-inline">
            <input
              type="range"
              aria-label="浮窗透明度"
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
            <strong>浮窗显示内容</strong>
            <span>修改后立即保存。异常和过期状态始终显示。</span>
          </div>
          <div className="settings-inline">
            {(
              [
                ["showAlias", "账号别名"],
                ["showProgress", "额度进度条"],
                ["showReset", "重置时间"],
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
              显示账号数{" "}
              <select
                aria-label="浮窗显示账号数"
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
                    {count} 个
                  </option>
                ))}
              </select>
            </label>
          </div>
        </div>
        <div className="settings-row column">
          <div>
            <strong>浮窗关注账号</strong>
            <span>
              不勾选时按接近限额自动排序；勾选后拖动左侧手柄或用上移、下移调整顺序。松开后自动保存，超出显示数量的账号暂不展示。
            </span>
          </div>
          <div className="widget-account-picker widget-account-list" role="group" aria-label="浮窗关注账号">
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
                    aria-label={`拖动排序 ${account.alias}`}
                    title="按住拖动调整顺序；也可使用右侧上移、下移"
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
                      <small>{account.productName}</small>
                    </span>
                  </label>
                  {checked && (
                    <div className="widget-order-actions">
                      <button
                        type="button"
                        className="button button-quiet"
                        aria-label={`上移 ${account.alias}`}
                        disabled={widgetOrderBusy || index === 0}
                        onClick={() => moveWidgetAccount(index, -1)}
                      >
                        上移
                      </button>
                      <button
                        type="button"
                        className="button button-quiet"
                        aria-label={`下移 ${account.alias}`}
                        disabled={widgetOrderBusy || index === selected.length - 1}
                        onClick={() => moveWidgetAccount(index, 1)}
                      >
                        下移
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
            {!accounts.length && <span className="settings-hint">还没有账号。</span>}
          </div>
          {settings.widgetAccounts !== null && (
            <button
              className="button button-quiet"
              disabled={widgetOrderBusy}
              onClick={() => void saveWidgetAccounts(null)}
            >
              恢复自动排序
            </button>
          )}
        </div>
      </div>

      <div className="settings-card">
        <div className="settings-row column">
          <div>
            <strong>低额度提醒</strong>
            <span>只对能读到剩余百分比的额度窗口提醒；一次下跌只提醒最严重的一档，余额不按百分比提醒。</span>
          </div>
          <div className="settings-inline">
            <label className="switch">
              <input
                type="checkbox"
                checked={alertEnabled}
                onChange={(event) => setAlertEnabled(event.target.checked)}
              />
              <span>{alertEnabled ? "已开启" : "已关闭"}</span>
            </label>
            <div className="threshold-picker" role="group" aria-label="提醒线">
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
            <strong>历史保留</strong>
            <span>超期的观测记录会被清理；已记录的提醒不受影响。</span>
          </div>
          <select value={retention} onChange={(event) => setRetention(Number(event.target.value))}>
            {RETENTION_OPTIONS.map((value) => (
              <option key={value} value={value}>
                保留 {value} 天
              </option>
            ))}
          </select>
        </div>
        <div className="settings-row">
          <span className="settings-hint">保存后立即按新的保留期清理历史。</span>
          <button className="button button-primary" disabled={busy} onClick={() => void saveMonitoring()}>
            保存提醒与历史设置
          </button>
        </div>
      </div>

      <div className="settings-card">
        <div className="settings-row">
          <div>
            <strong>导出数据</strong>
            <span>导出本工具保存的账号与观测，不含凭据、凭据引用、连接地址和本地路径。</span>
          </div>
          <div className="settings-inline">
            <button className="button button-quiet" disabled={busy} onClick={() => void runExport("json")}>
              <DownloadSimpleIcon /> 导出 JSON
            </button>
            <button className="button button-quiet" disabled={busy} onClick={() => void runExport("csv")}>
              <DownloadSimpleIcon /> 导出 CSV
            </button>
          </div>
        </div>
      </div>

      <div className="settings-card danger">
        <div className="settings-row column">
          <div>
            <strong>删除全部本地数据</strong>
            <span>
              删除所有账号、观测历史和提醒，并清理保存在 Windows
              凭据管理器中的密钥。此操作不可撤销，建议先导出。
            </span>
          </div>
          <div className="settings-inline">
            <input
              aria-label="删除确认"
              placeholder="输入「删除全部数据」以启用"
              value={clearText}
              onChange={(event) => {
                setClearText(event.target.value);
                setConfirmClear(event.target.value === "删除全部数据");
              }}
            />
            <button
              className="button button-danger"
              disabled={!confirmClear || busy}
              onClick={() => void clearAll()}
            >
              <TrashIcon /> 删除全部数据
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
