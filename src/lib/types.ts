export interface NetworkConfig {
  mode: "inherit" | "system" | "direct" | "custom";
  proxyUrl: string;
}
export type MetricKind = "quota" | "balance";
export interface QuotaMetric {
  metricId: string;
  label: string;
  kind: MetricKind;
  unit: string;
  remaining: string | null;
  limit: string | null;
  remainingPercent: number | null;
  resetsAt: string | null;
  source: "official_api" | "client_endpoint";
  observedAt: string;
  staleAfterSeconds?: number;
}
export interface Account {
  id: string;
  providerId: string;
  alias: string;
  productName: string;
  enabled: boolean;
  metrics: QuotaMetric[];
  fetchStatus: string;
  lastSuccessAt: string | null;
  lastAttemptAt?: string | null;
  nextRetryAt?: string | null;
  refreshing?: boolean;
  baseUrl?: string | null;
  cliAuthorized?: boolean;
  network?: NetworkConfig;
}
export interface AccountInput {
  id: string | null;
  providerId: string;
  alias: string;
  baseUrl: string;
  enabled: boolean;
  cliAuthorized?: boolean;
  network?: NetworkConfig;
}
export interface ProviderDescriptor {
  id: string;
  name: string;
  supported: boolean;
  authMethod: string;
  baseUrls: string[];
  capability: string;
  status: string;
  limitation: string;
  setupInstructions: string;
  authorizationLabel: string;
  networkNote: string;
}
export interface ProbeResult {
  metrics: QuotaMetric[];
  identityStatus: string;
  observedAt: string;
}
export interface WidgetDisplay {
  showAlias: boolean;
  showProgress: boolean;
  showReset: boolean;
  maxAccounts: number;
}
export interface AppSettings {
  language: import("./i18n").LanguagePreference;
  resolvedLanguage: import("./i18n").Language;
  refreshMinutes: number;
  widgetVisible: boolean;
  alertEnabled: boolean;
  alertThresholds: number[];
  historyRetentionDays: number;
  collectionPaused: boolean;
  autostart: boolean;
  widgetOpacity: number;
  widgetCollapsed: boolean;
  widgetPinned: boolean;
  widgetAccounts: string[] | null;
  widgetDisplay: WidgetDisplay;
  theme: "system" | "light" | "dark";
}
export interface WidgetOptions {
  display?: WidgetDisplay;
  collapsed: boolean;
  pinned: boolean;
  opacity: number;
  accounts: string[] | null;
}
export interface MonitoringSettings {
  alertEnabled: boolean;
  alertThresholds: number[];
  historyRetentionDays: number;
}
export interface AlertEvent {
  id: number;
  accountId: string;
  alias: string;
  productName: string;
  metricId: string;
  metricLabel: string;
  kind: MetricKind;
  unit: string;
  threshold: number;
  crossedThresholds: number[];
  remainingPercent: number;
  periodKey: string;
  triggeredAt: string;
  readAt: string | null;
}
export interface HistoryPoint {
  observedAt: string;
  value: number | null;
  text: string | null;
}
export interface HistorySegment {
  resetsAt: string | null;
  points: HistoryPoint[];
}
export interface HistorySeries {
  accountId: string;
  alias: string;
  productName: string;
  metricId: string;
  metricLabel: string;
  kind: MetricKind;
  unit: string;
  segments: HistorySegment[];
  observedCount: number;
}
export interface ExportSummary {
  format: string;
  fileName: string;
  path: string;
  accountCount: number;
  rowCount: number;
}

export interface AgentAccess { version: number; enabled: boolean; allowRefresh: boolean; allowOfflineCache: boolean; updatedAt: string | null; }
