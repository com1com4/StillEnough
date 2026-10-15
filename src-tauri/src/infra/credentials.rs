use zeroize::Zeroizing;
const SERVICE: &str = "com.ai-plan-manager.api-keys";
fn entry(reference: &str) -> Result<keyring::Entry, String> {
    if !cfg!(target_os = "windows") {
        return Err("系统凭据库目前仅支持 Windows".into());
    }
    keyring::Entry::new(SERVICE, reference).map_err(|_| "系统凭据库不可用".into())
}
pub fn read(reference: &str) -> Result<Zeroizing<String>, String> {
    entry(reference)?
        .get_password()
        .map(Zeroizing::new)
        .map_err(|_| "auth_required".into())
}
pub fn write(reference: &str, key: &str) -> Result<(), String> {
    entry(reference)?
        .set_password(key)
        .map_err(|_| "系统凭据库写入失败".into())
}
pub fn remove(reference: &str) -> Result<(), String> {
    match entry(reference)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("系统凭据待清理".into()),
    }
}
