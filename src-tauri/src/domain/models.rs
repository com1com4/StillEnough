use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub provider_id: String,
    pub alias: String,
    pub product_name: String,
    pub enabled: bool,
    pub metrics: Vec<QuotaMetric>,
    pub fetch_status: String,
    pub last_success_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub refreshing: bool,
    pub base_url: Option<String>,
    pub cli_authorized: bool,
    #[serde(default)]
    pub network: crate::services::network::NetworkConfig,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaMetric {
    pub metric_id: String,
    pub label: String,
    pub kind: String,
    pub unit: String,
    pub remaining: Option<String>,
    pub limit: Option<String>,
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<String>,
    pub source: String,
    pub observed_at: String,
    #[serde(default)]
    pub stale_after_seconds: u64,
}
// IPC accepts connection configuration only. Reject attempts to overwrite observations.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountInput {
    pub id: Option<String>,
    pub provider_id: String,
    pub alias: String,
    pub base_url: String,
    pub enabled: bool,
    #[serde(default)]
    pub cli_authorized: bool,
    #[serde(default)]
    pub network: crate::services::network::NetworkConfig,
}
pub use stillenough_core::providers::ProviderDescriptor;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub metrics: Vec<QuotaMetric>,
    pub identity_status: &'static str,
    pub observed_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub refresh_minutes: u8,
    pub widget_visible: bool,
    #[serde(default)]
    pub widget_position: Option<(i32, i32)>,
    #[serde(default = "default_alert_enabled")]
    pub alert_enabled: bool,
    #[serde(default = "default_alert_thresholds")]
    pub alert_thresholds: Vec<f64>,
    #[serde(default = "default_retention_days")]
    pub history_retention_days: u32,
    /// Stops every scheduled query without touching each account's own enabled flag.
    #[serde(default)]
    pub collection_paused: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_widget_opacity")]
    pub widget_opacity: u8,
    #[serde(default)]
    pub widget_collapsed: bool,
    #[serde(default = "default_true")]
    pub widget_pinned: bool,
    /// `None` keeps the automatic "first few accounts" behaviour.
    #[serde(default)]
    pub widget_accounts: Option<Vec<String>>,
    #[serde(default)]
    pub widget_display: WidgetDisplay,
    #[serde(default, deserialize_with = "theme_from_stored")]
    pub theme: ThemeMode,
    #[serde(default, deserialize_with = "language_from_stored")]
    pub language: LanguagePreference,
    /// Runtime value, always recomputed rather than trusted from stored JSON.
    #[serde(skip_deserializing)]
    pub resolved_language: String,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LanguagePreference {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "en")]
    English,
}

impl LanguagePreference {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "system" => Ok(Self::System),
            "zh-CN" => Ok(Self::Chinese),
            "en" => Ok(Self::English),
            _ => Err("invalid_language".into()),
        }
    }

    pub fn resolve(self) -> &'static str {
        match self {
            Self::Chinese => "zh-CN",
            Self::English => "en",
            Self::System => system_language(),
        }
    }
}

fn language_from_stored<'de, D>(deserializer: D) -> Result<LanguagePreference, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .and_then(|s| LanguagePreference::parse(s).ok())
        .unwrap_or_default())
}

#[cfg(windows)]
fn system_language() -> &'static str {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // LANGID's low ten bits are the primary language; 0x04 is Chinese.
    if unsafe { GetUserDefaultUILanguage() } & 0x03ff == 0x04 {
        "zh-CN"
    } else {
        "en"
    }
}

#[cfg(not(windows))]
fn system_language() -> &'static str {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default();
    if locale.to_ascii_lowercase().starts_with("zh") {
        "zh-CN"
    } else {
        "en"
    }
}

/// A theme this build does not know about falls back to following the OS.
///
/// `load_settings` discards the whole blob when deserialization fails, so an
/// unreadable value here would cost the user every other preference.
fn theme_from_stored<'de, D>(deserializer: D) -> Result<ThemeMode, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(match raw.as_deref() {
        Some("light") => ThemeMode::Light,
        Some("dark") => ThemeMode::Dark,
        _ => ThemeMode::System,
    })
}

