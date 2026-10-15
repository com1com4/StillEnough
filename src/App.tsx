import { useCallback, useEffect, useRef, useState } from "react";
import { ConnectionEditor } from "./components/ConnectionEditor";
import {
  balanceText,
  balanceLabel,
  fetchStatusLabel,
  metricSourceLabel,
  refreshState,
} from "./lib/presentation";
import { api } from "./lib/ipc";
import type {
  Account,
  AccountInput,
  AlertEvent,
  AppSettings,
  HistorySeries,
  WidgetOptions,
} from "./lib/types";
import "./App.css";
import {
  HouseIcon,
  ClockIcon,
  GearIcon,
  SquaresFourIcon,
  PlusIcon,
  ArrowsClockwiseIcon,
  DotsThreeIcon,
  TrashIcon,
  BellIcon,
  BellRingingIcon,
  CheckIcon,
  InfoIcon,
} from "@phosphor-icons/react";
import { ProviderIcon } from "./components/ProviderIcon";
import { Widget } from "./widget/Widget";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import {
  alertHeadline,
  alertDetail,
  alertTime,
  metricState,
  metricLabel,
  orderedMetrics,
  resetLabel,
} from "./lib/presentation";
import { HistoryPage } from "./components/HistoryPage";
import { applyTheme, watchSystemTheme } from "./lib/theme";
import type { ThemeMode } from "./lib/theme";
import { SettingsPage } from "./components/SettingsPage";
import { AboutPage } from "./components/AboutPage";
import { APP_VERSION } from "./lib/version";

const blankAccount = (): AccountInput => ({
  id: null,
  providerId: "",
  alias: "",
  baseUrl: "",
  enabled: false,
  cliAuthorized: false,
});

const defaultSettings = (): AppSettings => ({
  refreshMinutes: 10,
  widgetVisible: false,
  alertEnabled: true,
  alertThresholds: [30, 20, 10, 5],
  historyRetentionDays: 90,
  collectionPaused: false,
  autostart: false,
  widgetOpacity: 100,
  widgetCollapsed: false,
  widgetPinned: true,
  widgetAccounts: null,
  widgetDisplay: { showAlias: false, showProgress: true, showReset: true, maxAccounts: 4 },
  theme: "system",
});

// 每个页面一句话说明，跟标题一起构成完整的页面头部；页面正文不再重复标题。
const PAGE_SUMMARY: Record<string, string> = {
  总览: "集中查看额度、重置时间与 API 账号余额。",
  历史: "按账号查看额度与余额的实际观测；重置或长时间断档会断开曲线，不做插值。",
  设置: "管理本机窗口、外观、额度提醒和历史数据。",
  关于: "版本、产品信息与数据说明。",
};

const formatObserved = (value: string | null) => {
  if (!value) return "暂无观测";
  const date = new Date(value);
  return Number.isNaN(date.valueOf())
    ? "暂无观测"
    : `更新于 ${new Intl.DateTimeFormat("zh-CN", { hour: "2-digit", minute: "2-digit" }).format(date)}`;
};

