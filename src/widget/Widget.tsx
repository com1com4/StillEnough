import { providerName } from "../lib/providerPresentation";
import { t } from "../lib/i18n";
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { Menu } from "@tauri-apps/api/menu";
import { listen } from "@tauri-apps/api/event";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Account, AppSettings, WidgetOptions } from "../lib/types";
import { api } from "../lib/ipc";
import {
  balanceText,
  balanceLabel,
  refreshState,
  metricState,
  metricLabel,
  rotatingWidgetMetrics,
  resetLabel,
  shortLabel,
  fetchStatusLabel,
  metricSourceLabel,
  observationTime,
  percentText,
  widgetMetricLabel,
} from "../lib/presentation";
import { ProviderIcon } from "../components/ProviderIcon";
import { productBrand } from "../lib/brands";
import { startWidgetRotation } from "../lib/widgetRotation";
import { widgetGlassOpacity } from "../lib/widgetGlassOpacity";

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
  const [rotation, setRotation] = useState<Record<string, number>>({});
  const hoveredAccount = useRef<string | null>(null);
  const focusedAccount = useRef<string | null>(null);
  const rotationAccounts = useRef(accounts);
  useEffect(() => {
    rotationAccounts.current = accounts;
  }, [accounts]);
  useEffect(() => {
    return startWidgetRotation(
      () => rotationAccounts.current.map((account) => account.id),
      (id) => document.hidden || id === hoveredAccount.current || id === focusedAccount.current,
      (ids) =>
        setRotation((previous) => {
          const next = { ...previous };
          for (const id of ids) next[id] = (previous[id] ?? 0) + 1;
          return next;
        }),
    );
  }, []);
  const shell = useRef<HTMLDivElement>(null);
  const glassOpacityQueue = useRef(Promise.resolve());
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
      collapse: () => persist({ collapsed: !collapsed }),
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
          { id: "stillenough-widget-open", text: t("widget.open") },
          {
            id: "stillenough-widget-refresh",
            text: accounts.some((account) => account.refreshing) ? t("refresh.busy") : t("widget.refresh"),
            enabled: accounts.some((account) => !refreshState(account).disabled),
          },
          { item: "Separator" },
          {
            id: "stillenough-widget-collapse",
            text: collapsed ? t("widget.expand") : t("widget.collapse"),
          },
          { id: "stillenough-widget-pin", text: pinned ? t("widget.unpin") : t("widget.pin") },
          { item: "Separator" },
          { id: "stillenough-widget-hide", text: t("widget.hide") },
          { item: "Separator" },
          { id: "stillenough-widget-quit", text: t("app.quit") },
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
  useEffect(() => {
    const element = shell.current;
    if (!element || !display.frostedGlass || !isTauri()) return;
    let disposed = false;
    const update = () => {
      if (disposed) return;
      const interacting = element.matches(":hover, :focus-visible, :has(:focus-visible)");
      const opacity = widgetGlassOpacity(settings.widgetOpacity, display.idleOpacity, display.fadeWhenIdle, interacting);
      glassOpacityQueue.current = glassOpacityQueue.current.catch(() => {}).then(() =>
        invoke<void>("set_widget_glass_opacity", { opacity }).catch((reason: unknown) => {
          if (!disposed) setOperationError(String(reason));
        }),
      );
    };
    const updateAfterFocus = () => { queueMicrotask(update); };
    update();
    element.addEventListener("mouseenter", update);
    element.addEventListener("mouseleave", update);
    element.addEventListener("focusin", updateAfterFocus);
    element.addEventListener("focusout", updateAfterFocus);
    return () => {
      disposed = true;
      element.removeEventListener("mouseenter", update);
      element.removeEventListener("mouseleave", update);
      element.removeEventListener("focusin", updateAfterFocus);
      element.removeEventListener("focusout", updateAfterFocus);
    };
  }, [display.frostedGlass, display.fadeWhenIdle, display.idleOpacity, settings.widgetOpacity]);
  return (
    <div
      ref={shell}
      className={`widget-shell ${collapsed ? "is-collapsed" : ""} ${!display.frostedGlass && display.fadeWhenIdle ? "fade-when-idle" : ""} ${display.frostedGlass ? "frosted-glass" : ""}`}
      style={
        {
          "--widget-opacity": settings.widgetOpacity / 100,
          "--widget-idle-opacity": display.idleOpacity / 100,
        } as CSSProperties
      }
      tabIndex={0}
      onContextMenu={(event) => {
        event.preventDefault();
        void showContextMenu(event.clientX, event.clientY);
      }}
    >
      {operationError && (
        <div className="widget-operation-note">
          {fetchStatusLabel(operationError)}
          <button onClick={() => setOperationError("")}>×</button>
        </div>
      )}
      {error && (
        <div className="widget-message" role="alert">
          <p>{fetchStatusLabel(error)}</p>
          <button onClick={onReload}>{t("widget.retry")}</button>
        </div>
      )}
      {loading && !visibleAccounts.length ? (
        <p className="widget-empty" data-tauri-drag-region title={t("widget.drag")}>
          {t("common.reading")}
        </p>
      ) : !visibleAccounts.length ? (
        <div className="widget-message">
          <strong data-tauri-drag-region title={t("widget.drag")}>
            {t("account.noneAdded")}
          </strong>
          <button onClick={onOpen}>{t("widget.manager")}</button>
        </div>
      ) : (
        <div className="widget-rows">
          {visibleAccounts.slice(0, display.maxAccounts).map((account) => (
            <button
              className="widget-row"
              key={account.id}
              onMouseEnter={() => {
                hoveredAccount.current = account.id;
              }}
              onMouseLeave={() => {
                hoveredAccount.current = null;
              }}
              onFocus={(event) => {
                if (event.currentTarget.matches(":focus-visible")) focusedAccount.current = account.id;
              }}
              onBlur={() => {
                focusedAccount.current = null;
              }}
              onClick={(event) => {
                if (event.target instanceof Element && event.target.closest(".widget-identity")) return;
                onOpen();
              }}
              title={t("widget.accountTitle", {
                product: providerName(account.providerId, account.productName),
                alias: account.alias,
              })}
            >
              <span className="widget-identity" data-tauri-drag-region title={t("widget.drag")}>
                {display.showAlias && <small className="widget-alias">{account.alias}</small>}
                <span className="widget-product">
                  <ProviderIcon product={account.productName} />
                  <span>
                    {productBrand(account.productName)?.name ??
                      providerName(account.providerId, account.productName)}
                  </span>
                </span>
                <small className="widget-account-status" aria-live="polite">
                  {account.refreshing
                    ? t("refresh.busy")
                    : ["ok", "idle"].includes(account.fetchStatus)
                      ? ""
                      : account.fetchStatus === "rate_limited"
                        ? refreshState(account).label
                        : fetchStatusLabel(account.fetchStatus)}
                </small>
              </span>
              <span className="widget-metrics">
                {rotatingWidgetMetrics(account, rotation[account.id] ?? 0).length ? (
                  rotatingWidgetMetrics(account, rotation[account.id] ?? 0).map((metric) => {
                    const state = metricState(metric);
                    const unknown = state === "unknown";
                    return (
                      <span
                        className={`widget-metric state-${state} ${!unknown && state === "current" && metric.kind === "quota" && metric.remainingPercent! <= 20 ? "is-low" : ""}`}
                        key={metric.metricId}
                        title={`${metric.kind === "balance" ? balanceLabel(metric) : metricLabel(metric.label)} · ${metricSourceLabel(metric.source)} · ${t("observation.updated", { time: observationTime(metric.observedAt) })}`}
                      >
                        <strong>
                          {unknown
                            ? t("metric.noData")
                            : metric.kind === "quota"
                              ? `${collapsed ? `${widgetMetricLabel(metric.label)} ` : ""}${state === "stale" ? t("metric.previousPrefix") : ""}${percentText(metric.remainingPercent!)}`
                              : balanceText(metric, "widget")}
                        </strong>
                        {!collapsed && display.showProgress && !unknown && metric.kind === "quota" && (
                          <span className="widget-bar">
                            <i style={{ width: `${metric.remainingPercent}%` }} />
                          </span>
                        )}
                        {!collapsed && (
                          <small>
                            {unknown ? (
                              t("widget.notRecorded", {
                                label:
                                  metric.kind === "balance" ? balanceLabel(metric) : shortLabel(metric.label),
                              })
                            ) : state === "stale" ? (
                              t("widget.staleMinutes", {
                                minutes:
                                  Math.max(
                                    0,
                                    Math.floor((Date.now() - Date.parse(metric.observedAt)) / 60000),
                                  ) || 0,
                              })
                            ) : metric.kind === "quota" ? (
                              <>
                                <span>{widgetMetricLabel(metric.label)}</span>
                                {display.showReset && (
                                  <span className="widget-metric-reset">{resetLabel(metric.resetsAt)}</span>
                                )}
                              </>
                            ) : metric.kind === "balance" ? (
                              t("metric.wallet")
                            ) : (
                              metric.label
                            )}
                          </small>
                        )}
                      </span>
                    );
                  })
                ) : (
                  <span className="widget-metric state-unknown">
                    <strong>{t("metric.noData")}</strong>
                  </span>
                )}
              </span>
            </button>
          ))}
        </div>
      )}
      {visibleAccounts.length > display.maxAccounts && (
        <div className="widget-footer">
          <button className="widget-more" onClick={onOpen}>
            {t("widget.moreAccounts", { count: visibleAccounts.length - display.maxAccounts })}
          </button>
        </div>
      )}
    </div>
  );
}
