use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Command {
    Version,
    Providers,
    Accounts,
    Quota,
    Refresh,
    Operation,
    Doctor,
}
impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Providers => "providers",
            Self::Accounts => "accounts list",
            Self::Quota => "quota",
            Self::Refresh => "refresh",
            Self::Operation => "operations get",
            Self::Doctor => "doctor",
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Providers => "providers",
            Self::Accounts => "accounts",
            Self::Quota => "snapshot",
            Self::Refresh => "refresh",
            Self::Operation => "operation",
            Self::Doctor => "diagnostics",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub protocol_major: u8,
    pub protocol_minor: u8,
    pub schema_version: u8,
    pub app_version: String,
    pub request_id: String,
    pub deadline: String,
    pub command: Command,
    pub account: Option<String>,
    pub provider: Option<String>,
    pub all: bool,
    pub offline: bool,
    pub require_fresh: bool,
    pub live: bool,
    pub operation_id: Option<String>,
}
impl Request {
    pub fn new(command: Command, seconds: u16) -> Self {
        Self {
            protocol_major: 1,
            protocol_minor: 0,
            schema_version: 1,
            app_version: crate::CORE_VERSION.into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            deadline: (chrono::Utc::now() + chrono::Duration::seconds(seconds.into())).to_rfc3339(),
            command,
            account: None,
            provider: None,
            all: false,
            offline: false,
            require_fresh: false,
            live: false,
            operation_id: None,
        }
    }
    pub fn refreshes(&self) -> bool {
        self.command == Command::Refresh || (self.command == Command::Doctor && self.live)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.protocol_major != 1 || self.schema_version != 1 {
            return Err("version_mismatch");
        }
        if uuid::Uuid::parse_str(&self.request_id).is_err() {
            return Err("invalid_arguments");
        }
        let deadline = chrono::DateTime::parse_from_rfc3339(&self.deadline)
            .map_err(|_| "invalid_arguments")?;
        let left = deadline.signed_duration_since(chrono::Utc::now());
        if left.num_milliseconds() <= 0 {
            return Err("deadline_exceeded");
        }
        if left.num_seconds() > 300 {
            return Err("invalid_arguments");
        }
        for id in [&self.account, &self.operation_id].into_iter().flatten() {
            if id.is_empty()
                || id.len() > 128
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err("invalid_arguments");
            }
        }
        if self
            .provider
            .as_ref()
            .is_some_and(|id| crate::providers::descriptor(id).is_none())
        {
            return Err("invalid_arguments");
        }
        if self.account.is_some() && self.provider.is_some() {
            return Err("invalid_arguments");
        }
        use Command::*;
        let valid = match self.command {
            Version => {
                self.account.is_none()
                    && self.provider.is_none()
                    && !self.offline
                    && !self.all
                    && !self.require_fresh
                    && !self.live
                    && self.operation_id.is_none()
            }
            Providers => {
                self.account.is_none()
                    && !self.offline
                    && !self.all
                    && !self.require_fresh
                    && !self.live
                    && self.operation_id.is_none()
            }
            Accounts => {
                self.account.is_none()
                    && !self.all
                    && !self.require_fresh
                    && !self.live
                    && self.operation_id.is_none()
            }
            Quota => !self.all && !self.live && self.operation_id.is_none(),
            Refresh => {
                (self.account.is_some() != self.all)
                    && (self.provider.is_none() || self.all)
                    && !self.offline
                    && !self.live
                    && !self.require_fresh
                    && self.operation_id.is_none()
            }
            Operation => {
                self.operation_id.is_some()
                    && self.account.is_none()
                    && self.provider.is_none()
                    && !self.all
                    && !self.live
                    && !self.offline
                    && !self.require_fresh
            }
            Doctor => {
                self.provider.is_none()
                    && !self.all
                    && !self.require_fresh
                    && self.operation_id.is_none()
                    && (!self.live || (self.account.is_some() && !self.offline))
            }
        };
        if valid {
            Ok(())
        } else {
            Err("invalid_arguments")
        }
    }
}
pub struct Options {
    pub request: Request,
    pub json: bool,
}
pub fn parse(args: &[String]) -> Result<Options, &'static str> {
    let mut position = Vec::new();
    let mut provider = None;
    let mut account = None;
    let mut seconds = 30;
    let mut flags = std::collections::HashSet::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a.starts_with('-') {
            if !flags.insert(a) {
                return Err("invalid_arguments");
            }
            match a {
                "--json" | "--no-color" | "--offline" | "--all" | "--require-fresh" | "--live" => {}
                "--timeout" | "--provider" | "--account" => {
                    i += 1;
                    let value = args.get(i).ok_or("invalid_arguments")?;
                    match a {
                        "--timeout" => {
                            seconds = value.parse().map_err(|_| "invalid_arguments")?;
                            if !(1..=300).contains(&seconds) {
                                return Err("invalid_arguments");
                            }
                        }
                        "--provider" => provider = Some(value.clone()),
                        _ => account = Some(value.clone()),
                    }
                }
                _ => return Err("invalid_arguments"),
            }
        } else {
            position.push(a);
        }
        i += 1;
    }
    let (command, operation_id) = match position.as_slice() {
        ["version"] => (Command::Version, None),
        ["providers"] => (Command::Providers, None),
        ["accounts", "list"] => (Command::Accounts, None),
        ["quota"] => (Command::Quota, None),
        ["refresh"] => (Command::Refresh, None),
        ["doctor"] => (Command::Doctor, None),
        ["operations", "get", id] => (Command::Operation, Some((*id).into())),
        _ => return Err("invalid_arguments"),
    };
    let mut r = Request::new(command, seconds);
    r.provider = provider;
    r.account = account;
    r.offline = flags.contains("--offline");
    r.all = flags.contains("--all");
    r.live = flags.contains("--live");
    r.require_fresh = flags.contains("--require-fresh");
    r.operation_id = operation_id;
    r.validate()?;
    Ok(Options {
        request: r,
        json: flags.contains("--json"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_reject_unsafe_combinations_and_unknown_fields() {
        for args in [
            vec!["refresh"],
            vec!["refresh", "--all", "--account", "a"],
            vec!["refresh", "--all", "--offline"],
            vec!["doctor", "--live"],
            vec!["doctor", "--live", "--account", "a", "--offline"],
            vec!["quota", "--account", "a", "--provider", "codex"],
            vec!["quota", "--database", "SECRET"],
            vec!["providers", "--provider", "invalid"],
            vec!["version", "--timeout", "301"],
        ] {
            assert!(parse(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
        let r = Request::new(Command::Quota, 30);
        let mut v = serde_json::to_value(r).unwrap();
        v["token"] = "SECRET".into();
        assert!(serde_json::from_value::<Request>(v).is_err());
    }
    #[test]
    fn query_commands_and_refresh_targets_parse() {
        for args in [
            vec!["accounts", "list", "--offline", "--json"],
            vec!["quota", "--provider", "codex", "--require-fresh"],
            vec!["refresh", "--all", "--provider", "codex"],
            vec!["doctor", "--account", "a", "--live"],
            vec!["operations", "get", "id"],
        ] {
            assert!(parse(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_ok());
        }
    }
}
