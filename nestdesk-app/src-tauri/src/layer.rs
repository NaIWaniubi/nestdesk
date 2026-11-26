// layer.rs — Win32 桌面层与系统信息原语（零第三方依赖，S0 spike 实测结论的正式化）
use std::ffi::c_void;
use std::sync::Mutex;

pub type HWND = *mut c_void;

const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;
const SW_SHOWNOACTIVATE: i32 = 4; // 恢复最小化但不激活（SW_RESTORE=9 会抢焦点）

#[repr(C)]
#[derive(Default, Clone, Copy)]
#[allow(dead_code)] // FFI 结构体：字段必须完整布局，部分字段不参与计算
pub struct MemoryStatusEx {
    pub dw_length: u32,
    pub dw_memory_load: u32,
    pub ull_total_phys: u64,
    pub ull_avail_phys: u64,
    pub ull_total_pagefile: u64,
    pub ull_avail_pagefile: u64,
    pub ull_total_virtual: u64,
    pub ull_avail_virtual: u64,
    pub ull_avail_extended_virtual: u64,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct SystemPowerStatus {
    pub ac_line_status: u8,
    pub battery_flag: u8,
    pub battery_life_percent: u8,
    pub system_status_flag: u8,
    pub battery_life_time: u32,
    pub battery_full_life_time: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct FileTimeT {
    pub lo: u32,
    pub hi: u32,
}
impl FileTimeT {
    fn u64(&self) -> u64 { ((self.hi as u64) << 32) | self.lo as u64 }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn SetWindowPos(hwnd: HWND, after: HWND, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn GetWindow(hwnd: HWND, cmd: u32) -> HWND;
    fn IsWindow(hwnd: HWND) -> i32;
    fn IsIconic(hwnd: HWND) -> i32;
    fn ShowWindow(hwnd: HWND, cmd: i32) -> i32;
    fn GetWindowLongPtrW(hwnd: HWND, idx: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: HWND, idx: i32, value: isize) -> isize;
    fn SetLayeredWindowAttributes(hwnd: HWND, color_key: u32, alpha: u8, flags: u32) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemTimes(idle: *mut FileTimeT, kernel: *mut FileTimeT, user: *mut FileTimeT) -> i32;
    fn GlobalMemoryStatusEx(buf: *mut MemoryStatusEx) -> i32;
    fn GetSystemPowerStatus(sps: *mut SystemPowerStatus) -> i32;
    fn GetDiskFreeSpaceExW(dir: *const u16, free_to_caller: *mut u64, total: *mut u64, total_free: *mut u64) -> i32;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: HWND, attr: u32, pv: *const u32, cb: u32) -> i32;
}

/// 按控制台设置实时刷新窗口不透明度（30..=255）。只改 LWA_ALPHA，
/// 不碰扩展样式——扩展样式的反复改写会把窗口踢出桌面合成（实测教训）。
pub unsafe fn set_alpha(h: HWND, alpha: u8) {
    unsafe {
        if h.is_null() || IsWindow(h) == 0 {
            return;
        }
        let a = alpha.clamp(30, 255);
        SetLayeredWindowAttributes(h, 0, a, 0x2 /* LWA_ALPHA */);
    }
}

/// Win11 DWM 圆角。注意：ACRYLIC backdrop 与 WS_EX_LAYERED+LWA_ALPHA 互斥，
/// 曾导致窗口整体不进桌面合成（A/B 实测），故只设圆角、不设 backdrop。
pub unsafe fn style_surface(h: HWND) {
    unsafe {
        if h.is_null() {
            return;
        }
        let corner: u32 = 2; // DWMWCP_ROUND
        DwmSetWindowAttribute(h, 33, &corner, 4);
    }
}

/// 窗口扩展样式定型（仅应每窗执行一次；反复改写会破坏桌面合成，实测教训）。
/// tauri transparent 窗口带 WS_EX_NOREDIRECTIONBITMAP(0x40000)，该样式窗口沉底后
/// 不参与桌面合成（A/B 实测）。去掉它并补 WS_EX_LAYERED+LWA_ALPHA：
/// 走分层合成路径，既保留 CSS 半透明，也能沉到桌面层。
pub unsafe fn ensure_layered(h: HWND) {
    unsafe {
        if h.is_null() || IsWindow(h) == 0 {
            return;
        }
        const EX_LAYERED: isize = 0x0008_0000;
        const EX_NO_REDIR: isize = 0x0004_0000;
        let ex = GetWindowLongPtrW(h, -20);
        if (ex & EX_NO_REDIR) != 0 || (ex & EX_LAYERED) == 0 {
            SetWindowLongPtrW(h, -20, (ex & !EX_NO_REDIR) | EX_LAYERED);
            SetLayeredWindowAttributes(h, 0, 254, 0x2 /* LWA_ALPHA */);
        }
    }
}

/// 安全包装：判断 HWND 是否仍存活（用于清理 STYLED 死条目）。
pub fn is_window(h: HWND) -> bool {
    unsafe { !h.is_null() && IsWindow(h) != 0 }
}

/// 只在真的需要时重钉：被 Win+D 最小化，或已经不在最底。Z 序稳定时零系统调用代价。
pub unsafe fn repin_if_needed(h: HWND) {
    unsafe {
        if h.is_null() || IsWindow(h) == 0 {
            return;
        }
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_SHOWNOACTIVATE);
            SetWindowPos(h, 1 as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            return;
        }
        if GetWindow(h, 1 /* GW_HWNDLAST */) != h {
            SetWindowPos(h, 1 as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

static CPU_PREV: Mutex<Option<(u64, u64, u64)>> = Mutex::new(None);

/// CPU 占用率 0.0–1.0（GetSystemTimes 双采样差值）
pub fn cpu_usage() -> f64 {
    unsafe {
        let mut idle = FileTimeT::default();
        let mut kern = FileTimeT::default();
        let mut user = FileTimeT::default();
        if GetSystemTimes(&mut idle, &mut kern, &mut user) == 0 {
            return -1.0;
        }
        let (i, k, u) = (idle.u64(), kern.u64(), user.u64());
        let mut g = CPU_PREV.lock().unwrap_or_else(|e| e.into_inner());
        let r = match *g {
            Some((pi, pk, pu)) => {
                let di = i.saturating_sub(pi);
                let dt = (k.saturating_add(u)).saturating_sub(pk + pu);
                if dt == 0 { 0.0 } else { 1.0 - (di as f64 / dt as f64) }
            }
            None => 0.0,
        };
        *g = Some((i, k, u));
        r.clamp(0.0, 1.0)
    }
}

/// 路径所在卷的容量：(总容量, 可用容量) 字节。
/// 用 GetDiskFreeSpaceExW 而不是 GetDiskFreeSpaceW：前者返回 64 位、支持 >2TB 卷与 UNC。
pub fn disk_free(path: &std::path::Path) -> Option<(u64, u64)> {
    unsafe {
        // 取盘符根（"E:\\"）：API 要求目录路径，子目录同样接受，用根更稳
        let root = path.components().next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .filter(|s| s.len() >= 2)
            .unwrap_or_else(|| "C:\\".to_string());
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let mut free_to_caller: u64 = 0;
        let mut total: u64 = 0;
        let mut total_free: u64 = 0;
        if GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_to_caller, &mut total, &mut total_free) == 0 {
            return None;
        }
        Some((total, free_to_caller))
    }
}

/// (总内存, 可用内存) 字节
pub fn memory() -> Option<(u64, u64)> {
    unsafe {
        let mut m = MemoryStatusEx { dw_length: std::mem::size_of::<MemoryStatusEx>() as u32, ..Default::default() };
        if GlobalMemoryStatusEx(&mut m) == 0 { return None; }
        Some((m.ull_total_phys, m.ull_avail_phys))
    }
}

/// (百分比, 是否插电, 是否充电中)；台式机无电池返回 None
pub fn battery() -> Option<(u8, bool, bool)> {
    unsafe {
        let mut s = SystemPowerStatus::default();
        if GetSystemPowerStatus(&mut s) == 0 { return None; }
        if s.battery_life_percent == 255 { return None; }
        Some((
            s.battery_life_percent,
            s.ac_line_status == 1,
            (s.battery_flag & 0x08) == 0x08, // bit3 = charging
        ))
    }
}