function App() {
  const isWidget = new URLSearchParams(window.location.search).get("view") === "widget";
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [refreshingIds, setRefreshingIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [activePage, setActivePage] = useState("总览");
  const [editing, setEditing] = useState<AccountInput | null>(null);
  const [sort, setSort] = useState("default");
  const [now, setNow] = useState(() => Date.now());
  const [alerts, setAlerts] = useState<AlertEvent[]>([]);
  const [unread, setUnread] = useState(0);
  const [alertPanel, setAlertPanel] = useState(false);
  const [historyDays, setHistoryDays] = useState<7 | 30>(7);
  const [historyAccountId, setHistoryAccountId] = useState("");
  const [history, setHistory] = useState<{ key: string; series: HistorySeries[] }>({
    key: "",
    series: [],
  });
  // 各平台额度口径不同，历史一次只看一个账号；未手动选择时用第一个账号。
  const historyAccount = historyAccountId || accounts[0]?.id || "";
  const revisionRef = useRef(0);

  const visibleAccounts = accounts.map((account) => ({
    ...account,
    refreshing: account.refreshing || refreshingIds.includes(account.id),
  }));
  const sortedAccounts = [...visibleAccounts].sort((a, b) => {
    if (sort === "default") return 0;
    const score = (account: Account) =>
      Math.min(
        ...account.metrics
          .filter(
            (metric) =>
              metric.kind === "quota" &&
              metricState(metric) === "current" &&
              metric.remainingPercent !== null,
          )
          .map((metric) => metric.remainingPercent!),
        Infinity,
      );
    return score(a) - score(b);
  });

  const reload = useCallback(async () => {
    // Snapshot under a known revision and drop the read if the backend moved on
    // while it was in flight, so a slow response cannot repaint stale numbers.
    const started = await api.stateRevision().catch(() => 0);
    try {
      const [nextSettings, nextAccounts] = await Promise.all([api.settings(), api.accounts()]);
      if (revisionRef.current > started) return;
      setSettings(nextSettings);
      setAccounts(nextAccounts);
      setError("");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "无法连接桌面应用后台");
    } finally {
      setLoading(false);
    }
  }, []);

  const loadAlerts = useCallback(() => Promise.all([api.alertEvents(), api.unreadAlertCount()]), []);
  const reloadAlerts = useCallback(async () => {
    try {
      const [events, pending] = await loadAlerts();
      setAlerts(events);
      setUnread(pending);
    } catch {
      // A failed reminder read must not replace the dashboard with an error.
    }
  }, [loadAlerts]);

  useEffect(() => {
    let active = true;
    Promise.all([api.settings(), api.accounts()])
      .then(([nextSettings, nextAccounts]) => {
        if (!active) return;
        setSettings(nextSettings);
        setAccounts(nextAccounts);
        setError("");
      })
      .catch((reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : "无法连接桌面应用后台");
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    void loadAlerts()
      .then(([events, pending]) => {
        if (!active) return;
        setAlerts(events);
        setUnread(pending);
      })
      .catch(() => {
        // Reminders are secondary; the dashboard still loads without them.
      });
    return () => {
      active = false;
    };
  }, [loadAlerts]);

  // 主题只由 settings.theme 决定；跟随系统时由系统的外观变化驱动重绘。
  useEffect(() => {
    applyTheme(settings.theme);
    return watchSystemTheme(() => settings.theme);
  }, [settings.theme]);

  useEffect(() => {
    const tick = window.setInterval(() => {
      if (!document.hidden) setNow(Date.now());
    }, 1000);
    const onFocus = () => {
      setNow(Date.now());
      void reload();
      void reloadAlerts();
    };
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(tick);
      window.removeEventListener("focus", onFocus);
    };
  }, [reload, reloadAlerts]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const releases: (() => void)[] = [];
    const subscribe = (event: string, handler: (payload: unknown) => void) =>
      listen(event, ({ payload }) => handler(payload))
        .then((release) => {
          if (disposed) release();
          else releases.push(release);
        })
        .catch((reason: unknown) => setError(String(reason)));
    void subscribe("state-changed", (payload) => {
      const next = Number(payload);
      if (Number.isFinite(next) && next > revisionRef.current) revisionRef.current = next;
      void reload();
    });
    void subscribe("alerts-changed", () => void reloadAlerts());
    return () => {
      disposed = true;
      releases.forEach((release) => release());
    };
  }, [reload, reloadAlerts]);

  useEffect(() => {
    if (isWidget || activePage !== "历史" || !historyAccount) return;
    let active = true;
    const key = `history:${historyDays}:${historyAccount}`;
    api
      .history(historyDays, historyAccount)
      .then((series) => {
        if (active) setHistory({ key, series });
      })
      .catch((reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : "历史读取失败");
      });
    return () => {
      active = false;
    };
  }, [activePage, historyDays, historyAccount, isWidget]);
  const historyLoading =
    isWidget || activePage !== "历史" || history.key !== `history:${historyDays}:${historyAccount}`;

  const warningCount = accounts.reduce(
    (count, account) =>
      count +
      account.metrics.filter(
        (metric) =>
          metric.kind === "quota" &&
          metricState(metric, now) === "current" &&
          metric.remainingPercent !== null &&
          metric.remainingPercent <= 20,
      ).length,
    0,
  );

  function openEditor(account?: Account) {
    setEditing(
      account
        ? {
            id: account.id,
            providerId: account.providerId,
            alias: account.alias,
            baseUrl: account.baseUrl ?? "",
            enabled: account.enabled,
            cliAuthorized: account.cliAuthorized ?? false,
            network: account.network,
          }
        : blankAccount(),
    );
  }

  async function remove(account: Account) {
    if (!window.confirm(`删除「${account.alias}」及其本工具保存的历史数据？`)) return;
    try {
      await api.removeAccount(account.id);
      setNotice("账号已删除");
      await reload();
      await reloadAlerts();
    } catch (reason) {
      await reload();
      setError(reason instanceof Error ? reason.message : "删除失败");
    }
  }

  async function refresh(account?: Account) {
    const targets = (account ? [account] : accounts).filter(
      (item) => !refreshState(item).disabled && !refreshingIds.includes(item.id),
    );
    if (!targets.length) return;
    setRefreshingIds((ids) => [...new Set([...ids, ...targets.map((item) => item.id)])]);
    const results = await Promise.allSettled(targets.map((item) => api.refreshAccount(item.id)));
    await reload();
    await reloadAlerts();
    setRefreshingIds((ids) => ids.filter((id) => !targets.some((item) => item.id === id)));
    const completed = results.filter((result) => result.status === "fulfilled").length;
    if (!isWidget) {
      setNotice(
        completed === targets.length
          ? `已更新 ${completed} 个账号`
          : `已更新 ${completed} 个账号，${targets.length - completed} 个暂未更新，请查看账号状态`,
      );
    }
  }

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(""), 3500);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  async function toggleWidget() {
    try {
      // 后端回报浮窗实际是否可见，这里只跟随它，不自己翻转按钮状态。
      const actual = await api.setWidgetVisible(!settings.widgetVisible);
      setSettings((current) => ({ ...current, widgetVisible: actual }));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "浮窗操作失败");
    }
  }

  async function changeRefreshInterval(minutes: number) {
    try {
      setSettings(await api.updateRefreshInterval(minutes));
      setNotice("刷新间隔设置已保存");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "保存刷新间隔失败");
    }
  }

  async function changePause(paused: boolean) {
    try {
      setSettings(await api.setCollectionPaused(paused));
      setNotice(paused ? "已暂停后台采集" : "已恢复后台采集，正在重新读取");
      if (!paused) void reload();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "切换采集状态失败");
    }
  }

  async function changeAutostart(enabled: boolean) {
    try {
      const saved = await api.setAutostart(enabled);
      setSettings(saved);
      // 后端可能因权限拒绝而未写入，注册表里的真实状态才是准的。
      setNotice(
        saved.autostart === enabled
          ? enabled
            ? "已设置为开机自动启动"
            : "已取消开机自动启动"
          : "开机自动启动设置未能生效",
      );
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "设置开机自动启动失败");
    }
  }

  async function changeTheme(theme: ThemeMode) {
    try {
      setSettings(await api.setTheme(theme));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "保存外观失败");
    }
  }

  async function changeWidgetOptions(options: WidgetOptions) {
    try {
      setSettings(await api.setWidgetOptions(options));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "保存浮窗设置失败");
    }
  }

  async function markAlert(id: number) {
    try {
      setUnread(await api.markAlertRead(id));
      setAlerts((current) =>
        current.map((event) => (event.id === id ? { ...event, readAt: new Date().toISOString() } : event)),
      );
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "提醒状态更新失败");
    }
  }

  async function markAllAlerts() {
    try {
      await api.markAllAlertsRead();
      setUnread(0);
      setAlerts((current) =>
        current.map((event) => ({ ...event, readAt: event.readAt ?? new Date().toISOString() })),
      );
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "提醒状态更新失败");
    }
  }

  async function hideWidget() {
    try {
      await api.setWidgetVisible(false);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  async function openManager() {
    try {
      await api.openMainWindow();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

  if (isWidget) {
    return (
      <Widget
        accounts={visibleAccounts}
        settings={settings}
        loading={loading}
        error={error}
        onHide={() => void hideWidget()}
        onOpen={() => void openManager()}
        onReload={() => void refresh()}
      />
    );
  }

  return (
    <div
      className="app-shell"
      onContextMenu={(event) => {
        // WebView2 的默认右键菜单提供检查元素等开发用途，对本应用没有意义。
        event.preventDefault();
      }}
    >
      <aside className="sidebar">
        <div className="brand">
          <img className="brand-mark" src="/brand/stillenough.png" alt="" />
          <div>
            <strong>StillEnough</strong>
            <span>AI 额度与余额监控</span>
          </div>
        </div>
        <div className="sidebar-label">工作区</div>
        <nav>
          {["总览", "历史", "设置", "关于"].map((page, index) => (
            <button
              key={page}
              className={`nav-item ${activePage === page ? "selected" : ""}`}
              onClick={() => setActivePage(page)}
            >
              <span className="nav-icon">
                {
                  [
                    <HouseIcon key="home" />,
                    <ClockIcon key="clock" />,
                    <GearIcon key="gear" />,
                    <InfoIcon key="about" />,
                  ][index]
                }
              </span>
              {page}
              {page === "总览" && unread > 0 && <span className="nav-count">{unread}</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-spacer" />
        <div className="sidebar-footer">
          StillEnough <span>v{APP_VERSION}</span>
        </div>
      </aside>

      <main className="main-panel">
        <header className="topbar">
          <div>
            <h1 className="page-title">{activePage === "总览" ? "额度总览" : activePage}</h1>
            <p className="page-subtitle">{PAGE_SUMMARY[activePage]}</p>
          </div>
          <div className="top-actions">
            <button
              className="button button-quiet alert-button"
              aria-label={unread > 0 ? `额度提醒，${unread} 条未读` : "额度提醒"}
              title={unread > 0 ? `${unread} 条未读提醒` : "暂无未读提醒"}
              aria-expanded={alertPanel}
              onClick={() => setAlertPanel((open) => !open)}
            >
              {unread > 0 ? <BellRingingIcon /> : <BellIcon />}
              {unread > 0 && <span className="alert-badge">{unread}</span>}
            </button>
            <button className="button button-quiet" onClick={() => void toggleWidget()}>
              <SquaresFourIcon />
              浮窗
            </button>
            {activePage === "总览" && (
              <>
                <button
                  className="button button-quiet"
                  aria-label="刷新"
                  title="刷新可更新的账号；刚刚更新或限流中的账号会跳过"
                  disabled={!visibleAccounts.some((account) => !refreshState(account, now).disabled)}
                  onClick={() => void refresh()}
                >
                  <ArrowsClockwiseIcon
                    className={
                      visibleAccounts.some((account) => account.refreshing) ? "refresh-spinning" : ""
                    }
                  />
                </button>
                <button className="button button-primary" onClick={() => openEditor()}>
                  <PlusIcon />
                  添加账号
                </button>
              </>
            )}
          </div>
        </header>

        {alertPanel && (
          <section className="alert-panel" aria-label="额度提醒">
            <div className="alert-panel-head">
              <strong>额度提醒</strong>
              <div className="alert-panel-actions">
                <button className="text-button" disabled={!unread} onClick={() => void markAllAlerts()}>
                  <CheckIcon /> 全部标记已读
                </button>
                <button className="text-button" onClick={() => setAlertPanel(false)}>
                  关闭
                </button>
              </div>
            </div>
            {alerts.length ? (
              <ul className="alert-list">
                {alerts.map((event) => (
                  <li key={event.id} className={event.readAt ? "alert-item read" : "alert-item"}>
                    <button onClick={() => void markAlert(event.id)}>
                      <span className="alert-title">{alertHeadline(event)}</span>
                      <span className="alert-detail">{alertDetail(event)}</span>
                      <span className="alert-time">{alertTime(event.triggeredAt, now)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="alert-empty">还没有低额度提醒。额度低于提醒线时会在这里出现。</p>
            )}
          </section>
        )}

        <section className="content-area">
          {error && (
            <div className="callout error-callout">
              <span>!</span>
              <div>
                <strong>操作或数据读取未完成</strong>
                <small>{error}</small>
              </div>
              <button onClick={() => void reload()}>重试</button>
            </div>
          )}
          {notice && !error && (
            <div className="toast-notice">
              {notice}
              <button onClick={() => setNotice("")}>×</button>
            </div>
          )}

          {activePage === "总览" ? (
            <>
              <section className="summary-grid">
                <SummaryCard
                  label="监控账号"
                  value={`${accounts.length}`}
                  unit="个"
                  icon=""
                  tone="blue"
                  foot="已添加账号"
                />
                <SummaryCard
                  label="接近额度提醒线"
                  value={`${warningCount}`}
                  unit="个窗口"
                  icon=""
                  tone="amber"
                  foot={`提醒线 ${settings.alertThresholds.join("% / ")}%`}
                />
              </section>

              <div className="section-heading">
                <div>
                  <h2>我的账号</h2>
                  <span>{accounts.length} 个账号</span>
                </div>
                <select aria-label="账号排序" value={sort} onChange={(event) => setSort(event.target.value)}>
                  <option value="default">默认顺序</option>
                  <option value="quota">接近限额优先</option>
                </select>
              </div>

              {loading ? (
                <div className="loading-state">
                  <span className="spinner" /> 正在加载…
                </div>
              ) : accounts.length ? (
                <div className="account-grid">
                  {sortedAccounts.map((account) => (
                    <AccountCard
                      key={account.id}
                      account={account}

                      onEdit={() => openEditor(account)}
                      onRemove={() => void remove(account)}
                      onRefresh={() => void refresh(account)}
                    />
                  ))}
                </div>
              ) : (
                <div className="empty-state">
                  <SquaresFourIcon className="empty-icon" />
                  <h3>{error ? "等待桌面后台连接" : "添加你的第一个账号"}</h3>
                  <p>{error ? "请重新打开应用后重试。" : "连接 AI 平台，查看额度与余额。"}</p>
                  {!error && (
                    <button className="button button-primary" onClick={() => openEditor()}>
                      ＋ 添加第一个账号
                    </button>
                  )}
                </div>
              )}
            </>
          ) : activePage === "关于" ? (
            <AboutPage />
          ) : activePage === "设置" ? (
            <SettingsPage
              settings={settings}
              accounts={accounts}
              onRefreshInterval={changeRefreshInterval}
              onToggleWidget={toggleWidget}
              onPauseChange={changePause}
              onAutostartChange={changeAutostart}
              onWidgetOptions={changeWidgetOptions}
              onThemeChange={changeTheme}
              onNotice={setNotice}
              onError={setError}
              onCleared={() => {
                void reload();
                void reloadAlerts();
              }}
            />
          ) : (
            <HistoryPage
              series={history.key === `history:${historyDays}:${historyAccount}` ? history.series : []}
              loading={historyLoading}
              days={historyDays}
              onDaysChange={setHistoryDays}
              accounts={accounts}
              accountId={historyAccount}
              onAccountChange={setHistoryAccountId}
            />
          )}
        </section>
      </main>

      {editing && (
        <ConnectionEditor
          value={editing}
          onChange={setEditing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            setNotice("连接已保存");
            void reload();
          }}
        />
      )}
    </div>
  );
}

function SummaryCard({
  label,
  value,
  unit,
  icon,
  tone,
  foot,
}: {
  label: string;
  value: string;
  unit: string;
  icon: string;
  tone: string;
  foot: string;
}) {
  return (
    <div className="summary-card">
      <div className="summary-top">
        <span>{label}</span>
        <span className={`summary-icon ${tone}`}>{icon}</span>
      </div>
      <div className="summary-value">
        {value}
        <small>{unit}</small>
      </div>
      <div className="summary-foot">{foot}</div>
    </div>
  );
}

function AccountCard({
  account,
  onEdit,
  onRemove,
  onRefresh,
}: {
  account: Account;
  onEdit: () => void;
  onRemove: () => void;
  onRefresh: () => void;
}) {
  const refresh = refreshState(account);
  return (
    <article className="account-card">
      <div className="account-card-top">
        <div className="provider-mark">
          <ProviderIcon product={account.productName} />
        </div>
        <div className="account-heading">
          <div className="account-name-row">
            <h3>{account.productName}</h3>
          </div>
          <p>{account.alias}</p>
        </div>
        <button className="more-button" onClick={onEdit} aria-label={`编辑${account.productName}`}>
          <DotsThreeIcon />
        </button>
      </div>
      <div className="metrics-list">
        {account.metrics.length ? (
          orderedMetrics(account.metrics).map((metric) => {
            const state = metricState(metric);
            const unknown = state === "unknown";
            const stale = state === "stale";
            return (
              <div className={`metric-row state-${state}`} key={metric.metricId}>
                <span className="metric-title" title={metric.label}>
                  {metric.kind === "balance" ? balanceLabel(metric) : metricLabel(metric.label)}
                </span>
                {unknown ? (
                  <div className="metric-unknown">
                    暂无数据<span>尚未记录额度</span>
                  </div>
                ) : metric.kind === "quota" ? (
                  <>
                    <div className="metric-bar">
                      <i
                        className={!stale && metric.remainingPercent! <= 20 ? "bar-low" : ""}
                        style={{ width: `${metric.remainingPercent}%` }}
                      />
                    </div>
                    <div className="metric-value">
                      <strong className={!stale && metric.remainingPercent! <= 20 ? "low-value" : ""}>
                        {metric.remainingPercent}%
                      </strong>
                      <span>{stale ? "上次剩余" : "剩余"}</span>
                    </div>
                    <span className="metric-reset">
                      {stale ? "已过期 · 待更新" : resetLabel(metric.resetsAt)}
                    </span>
                  </>
                ) : (
                  <div className="balance-display">
                    <strong>{balanceText(metric)}</strong>
                    <small>
                      {metric.kind === "balance" ? "钱包余额" : "本地统计"}
                      {stale && " · 已过期"}
                    </small>
                  </div>
                )}
              </div>
            );
          })
        ) : (
          <div className="metric-placeholder">尚未添加额度窗口</div>
        )}
      </div>
      <div className="account-card-bottom">
        <div className="source-info">{`${Array.from(new Set(account.metrics.map((m) => metricSourceLabel(m.source)))).join(" / ") || "待连接"} · ${account.refreshing ? "正在刷新…" : account.fetchStatus === "rate_limited" ? refresh.label : fetchStatusLabel(account.fetchStatus)}`}</div>
        <div className="account-foot">
          <small>{formatObserved(account.lastSuccessAt)}</small>
          <div className="card-actions">
            <button
              onClick={onRefresh}
              disabled={refresh.disabled}
              title={refresh.label}
              aria-label={refresh.label}
            >
              <ArrowsClockwiseIcon className={account.refreshing ? "refresh-spinning" : ""} />
            </button>
            <button onClick={onEdit}>编辑</button>
            <button className="delete-button" onClick={onRemove} aria-label={`删除${account.productName}`}>
              <TrashIcon />
            </button>
          </div>
        </div>
      </div>
    </article>
  );
}

export default App;
