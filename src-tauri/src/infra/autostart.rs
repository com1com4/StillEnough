/// Windows "run at logon" registration, kept in the current user's own hive.
///
/// The entry is a plain `HKCU\...\Run` value, so it needs no service, no scheduled
/// task and no elevated rights, and disabling it leaves nothing behind but the
/// removal of that one value.
const VALUE_NAME: &str = "StillEnough";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
mod platform {
    use super::{RUN_KEY, VALUE_NAME};
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_READ},
        RegKey,
    };

    /// Quoted so a path containing spaces still resolves as one executable.
    fn command() -> Result<String, String> {
        let executable =
            std::env::current_exe().map_err(|error| format!("无法确定程序路径：{error}"))?;
        Ok(format!("\"{}\"", executable.display()))
    }

    pub fn is_enabled() -> Result<bool, String> {
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ)
        else {
            return Ok(false);
        };
        let Ok(value) = key.get_value::<String, _>(VALUE_NAME) else {
            return Ok(false);
        };
        // A stale entry pointing at another build is not this app running.
        Ok(value.trim() == command()?.trim())
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(RUN_KEY)
            .map_err(|error| format!("无法写入开机启动项：{error}"))?
            .0;
        if enabled {
            key.set_value(VALUE_NAME, &command()?)
                .map_err(|error| format!("无法写入开机启动项：{error}"))
        } else {
            match key.delete_value(VALUE_NAME) {
                Ok(()) => Ok(()),
                // Disabling an already-disabled entry is the requested end state.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("无法移除开机启动项：{error}")),
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn is_enabled() -> Result<bool, String> {
        Err("开机自启动目前仅支持 Windows".into())
    }
    pub fn set_enabled(_enabled: bool) -> Result<(), String> {
        Err("开机自启动目前仅支持 Windows".into())
    }
}

pub fn is_enabled() -> Result<bool, String> {
    platform::is_enabled()
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    platform::set_enabled(enabled)
}

/// Applies the preference and reports what actually took effect, so a rejected
/// registry write cannot leave the stored setting claiming otherwise.
pub fn apply(preferred: bool) -> Result<bool, String> {
    if is_enabled()? == preferred {
        return Ok(preferred);
    }
    set_enabled(preferred)?;
    is_enabled()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconciling_the_current_state_never_writes_the_registry() {
        if !cfg!(target_os = "windows") {
            return;
        }
        // Read-only: the preference already matches, so apply() short-circuits.
        let current = is_enabled().unwrap_or(false);
        assert_eq!(apply(current).unwrap(), current);
    }

    /// Writes a real logon entry, so it stays out of the default run.
    #[test]
    #[ignore = "写入真实 Windows 开机启动项"]
    fn enable_then_disable_leaves_no_run_value() {
        set_enabled(true).unwrap();
        assert!(is_enabled().unwrap());
        set_enabled(false).unwrap();
        assert!(!is_enabled().unwrap());
    }
}
