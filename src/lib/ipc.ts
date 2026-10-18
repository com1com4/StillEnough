import { invoke } from "@tauri-apps/api/core";
import type {
  NetworkConfig,
  Account,
  AccountInput,
  AlertEvent,
  AppSettings,
  ExportSummary,
  HistorySeries,
  MonitoringSettings,
  ProviderDescriptor,
  ProbeResult,
  WidgetOptions,
} from "./types";

function desktopCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

export const api = {
  agentAccess: () => desktopCommand<import("./types").AgentAccess>("get_agent_access"),
  setAgentAccess: (value: import("./types").AgentAccess) => desktopCommand<import("./types").AgentAccess>("set_agent_access", { value }),
  networkSettings: () => desktopCommand<NetworkConfig>("get_network_settings"),
  saveNetworkSettings: (config: NetworkConfig) => desktopCommand<void>("save_network_settings", { config }),
  accounts: () => desktopCommand<Account[]>("list_accounts"),
  providers: () => desktopCommand<ProviderDescriptor[]>("list_provider_descriptors"),
  beginSetup: (input: AccountInput) => desktopCommand<string>("begin_connection_setup", { input }),
  setCredential: (sessionId: string, key: string) =>
    desktopCommand<void>("set_setup_credential", { sessionId, key }),
  probe: (sessionId: string) => desktopCommand<ProbeResult>("probe_connection", { sessionId }),
  cancelSetup: (sessionId: string) => desktopCommand<void>("cancel_connection_setup", { sessionId }),
  saveConnection: (sessionId: string, alias: string, enabled: boolean) =>
    desktopCommand<Account>("save_account_with_connection", { sessionId, alias, enabled }),
  removeAccount: (id: string) => desktopCommand<boolean>("remove_account", { id }),
  settings: () => desktopCommand<AppSettings>("get_settings"),
  updateRefreshInterval: (minutes: number) =>
    desktopCommand<AppSettings>("update_refresh_interval", { minutes }),
  setCollectionPaused: (paused: boolean) => desktopCommand<AppSettings>("set_collection_paused", { paused }),
  stateRevision: () => desktopCommand<number>("state_revision"),
  setAutostart: (enabled: boolean) => desktopCommand<AppSettings>("set_autostart", { enabled }),
  setTheme: (theme: string) => desktopCommand<AppSettings>("set_theme", { theme }),
  setLanguage: (language: import("./i18n").LanguagePreference) =>
    desktopCommand<AppSettings>("set_language", { language }),
  syncNativeTheme: (dark: boolean) => desktopCommand<void>("sync_native_theme", { dark }),
  setWidgetOptions: (options: WidgetOptions) =>
    desktopCommand<AppSettings>("set_widget_options", { options }),
  setWidgetVisible: (visible: boolean) => desktopCommand<boolean>("set_widget_visible", { visible }),
  openMainWindow: () => desktopCommand<void>("open_main_window"),
  quitApp: () => desktopCommand<void>("quit_app"),
  refreshAccount: (id: string) => desktopCommand<void>("refresh_account", { id }),
  alertEvents: (accountId?: string) =>
    desktopCommand<AlertEvent[]>("list_alert_events", { accountId: accountId ?? null }),
  unreadAlertCount: () => desktopCommand<number>("unread_alert_count"),
  markAlertRead: (id: number) => desktopCommand<number>("mark_alert_read", { id }),
  markAllAlertsRead: () => desktopCommand<number>("mark_all_alerts_read"),
  history: (days: 7 | 30, accountId?: string) =>
    desktopCommand<HistorySeries[]>("get_history", { days, accountId: accountId ?? null }),
  updateMonitoringSettings: (settings: MonitoringSettings) =>
    desktopCommand<MonitoringSettings>("update_monitoring_settings", { settings }),
  exportData: (format: "json" | "csv") => desktopCommand<ExportSummary>("export_data", { format }),
  clearAllData: (confirm: string) => desktopCommand<number>("clear_all_data", { confirm }),
};
