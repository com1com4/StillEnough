import { t, applyLanguage, useLanguage, currentLanguage } from "./lib/i18n";
import type { LanguagePreference } from "./lib/i18n";
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
import { providerName } from "./lib/providerPresentation";

const blankAccount = (): AccountInput => ({
  id: null,
  providerId: "",
  alias: "",
  baseUrl: "",
  enabled: false,
  cliAuthorized: false,
});

const PAGES = ["overview", "history", "settings", "about"] as const;
type Page = (typeof PAGES)[number];

const defaultSettings = (): AppSettings => ({
  language: "system",
  resolvedLanguage: currentLanguage(),
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
const pageSummary = () => ({
  overview: t("overview.summary"),
  history: t("history.summary"),
  settings: t("settings.summary"),
  about: t("about.summary"),
});

const formatObserved = (value: string | null) => {
  if (!value) return t("observation.none");
  const date = new Date(value);
  return Number.isNaN(date.valueOf())
    ? t("observation.none")
    : t("observation.updated", {
        time: new Intl.DateTimeFormat(currentLanguage(), { hour: "2-digit", minute: "2-digit" }).format(date),
      });
};

function App() {
  useLanguage();
  const isWidget = new URLSearchParams(window.location.search).get("view") === "widget";
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [refreshingIds, setRefreshingIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [activePage, setActivePage] = useState<Page>("overview");
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
      setError(reason instanceof Error ? reason.message : t("error.backend"));
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
        if (active) setError(reason instanceof Error ? reason.message : t("error.backend"));
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
    applyLanguage(settings.resolvedLanguage);
  }, [settings.resolvedLanguage]);

  async function changeLanguage(language: LanguagePreference) {
    try {
      const saved = await api.setLanguage(language);
      applyLanguage(saved.resolvedLanguage);
      setSettings(saved);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  }

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
    if (isWidget || activePage !== "history" || !historyAccount) return;
    let active = true;
    const key = `history:${historyDays}:${historyAccount}`;
    api
      .history(historyDays, historyAccount)
      .then((series) => {
        if (active) setHistory({ key, series });
      })
      .catch((reason: unknown) => {
        if (active) setError(reason instanceof Error ? reason.message : t("error.history"));
      });
    return () => {
      active = false;
    };
  }, [activePage, historyDays, historyAccount, isWidget]);
  const historyLoading =
    isWidget || activePage !== "history" || history.key !== `history:${historyDays}:${historyAccount}`;

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
    if (!window.confirm(t("account.confirmDelete", { alias: account.alias }))) return;
    try {
      await api.removeAccount(account.id);
      setNotice(t("account.deleted"));
      await reload();
      await reloadAlerts();
    } catch (reason) {
      await reload();
      setError(reason instanceof Error ? reason.message : t("error.delete"));
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
          ? t("refresh.completed", { count: completed })
          : t("refresh.partial", { count: completed, failed: targets.length - completed }),
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
      setError(reason instanceof Error ? reason.message : t("error.widget"));
    }
  }

  async function changeRefreshInterval(minutes: number) {
    try {
      setSettings(await api.updateRefreshInterval(minutes));
      setNotice(t("refresh.saved"));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : t("error.refreshSave"));
    }
  }

  async function changePause(paused: boolean) {
    try {
      setSettings(await api.setCollectionPaused(paused));
      setNotice(paused ? t("collection.pausedNotice") : t("collection.resumedNotice"));
      if (!paused) void reload();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : t("error.collection"));
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
            ? t("autostart.enabledNotice")
            : t("autostart.disabledNotice")
          : t("autostart.notApplied"),
      );
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : t("error.autostart"));
    }
  }

  async function changeTheme(theme: ThemeMode) {
    try {
      setSettings(await api.setTheme(theme));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : t("error.theme"));
    }
  }

  async function changeWidgetOptions(options: WidgetOptions) {
    try {
      setSettings(await api.setWidgetOptions(options));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : t("error.widgetSave"));
    }
  }

  async function markAlert(id: number) {
    try {
      setUnread(await api.markAlertRead(id));
      setAlerts((current) =>
        current.map((event) => (event.id === id ? { ...event, readAt: new Date().toISOString() } : event)),
      );
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : t("error.alertUpdate"));
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
      setError(reason instanceof Error ? reason.message : t("error.alertUpdate"));
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
            <span>{t("app.tagline")}</span>
          </div>
        </div>
        <div className="sidebar-label">{t("nav.workspace")}</div>
        <nav>
          {PAGES.map((page, index) => (
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
              {t(`nav.${page}`)}
              {page === "overview" && unread > 0 && <span className="nav-count">{unread}</span>}
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
            <h1 className="page-title">
              {activePage === "overview" ? t("overview.title") : t(`nav.${activePage}`)}
            </h1>
            <p className="page-subtitle">{pageSummary()[activePage]}</p>
          </div>
          <div className="top-actions">
            <button
              className="button button-quiet alert-button"
              aria-label={unread > 0 ? t("alerts.unreadLabel", { count: unread }) : t("alerts.title")}
              title={unread > 0 ? t("alerts.unread", { count: unread }) : t("alerts.noneUnread")}
              aria-expanded={alertPanel}
              onClick={() => setAlertPanel((open) => !open)}
            >
              {unread > 0 ? <BellRingingIcon /> : <BellIcon />}
              {unread > 0 && <span className="alert-badge">{unread}</span>}
            </button>
            <button className="button button-quiet" onClick={() => void toggleWidget()}>
              <SquaresFourIcon />
              {t("widget.short")}
            </button>
            {activePage === "overview" && (
              <>
                <button
                  className="button button-quiet"
                  aria-label={t("refresh.action")}
                  title={t("refresh.help")}
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
                  {t("account.add")}
                </button>
              </>
            )}
          </div>
        </header>

        <section className="content-area" key={activePage}>
          {alertPanel && (
            <section className="alert-panel" aria-label={t("alerts.title")}>
              <div className="alert-panel-head">
                <strong>{t("alerts.title")}</strong>
                <div className="alert-panel-actions">
                  <button className="text-button" disabled={!unread} onClick={() => void markAllAlerts()}>
                    <CheckIcon /> {t("alerts.markAll")}
                  </button>
                  <button className="text-button" onClick={() => setAlertPanel(false)}>
                    {t("common.close")}
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
                <p className="alert-empty">{t("alerts.empty")}</p>
              )}
            </section>
          )}

          {error && (
            <div className="callout error-callout">
              <span>!</span>
              <div>
                <strong>{t("error.heading")}</strong>
                <small>{fetchStatusLabel(error)}</small>
              </div>
              <button onClick={() => void reload()}>{t("common.retry")}</button>
            </div>
          )}
          {notice && !error && (
            <div className="toast-notice">
              {fetchStatusLabel(notice)}
              <button onClick={() => setNotice("")}>×</button>
            </div>
          )}

          {activePage === "overview" ? (
            <>
              <section className="summary-grid">
                <SummaryCard
                  label={t("overview.accounts")}
                  value={`${accounts.length}`}
                  unit={t("common.countUnit")}
                  icon=""
                  tone="blue"
                  foot={t("overview.added")}
                />
                <SummaryCard
                  label={t("overview.nearThreshold")}
                  value={`${warningCount}`}
                  unit={t("overview.windowUnit")}
                  icon=""
                  tone="amber"
                  foot={t("alerts.thresholds", { values: settings.alertThresholds.join("% / ") })}
                />
              </section>

              <div className="section-heading">
                <div>
                  <h2>{t("overview.myAccounts")}</h2>
                  <span>{t("account.count", { count: accounts.length })}</span>
                </div>
                <select
                  aria-label={t("account.sort")}
                  value={sort}
                  onChange={(event) => setSort(event.target.value)}
                >
                  <option value="default">{t("account.defaultOrder")}</option>
                  <option value="quota">{t("account.quotaOrder")}</option>
                </select>
              </div>

              {loading ? (
                <div className="loading-state">
                  <span className="spinner" /> {t("common.loading")}
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
                  <h3>{error ? t("account.waitBackend") : t("account.first")}</h3>
                  <p>{error ? t("error.reopen") : t("account.connectHelp")}</p>
                  {!error && (
                    <button className="button button-primary" onClick={() => openEditor()}>
                      {t("account.addFirst")}
                    </button>
                  )}
                </div>
              )}
            </>
          ) : activePage === "about" ? (
            <AboutPage />
          ) : activePage === "settings" ? (
            <SettingsPage
              settings={settings}
              accounts={accounts}
              onRefreshInterval={changeRefreshInterval}
              onToggleWidget={toggleWidget}
              onPauseChange={changePause}
              onAutostartChange={changeAutostart}
              onWidgetOptions={changeWidgetOptions}
              onThemeChange={changeTheme}
              onLanguageChange={changeLanguage}
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
            setNotice(t("connection.saved"));
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
            <h3>{providerName(account.providerId, account.productName)}</h3>
          </div>
          <p>{account.alias}</p>
        </div>
        <button
          className="more-button"
          onClick={onEdit}
          aria-label={t("account.editLabel", {
            product: providerName(account.providerId, account.productName),
          })}
        >
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
                <span
                  className="metric-title"
                  title={metric.kind === "balance" ? balanceLabel(metric) : metricLabel(metric.label)}
                >
                  {metric.kind === "balance" ? balanceLabel(metric) : metricLabel(metric.label)}
                </span>
                {unknown ? (
                  <div className="metric-unknown">
                    {t("metric.noData")}
                    <span>{t("metric.notRecorded")}</span>
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
                      <span>{stale ? t("metric.previousRemaining") : t("metric.remaining")}</span>
                    </div>
                    <span className="metric-reset">
                      {stale ? t("metric.stalePending") : resetLabel(metric.resetsAt)}
                    </span>
                  </>
                ) : (
                  <div className="balance-display">
                    <strong>{balanceText(metric)}</strong>
                    <small>
                      {metric.kind === "balance" ? t("metric.wallet") : t("metric.local")}
                      {stale && t("metric.staleSuffix")}
                    </small>
                  </div>
                )}
              </div>
            );
          })
        ) : (
          <div className="metric-placeholder">{t("metric.noWindows")}</div>
        )}
      </div>
      <div className="account-card-bottom">
        <div className="source-info">{`${Array.from(new Set(account.metrics.map((m) => metricSourceLabel(m.source)))).join(" / ") || t("connection.pending")} · ${account.refreshing ? t("refresh.busy") : account.fetchStatus === "rate_limited" ? refresh.label : fetchStatusLabel(account.fetchStatus)}`}</div>
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
            <button onClick={onEdit}>{t("common.edit")}</button>
            <button
              className="delete-button"
              onClick={onRemove}
              aria-label={t("account.deleteLabel", {
                product: providerName(account.providerId, account.productName),
              })}
            >
              <TrashIcon />
            </button>
          </div>
        </div>
      </div>
    </article>
  );
}

export default App;