/// Whether to follow the OS appearance or pin one side of it.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "system" | "跟随系统" => Ok(Self::System),
            "light" | "白天" => Ok(Self::Light),
            "dark" | "夜间" => Ok(Self::Dark),
            _ => Err("外观仅支持跟随系统、白天或夜间".into()),
        }
    }
}
fn default_alert_enabled() -> bool {
    true
}
fn default_true() -> bool {
    true
}
/// Matches the translucent floor the widget stylesheet can still be read against.
pub const MIN_WIDGET_OPACITY: u8 = 50;
pub const MAX_WIDGET_OPACITY: u8 = 100;
fn default_widget_opacity() -> u8 {
    100
}
fn default_alert_thresholds() -> Vec<f64> {
    crate::services::alerts::DEFAULT_THRESHOLDS.to_vec()
}
fn default_retention_days() -> u32 {
    crate::services::history::DEFAULT_RETENTION_DAYS
}
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            refresh_minutes: 10,
            widget_visible: false,
            widget_position: None,
            alert_enabled: default_alert_enabled(),
            alert_thresholds: default_alert_thresholds(),
            history_retention_days: default_retention_days(),
            collection_paused: false,
            autostart: false,
            widget_opacity: default_widget_opacity(),
            widget_collapsed: false,
            widget_pinned: default_true(),
            widget_accounts: None,
            widget_display: WidgetDisplay::default(),
            theme: ThemeMode::default(),
            language: LanguagePreference::default(),
            resolved_language: LanguagePreference::default().resolve().into(),
        }
    }
}
impl AppSettings {
    /// Stored JSON from an older build must not silently widen alerting or retention.
    pub fn sanitize(mut self) -> Self {
        self.resolved_language = self.language.resolve().into();
        if !matches!(self.refresh_minutes, 5 | 10 | 15 | 30) {
            self.refresh_minutes = 10;
        }
        self.alert_thresholds =
            crate::services::alerts::sanitize_thresholds(&self.alert_thresholds);
        if !matches!(self.history_retention_days, 30 | 60 | 90) {
            self.history_retention_days = default_retention_days();
        }
        self.widget_display.idle_opacity = self.widget_display.idle_opacity.clamp(1, 100);
        self.widget_display.max_accounts = self.widget_display.max_accounts.clamp(1, 8);
        self.widget_opacity = self
            .widget_opacity
            .clamp(MIN_WIDGET_OPACITY, MAX_WIDGET_OPACITY);
        if let Some(accounts) = &mut self.widget_accounts {
            // An empty list would silently blank the widget, so it falls back to automatic order.
            let mut seen = std::collections::BTreeSet::new();
            accounts.retain(|id| !id.trim().is_empty() && seen.insert(id.clone()));
            if accounts.is_empty() {
                self.widget_accounts = None;
            }
        }
        self
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WidgetDisplay {
    pub fade_when_idle: bool,
    pub idle_opacity: u8,
    pub show_alias: bool,
    pub show_progress: bool,
    pub show_reset: bool,
    pub max_accounts: u8,
}
impl Default for WidgetDisplay {
    fn default() -> Self {
        Self {
            fade_when_idle: true,
            idle_opacity: 10,
            show_alias: false,
            show_progress: true,
            show_reset: true,
            max_accounts: 4,
        }
    }
}
/// The widget-facing subset of preferences, so one command can apply them atomically.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WidgetOptions {
    #[serde(default)]
    pub display: Option<WidgetDisplay>,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub pinned: bool,
    pub opacity: u8,
    #[serde(default)]
    pub accounts: Option<Vec<String>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertEvent {
    pub id: i64,
    pub account_id: String,
    pub alias: String,
    pub product_name: String,
    pub metric_id: String,
    pub metric_label: String,
    pub kind: String,
    pub unit: String,
    pub threshold: f64,
    pub crossed_thresholds: Vec<f64>,
    pub remaining_percent: f64,
    pub period_key: String,
    pub triggered_at: String,
    pub read_at: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub observed_at: String,
    pub value: Option<f64>,
    pub text: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySegment {
    pub resets_at: Option<String>,
    pub points: Vec<HistoryPoint>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySeries {
    pub account_id: String,
    pub alias: String,
    pub product_name: String,
    pub metric_id: String,
    pub metric_label: String,
    pub kind: String,
    pub unit: String,
    pub segments: Vec<HistorySegment>,
    pub observed_count: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringSettings {
    pub alert_enabled: bool,
    pub alert_thresholds: Vec<f64>,
    pub history_retention_days: u32,
}
impl From<&AppSettings> for MonitoringSettings {
    fn from(settings: &AppSettings) -> Self {
        Self {
            alert_enabled: settings.alert_enabled,
            alert_thresholds: settings.alert_thresholds.clone(),
            history_retention_days: settings.history_retention_days,
        }
    }
}
impl Default for MonitoringSettings {
    fn default() -> Self {
        MonitoringSettings::from(&AppSettings::default())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub format: String,
    pub file_name: String,
    pub path: String,
    pub account_count: usize,
    pub row_count: usize,
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn old_and_unknown_language_values_preserve_other_preferences() {
        for value in ["null", "42", "{}", "\"fr\"", "\"\""] {
            let json = format!(
                r#"{{"refreshMinutes":15,"widgetVisible":true,"language":{value},"resolvedLanguage":"untrusted"}}"#
            );
            let settings = serde_json::from_str::<AppSettings>(&json)
                .unwrap()
                .sanitize();
            assert_eq!(settings.language, LanguagePreference::System);
            assert_eq!(settings.refresh_minutes, 15);
            assert!(settings.widget_visible);
            assert_eq!(
                settings.resolved_language,
                LanguagePreference::System.resolve()
            );
        }
        let settings =
            serde_json::from_str::<AppSettings>(r#"{"refreshMinutes":30,"widgetVisible":false}"#)
                .unwrap()
                .sanitize();
        assert_eq!(settings.language, LanguagePreference::System);
        assert_eq!(settings.refresh_minutes, 30);
    }

    #[test]
    fn language_preferences_round_trip_and_resolve_consistently() {
        for (value, resolved) in [
            ("zh-CN", "zh-CN"),
            ("en", "en"),
            ("system", LanguagePreference::System.resolve()),
        ] {
            let settings = AppSettings {
                language: LanguagePreference::parse(value).unwrap(),
                ..AppSettings::default()
            }
            .sanitize();
            let stored = serde_json::to_string(&settings).unwrap();
            let reloaded = serde_json::from_str::<AppSettings>(&stored)
                .unwrap()
                .sanitize();
            assert_eq!(reloaded.language, settings.language);
            assert_eq!(reloaded.resolved_language, resolved);
            assert!(stored.contains(&format!(r#""language":"{value}""#)));
        }
        assert!(LanguagePreference::parse("fr").is_err());
        assert!(LanguagePreference::parse("English").is_err());
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    fn parse(json: &str) -> ThemeMode {
        serde_json::from_str::<AppSettings>(json)
            .expect("主题值无法读取时不应让整份设置作废")
            .theme
    }

    #[test]
    fn older_settings_without_a_theme_follow_the_system() {
        assert_eq!(
            parse(r#"{"refreshMinutes":10,"widgetVisible":true}"#),
            ThemeMode::System
        );
    }

    #[test]
    fn a_theme_this_build_does_not_know_still_keeps_every_other_setting() {
        // 这条很关键：load_settings 在反序列化失败时会整份退回默认值。
        let settings: AppSettings = serde_json::from_str(
            r#"{"refreshMinutes":15,"widgetVisible":true,"theme":"solarized"}"#,
        )
        .unwrap();
        assert_eq!(settings.theme, ThemeMode::System);
        assert_eq!(settings.refresh_minutes, 15, "其他设置必须保留");
    }

    #[test]
    fn each_theme_round_trips_through_storage() {
        for (stored, expected) in [
            ("system", ThemeMode::System),
            ("light", ThemeMode::Light),
            ("dark", ThemeMode::Dark),
        ] {
            let json =
                format!(r#"{{"refreshMinutes":10,"widgetVisible":true,"theme":"{stored}"}}"#);
            assert_eq!(parse(&json), expected);
        }
    }

    #[test]
    fn only_the_three_known_modes_are_accepted_from_the_ui() {
        assert!(ThemeMode::parse("dark").is_ok());
        assert!(ThemeMode::parse("夜间").is_ok());
        assert!(ThemeMode::parse("sepia").is_err());
    }
}

#[cfg(test)]
mod widget_display_tests {
    use super::*;
    #[test]
    fn old_settings_keep_widget_display_defaults() {
        let settings: AppSettings =
            serde_json::from_str(r#"{"refreshMinutes":10,"widgetVisible":true}"#).unwrap();
        assert_eq!(settings.widget_display.max_accounts, 4);
        assert!(settings.widget_display.fade_when_idle);
        assert_eq!(settings.widget_display.idle_opacity, 10);
        assert!(!settings.widget_display.show_alias);
        assert!(settings.widget_display.show_progress);
        assert!(settings.widget_display.show_reset);
    }
    #[test]
    fn idle_opacity_defaults_for_existing_display_and_clamps_stored_values() {
        let old: AppSettings =
            serde_json::from_str(r#"{"refreshMinutes":10,"widgetVisible":true,"widgetDisplay":{"fadeWhenIdle":true,"showAlias":true}}"#)
                .unwrap();
        assert_eq!(old.widget_display.idle_opacity, 10);
        for (stored, expected) in [(0, 1), (1, 1), (100, 100), (255, 100)] {
            let mut settings = AppSettings::default();
            settings.widget_display.idle_opacity = stored;
            assert_eq!(settings.sanitize().widget_display.idle_opacity, expected);
        }
    }
    #[test]
    fn display_preferences_survive_reload_and_limit_account_count() {
        let mut settings = AppSettings {
            widget_display: WidgetDisplay {
                fade_when_idle: false,
                idle_opacity: 25,
                show_alias: true,
                show_progress: false,
                show_reset: false,
                max_accounts: 255,
            },
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let restored = serde_json::from_str::<AppSettings>(&json)
            .unwrap()
            .sanitize();
        assert_eq!(restored.widget_display.max_accounts, 8);
        assert!(!restored.widget_display.fade_when_idle);
        assert_eq!(restored.widget_display.idle_opacity, 25);
        assert!(restored.widget_display.show_alias);
        assert!(!restored.widget_display.show_progress);
        assert!(!restored.widget_display.show_reset);
        settings.widget_display.max_accounts = 0;
        assert_eq!(settings.sanitize().widget_display.max_accounts, 1);
    }
}
