//! Experimental legacy Windows acrylic. This API is undocumented and may change.
#[cfg(windows)]
pub fn set(window: &tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    use std::ffi::c_void;
    #[repr(C)]
    struct AccentPolicy {
        state: u32,
        flags: u32,
        color: u32,
        animation: u32,
    }
    #[repr(C)]
    struct AttributeData {
        attribute: u32,
        data: *mut c_void,
        size: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn GetProcAddress(module: isize, name: *const u8) -> *const c_void;
    }
    type Apply = unsafe extern "system" fn(isize, *mut AttributeData) -> i32;
    let name: Vec<u16> = "user32.dll\0".encode_utf16().collect();
    // user32 is already loaded by the windowing runtime. No module ownership is transferred.
    let module = unsafe { GetModuleHandleW(name.as_ptr()) };
    if module == 0 {
        return Err("无法启用浮窗毛玻璃：Windows 接口不可用".into());
    }
    let pointer =
        unsafe { GetProcAddress(module, c"SetWindowCompositionAttribute".as_ptr().cast()) };
    if pointer.is_null() {
        return Err("无法启用浮窗毛玻璃：Windows 接口不可用".into());
    }
    let apply: Apply = unsafe { std::mem::transmute(pointer) };
    let mut policy = AccentPolicy {
        state: if enabled { 4 } else { 0 },
        flags: 0,
        color: if enabled { 0x01000000 } else { 0 },
        animation: 0,
    };
    let mut data = AttributeData {
        attribute: 19,
        data: (&mut policy as *mut AccentPolicy).cast(),
        size: std::mem::size_of::<AccentPolicy>(),
    };
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    if !enabled {
        set_opacity(window, 100)?;
    }
    // DWM's rounded-corner preference is a hint; older Windows can ignore it.
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(hwnd: isize, attr: u32, value: *const c_void, size: u32) -> i32;
    }
    let preference: u32 = if enabled { 2 } else { 0 };
    let _ = unsafe { DwmSetWindowAttribute(hwnd, 33, (&preference as *const u32).cast(), 4) };
    if unsafe { apply(hwnd, &mut data) } == 0 {
        return Err("无法切换浮窗毛玻璃，请使用普通透明模式".into());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set(_window: &tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    if enabled {
        Err("浮窗毛玻璃仅支持 Windows".into())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
static LAYERED_WINDOWS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<isize, bool>>,
> = std::sync::OnceLock::new();

#[cfg(windows)]
pub fn set_opacity(window: &tauri::WebviewWindow, percent: u8) -> Result<(), String> {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        fn SetLayeredWindowAttributes(hwnd: isize, key: u32, alpha: u8, flags: u32) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn SetLastError(error: u32);
        fn GetLastError() -> u32;
    }
    fn write_style(hwnd: isize, style: isize) -> Result<(), String> {
        unsafe {
            SetLastError(0);
        }
        let previous = unsafe { SetWindowLongPtrW(hwnd, -20, style) };
        if previous == 0 && unsafe { GetLastError() } != 0 {
            return Err("无法切换浮窗透明窗口样式".into());
        }
        Ok(())
    }
    const EX_STYLE: i32 = -20;
    const LAYERED: isize = 0x80000;
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    let mut windows = LAYERED_WINDOWS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "浮窗状态不可用".to_string())?;
    if percent >= 100 {
        if let Some(&originally_layered) = windows.get(&hwnd) {
            if originally_layered {
                if unsafe { SetLayeredWindowAttributes(hwnd, 0, 255, 2) } == 0 {
                    return Err("无法恢复浮窗不透明度".into());
                }
            } else {
                let style = unsafe { GetWindowLongPtrW(hwnd, EX_STYLE) };
                write_style(hwnd, style & !LAYERED)?;
            }
            windows.remove(&hwnd);
        }
        return Ok(());
    }
    let style = unsafe { GetWindowLongPtrW(hwnd, EX_STYLE) };
    let had_record = windows.contains_key(&hwnd);
    let originally_layered = *windows.entry(hwnd).or_insert(style & LAYERED != 0);
    if style & LAYERED == 0 {
        if let Err(error) = write_style(hwnd, style | LAYERED) {
            if !had_record {
                windows.remove(&hwnd);
            }
            return Err(error);
        }
    }
    let alpha = ((u16::from(percent.clamp(1, 100)) * 255 + 50) / 100) as u8;
    if unsafe { SetLayeredWindowAttributes(hwnd, 0, alpha, 2) } == 0 {
        if !had_record {
            windows.remove(&hwnd);
            if !originally_layered {
                let _ = write_style(hwnd, style);
            }
        }
        return Err("无法设置浮窗不透明度".into());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_opacity(_window: &tauri::WebviewWindow, _percent: u8) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub fn forget(hwnd: isize) {
    if let Some(windows) = LAYERED_WINDOWS.get() {
        if let Ok(mut windows) = windows.lock() {
            windows.remove(&hwnd);
        }
    }
}
