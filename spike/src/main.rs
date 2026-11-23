// nestdesk-spike — S0: 把透明窗口挂进桌面层 (Progman/WorkerW)
// 零依赖裸 Win32 FFI。验证 C1(Win+D 后重挂) / C4(explorer 重启) / C6(不浮于应用上)。
// 右键退出。
#![allow(clashing_extern_declarations)]

use std::ffi::c_void;
use std::ptr;
use std::time::Duration;

type HWND = *mut c_void;
type HINSTANCE = *mut c_void;
type LPARAM = isize;
type WPARAM = usize;
type LRESULT = isize;

const WM_DESTROY: u32 = 0x0002;
const WM_NCHITTEST: u32 = 0x0084;
const WM_TIMER: u32 = 0x0113;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_CLOSE: u32 = 0x0010;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const HTCAPTION: LRESULT = 2;
const HTCLIENT: LRESULT = 1;
const WS_POPUP: u32 = 0x8000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const SMTO_NORMAL: u32 = 0x0000;
const LWA_ALPHA: u32 = 0x0000_0002;
const SW_SHOW: i32 = 5;
const TIMER_ID: usize = 0x9E57;
// 私有消息：让 Progman 分裂出 WorkerW（公开十年以上的桌面嵌入惯用法）
const SHELL_MESSAGE: u32 = 0x052C;

