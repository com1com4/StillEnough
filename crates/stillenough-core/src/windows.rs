//! Windows 本机当前用户/会话边界；不读取认证文件或厂商秘密。
use std::{
    ffi::c_void,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_ALREADY_EXISTS, HANDLE},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        GetTokenInformation, TokenElevation, TokenSessionId, TokenUser, SECURITY_ATTRIBUTES,
        TOKEN_QUERY, TOKEN_USER,
    },
    System::{
        Com::CoTaskMemFree,
        Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
        Threading::{
            CreateMutexW, GetCurrentProcess, OpenProcess, OpenProcessToken,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
    UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
};
pub struct Handle(HANDLE);
// 句柄无线程归属（不以线程拥有 mutex），仅在唯一持有者析构时关闭。
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub sid: String,
    pub session: u32,
    pub elevated: bool,
}
fn token_info(token: HANDLE, class: i32) -> Result<Vec<usize>, &'static str> {
    let mut length = 0;
    unsafe {
        GetTokenInformation(token, class, ptr::null_mut(), 0, &mut length);
    };
    if length == 0 {
        return Err("agent_access_denied");
    }
    let mut data = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe { GetTokenInformation(token, class, data.as_mut_ptr().cast(), length, &mut length) }
        == 0
    {
        return Err("agent_access_denied");
    }
    Ok(data)
}
fn identity(process: HANDLE) -> Result<Identity, &'static str> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err("agent_access_denied");
    }
    let token = Handle(token);
    let data = token_info(token.0, TokenUser)?;
    let user = unsafe { &*data.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) } == 0 {
        return Err("agent_access_denied");
    }
    let value = unsafe { wide_string(sid) };
    unsafe {
        LocalFree(sid.cast());
    }
    let session = token_info(token.0, TokenSessionId)?;
    let elevation = token_info(token.0, TokenElevation)?;
    Ok(Identity {
        sid: value,
        session: session[0] as u32,
        elevated: elevation[0] as u32 != 0,
    })
}
unsafe fn wide_string(raw: *const u16) -> String {
    let mut length = 0;
    while unsafe { *raw.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(raw, length) })
}
pub fn current_identity() -> Result<Identity, &'static str> {
    identity(unsafe { GetCurrentProcess() })
}
pub fn data_dir() -> Result<PathBuf, &'static str> {
    let mut raw = ptr::null_mut();
    if unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, ptr::null_mut(), &mut raw) } < 0 {
        return Err("not_initialized");
    }
    let mut length = 0;
    while unsafe { *raw.add(length) } != 0 {
        length += 1;
    }
    let path = PathBuf::from(std::ffi::OsString::from_wide(unsafe {
        std::slice::from_raw_parts(raw, length)
    }));
    unsafe {
        CoTaskMemFree(raw.cast());
    }
    Ok(path.join("com.ai-plan-manager.app"))
}
pub fn database_path() -> Result<PathBuf, &'static str> {
    Ok(data_dir()?.join("ai-plan-manager.sqlite"))
}
fn normalized(path: &Path) -> Result<String, &'static str> {
    let full = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let raw = full.to_string_lossy().replace('/', "\\").to_lowercase();
    if raw.starts_with("\\\\?\\unc\\") || (raw.starts_with("\\\\") && !raw.starts_with("\\\\?\\")) {
        return Err("cache_unavailable");
    }
    Ok(raw
        .strip_prefix("\\\\?\\")
        .unwrap_or(&raw)
        .trim_end_matches('\\')
        .to_string())
}
fn key(path: &Path) -> Result<String, &'static str> {
    use sha2::{Digest, Sha256};
    let who = current_identity()?;
    Ok(format!(
        "{:x}",
        Sha256::digest(format!("{}:{}:{}", who.sid, who.session, normalized(path)?).as_bytes())
    ))
}
pub fn endpoint(path: &Path) -> Result<String, &'static str> {
    Ok(format!(r"\\.\pipe\StillEnough-{}", key(path)?))
}
pub struct Security {
    descriptor: *mut c_void,
}
impl Security {
    pub fn new() -> Result<Self, &'static str> {
        let sid = current_identity()?.sid;
        let sddl: Vec<_> = format!("D:P(D;;GA;;;NU)(A;;GA;;;SY)(A;;GA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err("agent_access_denied");
        }
        Ok(Self { descriptor })
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor,
            bInheritHandle: 0,
        }
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.descriptor);
        }
    }
}
pub fn owner_lock(path: &Path) -> Result<Handle, &'static str> {
    let security = Security::new()?;
    let attrs = security.attributes();
    use sha2::{Digest, Sha256};
    let owner = format!(
        "{:x}",
        Sha256::digest(format!("{}:{}", current_identity()?.sid, normalized(path)?).as_bytes())
    );
    let name: Vec<_> = format!("Global\\StillEnoughOwner-{owner}")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let handle = unsafe { CreateMutexW(&attrs, 0, name.as_ptr()) };
    if handle.is_null() {
        return Err("owner_conflict");
    }
    let guard = Handle(handle);
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return Err("owner_conflict");
    }
    Ok(guard)
}
pub fn verify_peer(
    pipe: &impl std::os::windows::io::AsRawHandle,
    server: bool,
) -> Result<(), &'static str> {
    let pipe = pipe.as_raw_handle();
    let mut pid = 0;
    let ok = unsafe {
        if server {
            GetNamedPipeServerProcessId(pipe, &mut pid)
        } else {
            GetNamedPipeClientProcessId(pipe, &mut pid)
        }
    };
    if ok == 0 {
        return Err("agent_access_denied");
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Err("agent_access_denied");
    }
    let handle = Handle(handle);
    if identity(handle.0)? != current_identity()? {
        return Err("agent_access_denied");
    }
    Ok(())
}
