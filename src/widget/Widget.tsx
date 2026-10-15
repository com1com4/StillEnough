import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowSquareOutIcon,
  ArrowsClockwiseIcon,
  CaretDownIcon,
  CaretUpIcon,
  PushPinIcon,
  XIcon,
} from "@phosphor-icons/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { Menu } from "@tauri-apps/api/menu";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import type { Account, AppSettings, WidgetOptions } from "../lib/types";
import { api } from "../lib/ipc";
import {
  balanceText,
  balanceLabel,
  refreshState,
  metricState,
  widgetMetrics,
  resetLabel,
  shortLabel,
  fetchStatusLabel,
  metricSourceLabel,
} from "../lib/presentation";
import { ProviderIcon } from "../components/ProviderIcon";
import { productBrand } from "../lib/brands";

export function Widget({
  accounts,
  settings,
  loading,
  error,
  onHide,
  onOpen,
  onReload,
}: {
  accounts: Account[];
  settings: AppSettings;
  loading: boolean;
  error: string;
  onHide: () => void;
  onOpen: () => void;
  onReload: () => void;
}) {
  const [operationError, setOperationError] = useState("");
  const shell = useRef<HTMLDivElement>(null);
  const contextMenu = useRef<Menu | null>(null);
  const openingMenu = useRef(false);
  const menuActions = useRef<Record<string, () => void>>({});

  // 折叠与置顶由后台设置直接驱动，浮窗不再维护会与之漂移的本地副本。
  const collapsed = settings.widgetCollapsed;
  const pinned = settings.widgetPinned;
  const display = settings.widgetDisplay;

  // 显式关注列表为空时退回自动排序，避免浮窗因为没有勾选而空白。
  const visibleAccounts = settings.widgetAccounts
    ? settings.widgetAccounts
        .map((id) => accounts.find((account) => account.id === id))
        .filter((account): account is Account => Boolean(account))
    : accounts;

  const persist = useCallback(
    (next: Partial<WidgetOptions>) => {
      void api
        .setWidgetOptions({
          collapsed: next.collapsed ?? collapsed,
          pinned: next.pinned ?? pinned,
          opacity: next.opacity ?? settings.widgetOpacity,
          accounts: "accounts" in next ? (next.accounts ?? null) : settings.widgetAccounts,
        })
        .catch((reason: unknown) => setOperationError(String(reason)));
    },
    [collapsed, pinned, settings.widgetOpacity, settings.widgetAccounts],
  );

  useEffect(() => {
    menuActions.current = {
      open: onOpen,
      refresh: onReload,
      collapse: toggleCollapsed,
      pin: () => persist({ pinned: !pinned }),
      hide: onHide,
      quit: () => {
        void api.quitApp().catch((reason: unknown) => setOperationError(String(reason)));
      },
    };
  });
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<string>("widget-menu-action", ({ payload }) => menuActions.current[payload]?.())
      .then((release) => {
        if (disposed) release();
        else unlisten = release;
      })
      .catch((reason: unknown) => setOperationError(String(reason)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  useEffect(
    () => () => {
      void contextMenu.current?.close();
    },
    [],
  );

  async function showContextMenu(x: number, y: number) {
    if (!isTauri() || openingMenu.current) return;
    openingMenu.current = true;
    try {
      await contextMenu.current?.close();
      const menu = await Menu.new({
        items: [
          { id: "stillenough-widget-open", text: "打开 StillEnough" },
          {
            id: "stillenough-widget-refresh",
            text: accounts.some((account) => account.refreshing) ? "正在刷新…" : "刷新额度",
            enabled: accounts.some((account) => !refreshState(account).disabled),
          },
          { item: "Separator" },
          {
            id: "stillenough-widget-collapse",
            text: collapsed ? "展开浮窗" : "折叠浮窗",
          },
          { id: "stillenough-widget-pin", text: pinned ? "取消置顶" : "置顶浮窗" },
          { item: "Separator" },
          { id: "stillenough-widget-hide", text: "隐藏浮窗" },
          { item: "Separator" },
          { id: "stillenough-widget-quit", text: "退出 StillEnough" },
        ],
      });
      contextMenu.current = menu;
      await menu.popup(new LogicalPosition(x, y), getCurrentWindow());
    } catch (reason) {
      setOperationError(String(reason));
    } finally {
      openingMenu.current = false;
    }
  }
  useEffect(() => {
    if (!isTauri() || !shell.current) return;
    let lastHeight = 0;
    const observer = new ResizeObserver(() => {
      const height = Math.min(420, Math.max(80, Math.ceil(shell.current!.getBoundingClientRect().height)));
      if (height === lastHeight) return;
      lastHeight = height;
      void getCurrentWindow()
        .setSize(new LogicalSize(280, height))
        .catch((reason: unknown) => setOperationError(String(reason)));
    });
    observer.observe(shell.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") onHide();
    };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onHide]);
  function toggleCollapsed() {
    persist({ collapsed: !collapsed });
  }
  return (
    <div
      ref={shell}
      className={`widget-shell ${collapsed ? "is-collapsed" : ""}`}
      style={{ opacity: settings.widgetOpacity / 100 }}
      tabIndex={0}
      onContextMenu={(event) => {
        event.preventDefault();
        void showContextMenu(event.clientX, event.clientY);
      }}
    >
      <div className="widget-drag" data-tauri-drag-region title="拖动浮窗" />
      <div className="widget-controls">
        <button
          onClick={() => persist({ pinned: !pinned })}
          aria-label={pinned ? "取消置顶" : "置顶"}
          aria-pressed={pinned}
        >
          <PushPinIcon weight={pinned ? "fill" : "regular"} />
        </button>
        <button onClick={toggleCollapsed} aria-label={collapsed ? "展开浮窗" : "折叠浮窗"}>
          {collapsed ? <CaretDownIcon /> : <CaretUpIcon />}
        </button>
        <button
          onClick={onReload}
          disabled={!accounts.some((account) => !refreshState(account).disabled)}
          aria-label="重新读取额度"
        >
          <ArrowsClockwiseIcon
            className={accounts.some((account) => account.refreshing) ? "refresh-spinning" : ""}
          />
        </button>
        <button onClick={onOpen} aria-label="打开管理器">
          <ArrowSquareOutIcon />
        </button>
        <button onClick={onHide} aria-label="隐藏浮窗">
          <XIcon />
        </button>
      </div>
      {operationError && (
        <div className="widget-operation-note">
          {operationError}
          <button onClick={() => setOperationError("")}>×</button>
        </div>
      )}
      {error && (
        <div className="widget-message" role="alert">
          <p>{error}</p>
          <button onClick={onReload}>重试读取</button>
        </div>
      )}
      {loading && !visibleAccounts.length ? (
        <p className="widget-empty">正在读取…</p>
      ) : !visibleAccounts.length ? (
        <div className="widget-message">
          <strong>尚未添加账号</strong>
          <button onClick={onOpen}>打开管理器</button>
        </div>
      ) : (
        <div className="widget-rows">
          {visibleAccounts.slice(0, display.maxAccounts).map((account) => (
            <button
              className="widget-row"
              key={account.id}
              onClick={onOpen}
              title={`${account.productName} · ${account.alias}；点击打开管理器`}
            >
              <span className="widget-identity">
                {display.showAlias && <small className="widget-alias">{account.alias}</small>}
                <span className="widget-product">
                  <ProviderIcon product={account.productName} />
                  <span>{productBrand(account.productName)?.name ?? account.productName}</span>
                </span>
                <small className="widget-account-status" aria-live="polite">
                  {account.refreshing
                    ? "正在刷新…"
                    : ["ok", "idle"].includes(account.fetchStatus)
                      ? ""
                      : account.fetchStatus === "rate_limited"
                        ? refreshState(account).label
                        : fetchStatusLabel(account.fetchStatus)}
                </small>
              </span>
              <span className="widget-metrics">
                {widgetMetrics(account).length ? (
                  widgetMetrics(account).map((metric) => {
                    const state = metricState(metric);
                    const unknown = state === "unknown";
                    return (
                      <span
                        className={`widget-metric state-${state} ${!unknown && state === "current" && metric.kind === "quota" && metric.remainingPercent! <= 20 ? "is-low" : ""}`}
                        key={metric.metricId}
                        title={`${metric.kind === "balance" ? balanceLabel(metric) : metric.label} · ${metricSourceLabel(metric.source)} · ${metric.observedAt}`}
                      >
                        <strong>
                          {unknown
                            ? "暂无数据"
                            : metric.kind === "quota"
                              ? `${collapsed ? `${shortLabel(metric.label)} ` : ""}${state === "stale" ? "上次 " : ""}${metric.remainingPercent}%`
                              : balanceText(metric, "widget")}
                        </strong>
                        {!collapsed && display.showProgress && !unknown && metric.kind === "quota" && (
                          <span className="widget-bar">
                            <i style={{ width: `${metric.remainingPercent}%` }} />
                          </span>
                        )}
                        {!collapsed && (
                          <small>
                            {unknown
                              ? `${metric.kind === "balance" ? balanceLabel(metric) : shortLabel(metric.label)} · 未记录`
                              : state === "stale"
                                ? `已过期 · ${Math.max(0, Math.floor((Date.now() - Date.parse(metric.observedAt)) / 60000)) || 0}分钟前`
                                : metric.kind === "quota"
                                  ? `${shortLabel(metric.label)}${display.showReset ? ` · ${resetLabel(metric.resetsAt)}` : ""}`
                                  : metric.kind === "balance"
                                    ? "钱包余额"
                                    : metric.label}
                          </small>
                        )}
                      </span>
                    );
                  })
                ) : (
                  <span className="widget-metric state-unknown">
                    <strong>暂无数据</strong>
                  </span>
                )}
              </span>
            </button>
          ))}
        </div>
      )}
      <div className="widget-footer">
        {visibleAccounts.length > display.maxAccounts && (
          <button className="widget-more" onClick={onOpen}>
            另有 {visibleAccounts.length - display.maxAccounts} 个账号
          </button>
        )}
      </div>
    </div>
  );
}