#[link(name = "user32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    fn RegisterClassExW(cls: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(
        ex_style: u32, class_name: *const u16, window_name: *const u16, style: u32,
        x: i32, y: i32, w: i32, h: i32, parent: HWND, menu: *mut c_void,
        instance: HINSTANCE, param: *mut c_void,
    ) -> HWND;
    fn DefWindowProcW(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn DestroyWindow(hwnd: HWND) -> i32;
    fn PostQuitMessage(code: i32);
    fn SetWindowLongPtrW(hwnd: HWND, idx: i32, value: isize) -> isize;
    fn FindWindowW(class_name: *const u16, window_name: *const u16) -> HWND;
    fn FindWindowExW(parent: HWND, child_after: HWND, class_name: *const u16, window_name: *const u16) -> HWND;
    fn SendMessageTimeoutW(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM, flags: u32, timeout: u32, result: *mut usize) -> usize;
    fn SetParent(child: HWND, new_parent: HWND) -> HWND;
    fn GetParent(hwnd: HWND) -> HWND;
    fn SetLayeredWindowAttributes(hwnd: HWND, color_key: u32, alpha: u8, flags: u32) -> i32;
    fn SetWindowPos(hwnd: HWND, after: HWND, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const MSG) -> i32;
    fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    fn SetTimer(hwnd: HWND, idevent: usize, elapsed: u32, proc: *mut c_void) -> usize;
    fn KillTimer(hwnd: HWND, idevent: usize) -> i32;
    fn PostMessageW(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> i32;
    fn SetProcessDpiAwarenessContext(ctx: *mut c_void) -> i32;
    fn IsWindow(hwnd: HWND) -> i32;
    fn IsIconic(hwnd: HWND) -> i32;
    fn IsWindowVisible(hwnd: HWND) -> i32;
    fn ShowWindow(hwnd: HWND, cmd: i32) -> i32;
    fn GetAncestor(hwnd: HWND, flags: u32) -> HWND;
    fn GetClassNameW(hwnd: HWND, buf: *mut u16, max: i32) -> i32;
}

const GWL_EXSTYLE_IDX: i32 = -20;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;
const HWND_BOTTOM: HWND = 1 as HWND;

#[repr(C)]
#[derive(Clone, Copy)]
struct WNDCLASSEXW {
    cb_size: u32, style: u32, lpfn_wnd_proc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
    cb_cls_extra: i32, cb_wnd_extra: i32, h_instance: HINSTANCE, h_icon: *mut c_void, h_cursor: *mut c_void,
    h_br_background: *mut c_void, lpsz_menu_name: *const u16, lpsz_class_name: *const u16, h_icon_sm: *mut c_void,
}

#[repr(C)]
struct MSG { hwnd: HWND, message: u32, w_param: WPARAM, l_param: LPARAM, time: u32, pt: POINT }
#[repr(C)]
#[derive(Clone, Copy)]
struct POINT { x: i32, y: i32 }

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

static mut WINDOW_HWND: HWND = ptr::null_mut();

/// 找/造桌面层父窗口：Progman → 发 0x052C 逼出 WorkerW；再取不到就退回 Progman。
unsafe fn desktop_parent() -> HWND {
    // 模式：progman=直挂图标层；toplevel=不挂父(顶层沉底)；默认 workerw
    let mode: Vec<String> = std::env::args().collect();
    let mode = mode.get(1).map(String::as_str).unwrap_or("workerw");
    if mode == "toplevel" {
        return ptr::null_mut();
    }
    let progman = FindWindowW(wide("Progman").as_ptr(), ptr::null());
    if progman.is_null() {
        return ptr::null_mut();
    }
    if mode == "progman" {
        return progman;
    }
    let mut res: usize = 0;
    SendMessageTimeoutW(progman, SHELL_MESSAGE, 0, 0, SMTO_NORMAL, 1000, &mut res);
    let mut worker = FindWindowExW(progman, ptr::null_mut(), wide("WorkerW").as_ptr(), ptr::null());
    if worker.is_null() {
        // WorkerW 可能是 Progman 的兄弟窗口（桌面宿主在其下）
        let mut h = FindWindowExW(ptr::null_mut(), progman, ptr::null(), ptr::null());
        while !h.is_null() {
            if !FindWindowExW(h, ptr::null_mut(), wide("SHELLDLL_DefView").as_ptr(), ptr::null()).is_null() {
                worker = h;
                break;
            }
            h = FindWindowExW(ptr::null_mut(), h, ptr::null(), ptr::null());
        }
    }
    if worker.is_null() { progman } else { worker }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY | WM_CLOSE => {
            KillTimer(hwnd, TIMER_ID);
            PostQuitMessage(0);
            0
        }
        WM_NCHITTEST => {
            // 整窗可拖（spike 够用；正式 UI 会区分标题栏/内容区）
            HTCAPTION
        }
        WM_LBUTTONUP => {
            // 演示：左键在桌面层窗口上按下 → 触发归档占位（正式实现走 IDropTarget）
            println!("[spike] click on shelf (archive hook placeholder)");
            DefWindowProcW(hwnd, msg, w, l)
        }
        WM_RBUTTONUP => {
            println!("[spike] quit requested");
            DestroyWindow(hwnd);
            0
        }
        WM_TIMER => {
            // 顶层沉底模式：显示桌面/最小化后把它拽回来，保持桌面可见
            if std::env::args().nth(1).as_deref() == Some("toplevel") {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, 9 /* SW_RESTORE */);
                }
                if IsWindowVisible(hwnd) == 0 {
                    ShowWindow(hwnd, 5 /* SW_SHOW */);
                }
                SetWindowPos(hwnd, HWND_BOTTOM, 2360, 800, 190, 300, SWP_NOACTIVATE);
                return 0;
            }
            // 重挂检查：WS_POPUP 的 GetParent 语义是"所有者"（恒 NULL），必须用 GetAncestor 查父链
            let parent = GetAncestor(hwnd, 1 /* GA_PARENT */);
            let on_desktop_layer = !parent.is_null()
                && IsWindow(parent) != 0
                && {
                    let mut buf = [0u16; 16];
                    let n = GetClassNameW(parent, buf.as_mut_ptr(), 16);
                    if n <= 0 {
                        false
                    } else {
                        let cls = String::from_utf16_lossy(&buf[..n as usize]);
                        cls == "WorkerW" || cls == "Progman"
                    }
                };
            if !on_desktop_layer {
                let target = desktop_parent();
                if !target.is_null() {
                    SetParent(hwnd, target);
                    SetWindowPos(hwnd, HWND_BOTTOM, 200, 200, 360, 220, SWP_NOACTIVATE | SWP_FRAMECHANGED);
                    println!("[spike] re-attached to desktop layer");
                }
            }
            0
        }
        WM_DISPLAYCHANGE => {
            // 分辨率/显示器变化：重挂一次（C2/C3）
            SetTimer(hwnd, TIMER_ID, 2000, ptr::null_mut());
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

fn main() {
    let class_name = wide("NestDeskSpike");
    unsafe {
        SetProcessDpiAwarenessContext(-4isize as *mut c_void); // PER_MONITOR_AWARE_V2
        let hinst = GetModuleHandleW(ptr::null());
        let wc = WNDCLASSEXW {
            cb_size: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0x0003 /* CS_HREDRAW|CS_VREDRAW */,
            lpfn_wnd_proc: wnd_proc,
            cb_cls_extra: 0, cb_wnd_extra: 0,
            h_instance: hinst,
            h_icon: ptr::null_mut(), h_cursor: ptr::null_mut(),
            h_br_background: 6 as *mut c_void, // COLOR_APPWORKSPACE+... 用整数 brush 索引即可，spike 不讲究
            lpsz_menu_name: ptr::null(),
            lpsz_class_name: class_name.as_ptr(),
            h_icon_sm: ptr::null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
            eprintln!("[spike] RegisterClassExW failed: {}", std::io::Error::last_os_error());
            return;
        }
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name.as_ptr(), wide("归巢 NestDesk — S0 桌面层演示（右键退出）").as_ptr(),
            WS_POPUP | WS_VISIBLE,
            200, 200, 360, 220,
            ptr::null_mut(), ptr::null_mut(), hinst, ptr::null_mut(),
        );
        if hwnd.is_null() {
            eprintln!("[spike] CreateWindowExW failed: {}", std::io::Error::last_os_error());
            return;
        }
        WINDOW_HWND = hwnd;
        // 去掉阴影边框即可（保持 WS_POPUP）
        let ex = (SetWindowLongPtrW(hwnd, GWL_EXSTYLE_IDX, WS_EX_TOOLWINDOW as isize | WS_EX_LAYERED as isize)) as u32;
        println!("[spike] created hwnd={hwnd:p} ex=0x{ex:x}");
        SetLayeredWindowAttributes(hwnd, 0, 220, LWA_ALPHA);
        let target = desktop_parent();
        if target.is_null() {
            println!("[spike] toplevel mode: staying as bottom-level top window");
            ShowWindow(hwnd, 5);
            SetWindowPos(hwnd, HWND_BOTTOM, 2360, 800, 190, 300, SWP_NOACTIVATE);
        } else {
            SetParent(hwnd, target);
            println!("[spike] attached to desktop parent {target:p}");
        }
        SetTimer(hwnd, TIMER_ID, 2000, ptr::null_mut());

        let mut msg = std::mem::zeroed::<MSG>();
        loop {
            let r = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);
            if r <= 0 {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    let _ = Duration::from_secs(0);
}
