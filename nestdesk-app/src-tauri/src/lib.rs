// lib.rs — NestDesk v0：桌面层 surface 引擎 + 收纳归档 + journal 复原 + 小组件 + 控制台设置
mod fs_core;
mod layer;

use fs_core::{Config, DeskItem, DupSet, IngestResult, Snapshot, SuggestPlan, UndoReport};
use std::process::Command;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;
use tauri::{Manager, PhysicalPosition, PhysicalSize, Position, Size};

pub const SURFACES: [&str; 5] = ["shelf", "clock", "sysmon", "note", "todo"];

// ---------- 应用级路径 ----------
fn appdata() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| ".".into()))
}
fn data_dir() -> PathBuf {
    let p = appdata().join("NestDesk");
    let _ = fs::create_dir_all(&p);
    p
}
fn user_home() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into()))
}
fn desktop_dir() -> PathBuf {
    // 会话内路径不变：memoize，避免每个 command 都拉起一次 powershell
    static DESKTOP: OnceLock<PathBuf> = OnceLock::new();
    DESKTOP.get_or_init(|| {
        // OneDrive 重定向时 USERPROFILE\Desktop 可能是假路径 → 问系统要真实桌面
        let found = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "[Environment]::GetFolderPath('Desktop')"])
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !s.is_empty() && Path::new(&s).exists() { Some(PathBuf::from(s)) } else { None }
            });
        found.unwrap_or_else(|| user_home().join("Desktop"))
    }).clone()
}

// ---------- 控制台设置（分组表 + 每面板透明度） ----------
#[derive(Serialize, Deserialize, Clone)]
struct GroupRule { name: String, exts: Vec<String> }

#[derive(Serialize, Deserialize, Clone)]
struct AppSettings {
    groups: Vec<GroupRule>,
    /// 每个面板的窗口级不透明度（30..=255，LWA_ALPHA）
    alpha: HashMap<String, u32>,
}

fn settings_path() -> PathBuf { data_dir().join("settings.json") }

fn settings_defaults() -> AppSettings {
    let groups: Vec<GroupRule> = fs_core::default_groups().iter()
        .map(|(n, e)| GroupRule { name: n.clone(), exts: e.clone() })
        .collect();
    let alpha = [("shelf", 254u32), ("clock", 250), ("sysmon", 250), ("note", 250)]
        .iter().map(|(k, v)| (k.to_string(), *v)).collect();
    AppSettings { groups, alpha }
}

static SETTINGS: OnceLock<RwLock<AppSettings>> = OnceLock::new();

fn init_settings() {
    if SETTINGS.get().is_some() {
        return;
    }
    let path = settings_path();
    let bak = settings_path().with_extension("json.bak");
    let parse = |p: &PathBuf| fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<AppSettings>(&s).ok());
    let base = if path.exists() {
        parse(&path)
            .or_else(|| {
                // 主文件损坏时用上一份 .bak 自愈，而不是静默回退默认设置
                if bak.exists() { parse(&bak) } else { None }
            })
            .unwrap_or_else(settings_defaults)
    } else {
        settings_defaults()
    };
    // 自愈：至少保留「其他」兜底格与四个面板键
    let mut s = base;
    if !s.groups.iter().any(|g| g.name == "其他") {
        s.groups.push(GroupRule { name: "其他".into(), exts: vec![] });
    }
    for k in SURFACES {
        s.alpha.entry(k.to_string()).or_insert(250);
    }
    let _ = SETTINGS.set(RwLock::new(s));
    save_settings_file();
}

fn current_settings() -> AppSettings {
    SETTINGS.get().map(|r| r.read().unwrap_or_else(|e| e.into_inner()).clone()).unwrap_or_else(settings_defaults)
}

fn save_settings_file() {
    if let Some(r) = SETTINGS.get() {
        let s = r.read().unwrap_or_else(|e| e.into_inner()).clone();
        if let Ok(txt) = serde_json::to_string_pretty(&s) {
            let tmp = settings_path().with_extension("json.tmp");
            let main = settings_path();
            let bak = settings_path().with_extension("json.bak");
            if fs::write(&tmp, txt).is_err() {
                return;
            }
            // 顺序：先复制旧文件为 .bak（主文件始终存在），再原子替换主文件
            if main.exists() {
                let _ = fs::copy(&main, &bak);
            }
            let _ = fs::rename(&tmp, &main);
        }
    }
}

fn real_cfg() -> Config {
    let s = current_settings();
    let groups: Vec<(String, Vec<String>)> = s.groups.into_iter().map(|g| (g.name, g.exts)).collect();
    Config {
        desktop: desktop_dir(),
        inbox_root: user_home().join("Nest").join("收件箱"),
        journal: data_dir().join("journal.jsonl"),
        groups: Some(groups),
    }
}

// ---------- 收纳/复原 commands ----------
#[tauri::command]
fn shelf_snapshot() -> Snapshot {
    fs_core::snapshot(&real_cfg())
}
#[tauri::command]
fn shelf_ingest(paths: Vec<String>) -> IngestResult {
    fs_core::ingest(&real_cfg(), paths)
}
#[tauri::command]
fn shelf_collect_desktop() -> IngestResult {
    fs_core::collect_desktop(&real_cfg())
}
#[tauri::command]
fn shelf_undo_last() -> UndoReport {
    fs_core::undo_last(&real_cfg())
}
#[tauri::command]
fn shelf_undo_all() -> UndoReport {
    fs_core::undo_all(&real_cfg())
}

// ---------- 文件交互（双击打开 / 资源管理器定位） ----------
#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("文件不存在（可能已被移动或删除）".into());
    }
    std::process::Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err("文件不存在（可能已被移动或删除）".into());
    }
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", p.to_string_lossy()))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// 透明度拖动时的即时预览：只改内存中的设置，钉扎循环 ≤2s 应用到 LWA_ALPHA。
/// 落盘仍由「保存」按钮统一执行。
#[tauri::command]
fn alpha_live(label: String, alpha: u32) {
    if !SURFACES.contains(&label.as_str()) {
        return;
    }
    if let Some(r) = SETTINGS.get() {
        let mut s = r.write().unwrap_or_else(|e| e.into_inner());
        s.alpha.insert(label, alpha.clamp(30, 255));
    }
}

// ---------- 系统监视 command ----------
#[derive(Serialize)]
struct Battery { percent: u8, ac: bool, charging: bool }
#[derive(Serialize)]
struct SysInfo { cpu: f64, mem_total: u64, mem_used: u64, battery: Option<Battery> }

#[tauri::command]
fn get_sysinfo() -> SysInfo {
    let (mem_total, mem_avail) = layer::memory().unwrap_or((0, 0));
    SysInfo {
        cpu: layer::cpu_usage(),
        mem_total,
        mem_used: mem_total.saturating_sub(mem_avail),
        battery: layer::battery().map(|(p, ac, ch)| Battery { percent: p, ac, charging: ch }),
    }
}

// ---------- 便利贴 ----------
#[tauri::command]
fn note_get() -> String {
    fs::read_to_string(data_dir().join("note.txt")).unwrap_or_default()
}
#[tauri::command]
fn note_set(text: String) {
    let _ = fs::write(data_dir().join("note.txt"), text);
}

// ---------- 皮肤 ----------
#[tauri::command]
fn theme_get() -> String {
    fs::read_to_string(data_dir().join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("theme").and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_else(|| "night".into())
}
#[tauri::command]
fn theme_set(theme: String) {
    // serde 序列化而非手拼 JSON：主题名含引号/反斜杠也不会写坏 config.json
    let v = serde_json::json!({ "theme": theme });
    if let Ok(txt) = serde_json::to_string(&v) {
        let _ = fs::write(data_dir().join("config.json"), txt);
    }
}

// ---------- v0.3 控制台融合 commands ----------
#[tauri::command]
fn shelf_desk_items() -> Vec<DeskItem> {
    init_settings();
    let s = current_settings();
    let groups: Vec<(String, Vec<String>)> = s.groups.iter().map(|g| (g.name.clone(), g.exts.clone())).collect();
    fs_core::desk_items(&real_cfg(), &groups)
}

#[tauri::command]
fn plan_suggest(threshold: Option<usize>) -> SuggestPlan {
    init_settings();
    fs_core::suggest_plan(&real_cfg(), threshold.unwrap_or(3))
}

#[tauri::command]
fn yaml_export() -> String {
    init_settings();
    let s = current_settings();
    let groups: Vec<(String, Vec<String>)> = s.groups.iter().map(|g| (g.name.clone(), g.exts.clone())).collect();
    fs_core::export_yaml(&groups)
}

#[tauri::command]
fn yaml_import(text: String) -> Result<AppSettings, String> {
    // 解析失败必须让前端看到原因，不再静默回退 current_settings 假装成功
    let groups = fs_core::import_yaml(&text)?;
    if groups.is_empty() {
        return Err("未解析出任何归纳文件夹规则".into());
    }
    let mut s = current_settings();
    s.groups = groups.into_iter().map(|(n, e)| GroupRule { name: n, exts: e }).collect();
    if !s.groups.iter().any(|g| g.name == "其他") {
        s.groups.push(GroupRule { name: "其他".into(), exts: vec![] });
    }
    if let Some(r) = SETTINGS.get() { *r.write().unwrap_or_else(|e| e.into_inner()) = s.clone(); }
    save_settings_file();
    Ok(s)
}

#[tauri::command]
fn shelf_batches() -> Vec<fs_core::BatchInfo> {
    fs_core::list_batches(&real_cfg())
}

#[tauri::command]
fn shelf_undo_selected(batches: Vec<String>) -> UndoReport {
    fs_core::undo_batches(&real_cfg(), batches)
}

#[tauri::command]
fn dup_scan() -> Result<Vec<DupSet>, String> {
    let cfg = real_cfg();
    fs_core::find_dups(&cfg.inbox_root)
}

#[tauri::command]
fn dup_archive(shas: Vec<String>, keep_newest: bool) -> IngestResult {
    let cfg = real_cfg();
    if shas.is_empty() {
        return IngestResult { batch: String::new(), moved: vec![], failed: vec![] };
    }
    let all = fs_core::find_dups(&cfg.inbox_root).unwrap_or_default();
    let sel: Vec<DupSet> = all.into_iter().filter(|d| shas.contains(&d.sha)).collect();
    fs_core::archive_dups(&cfg, &sel, keep_newest)
}

// ---------- 布局持久化 + 桌面层钉扎 ----------
#[derive(Serialize, serde::Deserialize, Clone, Copy, PartialEq)]
struct Geom { x: i32, y: i32, w: u32, h: u32 }

fn layout_path() -> PathBuf { data_dir().join("layout.json") }

#[derive(serde::Deserialize, serde::Serialize)]
struct LayoutFile(HashMap<String, Geom>);

fn load_layout() -> HashMap<String, Geom> {
    let parse = |p: &Path| fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<LayoutFile>(&s).ok()).map(|l| l.0);
    let main = layout_path();
    parse(&main)
        .or_else(|| {
            // 主文件损坏时用上一份 .bak 自愈（与 settings 同策略）
            let bak = layout_path().with_extension("json.bak");
            if bak.exists() { parse(&bak) } else { None }
        })
        .unwrap_or_default()
}

fn save_layout(map: &HashMap<String, Geom>) {
    let tmp = layout_path().with_extension("json.tmp");
    if let Ok(s) = serde_json::to_string(&LayoutFile(map.clone())) {
        let _ = fs::write(&tmp, s);
        let bak = layout_path().with_extension("json.bak");
        let main = layout_path();
        if main.exists() {
            let _ = fs::rename(&main, bak);
        }
        let _ = fs::rename(&tmp, &main);
    }
}

#[tauri::command]
fn settings_get() -> AppSettings {
    init_settings();
    current_settings()
}

#[tauri::command]
fn settings_save(groups: Vec<GroupRule>, alpha: HashMap<String, u32>) -> AppSettings {
    // 组名/扩展名清洗：禁止路径穿越与目录注入（手改 settings.json 或 UI 输入同样兜底）
    let mut seen = std::collections::HashSet::new();
    let mut cleaned: Vec<GroupRule> = Vec::new();
    for g in groups {
        let name = fs_core::safe_group(&g.name);
        if !seen.insert(name.clone()) { continue; }
        let exts: Vec<String> = g.exts.into_iter()
            .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|e| !e.is_empty() && e.len() <= 16
                && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'))
            .collect();
        cleaned.push(GroupRule { name, exts });
    }
    if cleaned.is_empty() {
        return current_settings();
    }
    if !cleaned.iter().any(|g| g.name == "其他") {
        cleaned.push(GroupRule { name: "其他".into(), exts: vec![] });
    }
    let mut s = current_settings();
    s.groups = cleaned;
    for k in SURFACES {
        s.alpha.entry(k.to_string()).or_insert(250);
    }
    for (k, v) in &alpha {
        if *v >= 30 && *v <= 255 {
            s.alpha.insert(k.clone(), *v);
        }
    }
    if let Some(r) = SETTINGS.get() {
        *r.write().unwrap_or_else(|e| e.into_inner()) = s.clone();
    }
    save_settings_file();
    s
}

// ---------- v0.4 fusion commands ----------
#[tauri::command]
fn shelf_save_url(url: String) -> IngestResult {
    init_settings();
    let s = current_settings();
    let groups: Vec<(String, Vec<String>)> = s.groups.iter().map(|g| (g.name.clone(), g.exts.clone())).collect();
    fs_core::save_url(&real_cfg(), &groups, &url)
}

// ---- Glance: 农历（.NET ChineseLunisolarCalendar，零数据风险）----
const LUNAR_MONTHS: [&str; 12] = ["正", "二", "三", "四", "五", "六", "七", "八", "九", "十", "冬", "腊"];
const LUNAR_DAYS: [&str; 30] = [
    "初一","初二","初三","初四","初五","初六","初七","初八","初九","初十",
    "十一","十二","十三","十四","十五","十六","十七","十八","十九","二十",
    "廿一","廿二","廿三","廿四","廿五","廿六","廿七","廿八","廿九","三十",
];
const ZODIAC: [&str; 12] = ["鼠","牛","虎","兔","龙","蛇","马","羊","猴","鸡","狗","猪"];

#[tauri::command]
fn lunar_now() -> String {
    let script = "$c=New-Object System.Globalization.ChineseLunisolarCalendar; $d=Get-Date; $leap=$c.GetLeapMonth($c.GetYear($d)); '{0}|{1}|{2}|{3}|{4}' -f $c.GetYear($d), $c.GetMonth($d), $c.GetDayOfMonth($d), $leap, ([int]$d.DayOfWeek)";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output();
    let Ok(o) = out else { return String::new() };
    let s = String::from_utf8_lossy(&o.stdout);
    let parts: Vec<&str> = s.trim().split('|').collect();
    if parts.len() < 4 { return String::new() }
    let y: i32 = parts[0].trim().parse().unwrap_or(0);
    let m: u32 = parts[1].trim().parse().unwrap_or(0);
    let d: u32 = parts[2].trim().parse().unwrap_or(0);
    if y < 1900 || y > 2100 || m == 0 || d == 0 { return String::new() }
    let leap_month: u32 = parts[3].trim().parse().unwrap_or(0);
    let mi = ((m - 1) as usize).min(11);
    let is_leap = leap_month != 0 && m == leap_month;
    format!("{}{}月{} · {}年", if is_leap {"闰"} else {""}, LUNAR_MONTHS[mi], LUNAR_DAYS[(d as usize).saturating_sub(1)], ZODIAC[(((y - 4)).rem_euclid(12)) as usize])
}

// ---- Music host path config ----
#[tauri::command]
fn music_set(path: String) -> String {
    let cfg_path = data_dir().join("config.json");
    let mut map = serde_json::Map::new();
    if let Ok(txt) = std::fs::read_to_string(&cfg_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            if let Some(obj) = v.as_object() { map = obj.clone(); }
        }
    }
    map.insert("player".into(), serde_json::Value::String(path));
    let _ = std::fs::write(&cfg_path, serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default());
    "已保存播放器路径".into()
}

// ---- Todo (S-D) ----
#[derive(Serialize, Deserialize, Clone)]
struct TodoItem { text: String, done: bool, created: u64 }
#[tauri::command]
fn todo_list() -> Vec<TodoItem> {
    let p = data_dir().join("todo.json");
    std::fs::read_to_string(&p).ok()
        .and_then(|t| serde_json::from_str::<Vec<TodoItem>>(&t).ok())
        .unwrap_or_default()
}
#[tauri::command]
fn todo_add(text: String) -> Vec<TodoItem> {
    let mut list = todo_list();
    list.insert(0, TodoItem { text, done: false, created: fs_core::now_ms() });
    if list.len() > 100 { list.truncate(100); }
    save_todo(&list);
    list
}
#[tauri::command]
fn todo_toggle(i: usize) -> Vec<TodoItem> {
    let mut list = todo_list();
    if i < list.len() { list[i].done = !list[i].done; }
    save_todo(&list);
    list
}
#[tauri::command]
fn todo_del(i: usize) -> Vec<TodoItem> {
    let mut list = todo_list();
    if i < list.len() { list.remove(i); }
    save_todo(&list);
    list
}

fn save_todo(list: &[TodoItem]) {
    let tmp = data_dir().join("todo.json.tmp");
    if let Ok(txt) = serde_json::to_string_pretty(list) {
        let _ = std::fs::write(&tmp, txt);
        let main = data_dir().join("todo.json");
        let _ = std::fs::rename(&tmp, &main);
    }
}

// ---- Music host (S-E): launches Folia or configured player ----
#[tauri::command]
fn music_open() -> Result<String, String> {
    let cfg_path = data_dir().join("config.json");
    let raw_player = std::fs::read_to_string(&cfg_path).ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("player").and_then(|p| p.as_str()).map(String::from))
        .filter(|p| !p.is_empty());
    // 缺省顺序：配置的 player（必须存在且是文件）→ 系统默认 Windows Media Player。
    // 路径无效时返回明确指引，UI 可提示到控制台「更多」里重新保存。
    let target = match raw_player.as_deref() {
        Some(p) if std::path::Path::new(p).is_file() => p,
        Some(p) => return Err(format!("播放器路径不存在: {p} —— 在控制台「更多」标签里重新保存路径")),
        None => "wmplayer",
    };
    let mut cmd = Command::new(target);
    let out;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        out = cmd.creation_flags(0x0800_0008 /* DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP */).spawn();
    }
    #[cfg(not(windows))]
    { out = cmd.spawn(); }
    out.map(|_| format!("started: {target}")).map_err(|e| format!("failed: {e}"))
}

fn surfaces(app: &tauri::AppHandle) -> Vec<tauri::WebviewWindow> {
    SURFACES.iter().filter_map(|l| app.get_webview_window(l)).collect()
}

/// 参与几何记忆的窗口：5 个桌面面板 + 控制台。控制台不参与沉底/透明处理。
fn layout_windows(app: &tauri::AppHandle) -> Vec<tauri::WebviewWindow> {
    let mut v = surfaces(app);
    if let Some(w) = app.get_webview_window("settings") { v.push(w); }
    v
}

/// 屏外/负坐标钳制：至少 40×40 可见才保留，否则拉回主屏左上安全区（W-01）。
fn clamp_geom(app: &tauri::AppHandle, g: Geom) -> Geom {
    let mons = app.available_monitors().unwrap_or_default();
    if mons.is_empty() { return g; }
    for m in &mons {
        let p = m.position();
        let s = m.size();
        let ix = (g.x + g.w as i32).min(p.x + s.width as i32) - g.x.max(p.x);
        let iy = (g.y + g.h as i32).min(p.y + s.height as i32) - g.y.max(p.y);
        if ix >= 40 && iy >= 40 {
            return g;
        }
    }
    let p = mons[0].position();
    let s = mons[0].size();
    Geom {
        x: p.x + 40,
        y: p.y + 60,
        w: g.w.clamp(120, s.width.max(120)),
        h: g.h.clamp(80, s.height.max(80)),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_settings();
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            shelf_snapshot, shelf_ingest, shelf_collect_desktop, shelf_undo_last, shelf_undo_all,
            get_sysinfo, note_get, note_set, theme_get, theme_set,
            settings_get, settings_save,
            shelf_desk_items, plan_suggest, yaml_export, yaml_import,
            shelf_batches, shelf_undo_selected, dup_scan, dup_archive,
            shelf_save_url, todo_list, todo_add, todo_toggle, todo_del, music_open,
            lunar_now, music_set, open_path, reveal_path, alpha_live
        ])
        .build(tauri::generate_context!())
        .expect("NestDesk 启动失败");

    // 启动时恢复布局（layout.json 缺失/损坏则用 tauri.conf 默认位置）；屏外坐标先钳制
    for (label, g) in load_layout() {
        if let Some(w) = app.get_webview_window(label.as_str()) {
            let g = clamp_geom(app.handle(), g);
            let _ = w.set_position(Position::Physical(PhysicalPosition::new(g.x, g.y)));
            let _ = w.set_size(Size::Physical(PhysicalSize::new(g.w, g.h)));
        }
    }

    // 主循环线程：钉扎桌面层 + 位置采样（移动后 3 秒静止自动落盘 .bak 双份）
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        let mut last: HashMap<String, Geom> = HashMap::new();
        let mut applied_alpha: HashMap<String, u8> = HashMap::new();
        let mut dirty_since: Option<std::time::Instant> = None;
        loop {
            std::thread::sleep(Duration::from_millis(2000));
            let mut changed = false;
            let pin = std::env::var_os("NESTDESK_NOPIN").is_none();
            for w in layout_windows(&handle) {
                let is_surface = SURFACES.contains(&w.label());
                if let Ok(h) = w.hwnd() {
                    unsafe {
                        let ptr = h.0 as *mut std::ffi::c_void;
                        if pin && is_surface {
                            use std::sync::{LazyLock, Mutex};
                            use std::collections::HashSet;
                            static STYLED: LazyLock<Mutex<HashSet<usize>>> =
                                LazyLock::new(|| Mutex::new(HashSet::new()));
                            let addr = ptr as usize;
                            let mut set = STYLED.lock().unwrap_or_else(|e| e.into_inner());
                            // 清理已销毁窗口的死条目（HWND 地址复用误判的根源）
                            set.retain(|a| layer::is_window(*a as *mut std::ffi::c_void));
                            let fresh = !set.contains(&addr);
                            if fresh {
                                layer::ensure_layered(ptr);
                                layer::style_surface(ptr);
                                set.insert(addr);
                            }
                            drop(set);
                            // 透明度：设置值/句柄未变就不再下发（避免每 2s 触发 DWM 重合成）
                            let alpha = current_settings()
                                .alpha
                                .get(w.label())
                                .copied()
                                .unwrap_or(250)
                                .clamp(30, 255) as u8;
                            let key = format!("{}@{addr}", w.label());
                            if fresh || applied_alpha.get(&key) != Some(&alpha) {
                                layer::set_alpha(ptr, alpha);
                                applied_alpha.insert(key, alpha);
                            }
                            // Z 序：仅 Win+D 最小化或已不在最底时才重钉
                            layer::repin_if_needed(ptr);
                        }
                    }
                }
                if let (Ok(pos), Ok(size)) = (w.outer_position(), w.inner_size()) {
                    let g = clamp_geom(&handle, Geom { x: pos.x, y: pos.y, w: size.width, h: size.height });
                    if last.get(w.label()) != Some(&g) {
                        last.insert(w.label().to_string(), g);
                        changed = true;
                    }
                }
            }
            if changed {
                dirty_since = Some(std::time::Instant::now());
            } else if let Some(t) = dirty_since {
                if t.elapsed() > Duration::from_secs(3) {
                    save_layout(&last);
                    dirty_since = None;
                }
            }
        }
    });

    app.run(|_app, _event| {});
}

// ---------- --demo：真实桌面全链路（收取→分格→复原），JSON 证据输出后退出 ----------
#[cfg(any(debug_assertions, feature = "harness"))]
pub fn demo_run() {
    if std::env::var_os("NESTDESK_DEMO").is_none() {
        eprintln!("refusing --demo: it moves REAL desktop files. Set NESTDESK_DEMO=1 to confirm.");
        std::process::exit(2);
    }
    init_settings();
    let cfg = real_cfg();
    let before = fs_core::snapshot(&cfg);
    let res = fs_core::collect_desktop(&cfg);
    println!("{}", serde_json::json!({
        "phase": "collect",
        "desktop_before": before.desktop_files,
        "moved": res.moved.len(),
        "failed": res.failed.len(),
        "groups": res.moved.iter().map(|m| m.group.clone()).fold(std::collections::BTreeMap::new(), |mut acc, g| { *acc.entry(g).or_insert(0) += 1; acc }),
        "batch": res.batch,
    }));
    let after = fs_core::snapshot(&cfg);
    println!("{}", serde_json::json!({
        "phase": "after_collect",
        "inbox_groups": after.inbox.iter().map(|g| serde_json::json!({"group": g.group, "count": g.count})).collect::<Vec<_>>(),
        "inbox_total": after.inbox_total,
        "journal_len": after.journal_len,
    }));
    let undo = fs_core::undo_all(&cfg);
    let restored = fs_core::snapshot(&cfg);
    println!("{}", serde_json::json!({
        "phase": "undo_all",
        "restored": undo.restored,
        "renamed": undo.renamed,
        "missing": undo.missing,
        "batches_left": undo.batches_left,
        "desktop_after_undo": restored.desktop_files,
        "inbox_after_undo": restored.inbox_total,
    }));
}

// ---------- selftest：--selftest 全离线自测（A3/A4 自动化版，不碰真实桌面） ----------
#[cfg(any(debug_assertions, feature = "harness"))]
pub fn selftest() {
    // 兜底：任何断言/写盘 panic 也要产出结构化 JSON（CI 门禁不能只看到裸 panic）
    let outcome = std::panic::catch_unwind(selftest_body);
    match outcome {
        Ok(()) => {}
        Err(e) => {
            let msg = e.downcast_ref::<&str>().map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".into());
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({"pass": false, "panic": msg})).unwrap());
            std::process::exit(1);
        }
    }
}

#[cfg(any(debug_assertions, feature = "harness"))]
fn selftest_body() {
    use std::collections::HashSet;
    let root = std::env::temp_dir().join(format!("nestdesk-selftest-{}", fs_core::now_ms()));
    let desktop = root.join("Desktop");
    let inbox = root.join("Nest").join("收件箱");
    let alt = root.join("alt");
    let _ = fs::create_dir_all(&desktop);
    let _ = fs::create_dir_all(&alt);
    let cfg = Config { desktop: desktop.clone(), inbox_root: inbox.clone(), journal: root.join("journal.jsonl"), groups: None };

    let mut checks: Vec<serde_json::Value> = Vec::new();
    let mut report = |name: &str, ok: bool, info: String| {
        checks.push(serde_json::json!({"name": name, "ok": ok, "info": info}));
    };

    // 样本：多类型 + 中文名 + 大写扩展名 + 未知扩展 + 批间同名冲突
    let mut originals: HashMap<String, Vec<u8>> = HashMap::new();
    let mut paths_b1: Vec<String> = Vec::new();
    for i in 0..120 {
        let name = match i % 6 {
            0 => format!("报告 {i}.docx"),
            1 => format!("photo_{i}.PNG"),
            2 => format!("clip_{i}.mp4"),
            3 => format!("data_{i}.unknownext"),
            4 => format!("setup_{i}.msi"),
            _ => format!("code_{i}.rs"),
        };
        let p = desktop.join(&name);
        let content = format!("nestdesk-selftest-{name}").into_bytes();
        fs::write(&p, &content).unwrap();
        originals.insert(name, content);
        paths_b1.push(p.to_string_lossy().to_string());
    }
    let mut paths_b2: Vec<String> = Vec::new();
    for i in 0..80 {
        let name = format!("b2_{i}.txt");
        let p = alt.join(&name);
        let content = format!("alt-{name}").into_bytes();
        fs::write(&p, &content).unwrap();
        paths_b2.push(p.to_string_lossy().to_string());
    }

    let r1 = fs_core::ingest(&cfg, paths_b1);
    report("批1: 120 文件全部归档", r1.moved.len() == 120 && r1.failed.is_empty(), format!("{} ok / {} fail", r1.moved.len(), r1.failed.len()));
    let r2 = fs_core::ingest(&cfg, paths_b2);
    report("批2: 80 文件全部归档", r2.moved.len() == 80 && r2.failed.is_empty(), format!("{} ok / {} fail", r2.moved.len(), r2.failed.len()));

    // 批3：跨批同名冲突——5 个与批1同名 + 5 个与批2同名（不同目录、不同内容）
    let collide: Vec<String> = ["报告 0.docx", "photo_1.PNG", "data_3.unknownext", "setup_4.msi", "code_5.rs"]
        .iter()
        .map(|n| alt.join(n))
        .chain((0..5).map(|i| alt.join(format!("b2_{i}.txt"))))
        .map(|p| {
            fs::write(&p, format!("collide-{}", p.file_name().unwrap().to_string_lossy()).into_bytes()).unwrap();
            p.to_string_lossy().to_string()
        })
        .collect();
    let r3 = fs_core::ingest(&cfg, collide);
    let suffixed = r3.moved.iter().filter(|m| m.new.contains(" (2)")).count();
    report("冲突: 跨批同名 10/10 全部带 (2) 后缀归档", r3.moved.len() == 10 && suffixed == 10, format!("moved={} suffixed={}", r3.moved.len(), suffixed));

    let ents = fs_core::read_entries(&cfg);
    let in_group = |name: &str, want: &str| ents.iter().any(|e| e.new.contains(&format!("{want}\\{name}")) || e.new.contains(&format!("{want}/{name}")));
    report("分类: docx→文档", in_group("报告 0.docx", "文档"), String::new());
    report("分类: 大写.PNG→图片", in_group("photo_1.PNG", "图片"), String::new());
    report("分类: unknownext→其他", in_group("data_3.unknownext", "其他"), String::new());
    report("分类: msi→安装包", in_group("setup_4.msi", "安装包"), String::new());

    // 控制台自定义分组表（用户可自由增删归纳文件夹）
    let custom: Vec<(String, Vec<String>)> = vec![
        ("论文".to_string(), vec!["pdf".to_string(), "docx".to_string()]),
        ("数据集".to_string(), vec![]),
    ];
    let mut cfg_custom = cfg.clone();
    cfg_custom.groups = Some(custom);
    let samples: [(&str, &[u8]); 4] = [
        ("论文.txt", b"paper"), ("论文.pdf", b"paper2"),
        ("数据.bin", b"data"), ("杂项.zip", b"zip"),
    ];
    for (n, e) in samples.iter() {
        let p = alt.join(n);
        fs::write(&p, e.to_vec()).unwrap();
    }
    let rc = fs_core::ingest(&cfg_custom, ["论文.txt", "论文.pdf", "数据.bin", "杂项.zip"]
        .iter().map(|n| alt.join(n).to_string_lossy().to_string()).collect());
    let n_paper = rc.moved.iter().filter(|m| m.group == "论文").count();
    let n_data = rc.moved.iter().filter(|m| m.group == "数据集").count();
    report("自定分组: pdf→「论文」，其余落入表尾「数据集」兜底",
        rc.moved.len() == 4 && n_paper == 1 && n_data == 3,
        format!("paper={n_paper} data={n_data}"));

    // 删除一个已归档文件 → undo 计 missing 不崩
    if let Some(first) = ents.first() {
        let victim = first.new.clone();
        let _ = fs::remove_file(&victim);
    }

    let rep = fs_core::undo_all(&cfg);
    let mut found: HashSet<Vec<u8>> = HashSet::new();
    let mut scan = |dir: &PathBuf| {
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() {
                    if let Ok(b) = fs::read(&p) { found.insert(b); }
                }
            }
        }
    };
    scan(&desktop);
    scan(&alt);
    let miss: Vec<String> = originals.iter().filter(|(_, c)| !found.contains(*c)).map(|(n, _)| n.clone()).collect();
    report("复原: 其余文件字节级归位", miss.len() <= 1, format!("miss={:?}", miss));
    report("复原: 被删文件计入 missing", rep.missing.len() == 1, format!("{:?}", rep.missing));
    report("复原后 journal 清空", fs_core::read_entries(&cfg).is_empty(), String::new());

    let rep2 = fs_core::undo_all(&cfg);
    report("幂等: 重复全部复原 no-op", rep2.restored == 0 && rep2.batches_left == 0, format!("{rep2:?}"));

    // undo_last 粒度：只回滚末批
    let p1 = desktop.join("batchA.txt");
    let p2 = desktop.join("batchB.txt");
    fs::write(&p1, b"A").unwrap();
    fs::write(&p2, b"B").unwrap();
    fs_core::ingest(&cfg, vec![p1.to_string_lossy().to_string()]);
    fs_core::ingest(&cfg, vec![p2.to_string_lossy().to_string()]);
    let rep3 = fs_core::undo_last(&cfg);
    report("粒度: undo_last 仅回滚末批", rep3.restored == 1 && !p1.exists() && p2.exists() && fs_core::read_entries(&cfg).len() == 1, format!("{rep3:?}"));

    // v0.3: 智能推荐
    for i in 0..5 { fs::write(desktop.join(format!("dwg_{i}.dwg")), b"cad").unwrap(); }
    for i in 0..4 { fs::write(desktop.join(format!("font_{i}.ttf")), b"font").unwrap(); }
    let plan = fs_core::suggest_plan(&cfg, 3);
    let has_cad = plan.suggested.iter().any(|g| g.name == "CAD图纸" && g.exts.iter().any(|e| e == "dwg"));
    let has_font = plan.suggested.iter().any(|g| g.name == "字体" && g.exts.iter().any(|e| e == "ttf"));
    report("智能推荐: 检测到 CAD图纸/字体 领域未覆盖", has_cad && has_font, format!("{} 条建议", plan.suggested.len()));

    // v0.3: 事务批次勾选复原（独立子沙箱，避免前后步楠屾薄染）
    let sub = root.join("selbox"); let subd = sub.join("Desktop"); let subi = sub.join("Nest").join("收件箱");
    let _ = fs::create_dir_all(&subd); let _ = fs::create_dir_all(&subi);
    let cfg2 = Config { desktop: subd.clone(), inbox_root: subi.clone(), journal: sub.join("journal.jsonl"), groups: None };
    let sa = sub.join("paperX-aaa.pdf"); fs::write(&sa, b"A").unwrap();
    let sb = sub.join("paperX-bbb.pdf"); fs::write(&sb, b"B").unwrap();
    fs_core::ingest(&cfg2, vec![sa.to_string_lossy().to_string()]);
    fs_core::ingest(&cfg2, vec![sb.to_string_lossy().to_string()]);
    let bl = fs_core::list_batches(&cfg2);
    assert_eq!(bl.len(), 2);
    let target_batch = bl.last().unwrap().batch.clone(); // oldest
    let rep_sel = fs_core::undo_batches(&cfg2, vec![target_batch]);
    let sel_ok = rep_sel.restored == 1 && sa.exists() && !sb.exists() && fs_core::list_batches(&cfg2).len() == 1;
    report("勾选复原: 仅回滚指定批次，其余保留", sel_ok, format!("target={} {:?}", bl.last().unwrap().batch, rep_sel));

    // v0.3: 查重模块（Czkawka 思路）
    let dc = cfg2.inbox_root.join("文档"); let _ = fs::create_dir_all(&dc);
    for i in 0..3 { fs::write(dc.join(format!("same_{}.pdf", i)), b"DUPLICATE-CONTENT").unwrap(); }
    fs::write(dc.join("uniq.pdf"), b"UNIQUE").unwrap();
    let cl = b"DUPLICATE-CONTENT".len() as u64;
    let sets = fs_core::find_dups(&cfg2.inbox_root).unwrap();
    let ds = sets.iter().find(|d| d.count == 3 && d.size == cl).expect("dup set");
    let mov = fs_core::archive_dups(&cfg2, std::slice::from_ref(ds), true);
    let moved_back_ok = {
        // undo the dup batch → originals return
        let r = fs_core::undo_all(&cfg2);
        (dc.join("same_0.pdf")).exists() && r.restored >= 1
    };
    report("查重: 3份相同→组成 1 组·1 几何，归档可复原", sets.len() == 1 && mov.moved.len() == 2 && moved_back_ok,
        format!("sets={} moved={}", sets.len(), mov.moved.len()));

    // v0.5 回归 1：URL 下载条目 undo = 撤销下载（删除落点），不混入 missing
    let urlfile = cfg2.inbox_root.join("图片").join("downloaded.png");
    let _ = fs::create_dir_all(urlfile.parent().unwrap());
    fs::write(&urlfile, b"PNG").unwrap();
    let url_entry = serde_json::json!({
        "batch": "url-test", "old": "@url|https://example.com/a.png",
        "new": urlfile.to_string_lossy(), "ts": fs_core::now_ms()
    });
    let mut jbody = fs::read_to_string(&cfg2.journal).unwrap_or_default();
    jbody.push_str(&format!("{}\n", url_entry));
    fs::write(&cfg2.journal, jbody).unwrap();
    let ru = fs_core::undo_all(&cfg2);
    report("URL 下载: undo 撤销下载（删除落点）而非报 missing",
        ru.restored == 1 && !urlfile.exists() && ru.missing.is_empty(), format!("{ru:?}"));

    // v0.5 回归 2：YAML 导出→导入严格往返 + 路径穿越组名被拒
    let src_groups = vec![
        ("文档".to_string(), vec!["docx".to_string(), "pdf".to_string()]),
        ("图片".to_string(), vec!["png".to_string()]),
    ];
    let y = fs_core::export_yaml(&src_groups);
    let back = fs_core::import_yaml(&y).unwrap_or_default();
    report("YAML: 导出→导入严格往返", back == src_groups, format!("{back:?}"));
    let evil = fs_core::import_yaml("groups:\n  - name: ../../outside\n    exts: [zip]\n");
    report("YAML: 路径穿越组名被拒绝", evil.is_err(), format!("{evil:?}"));

    // v0.5 回归 3：事务批次的 groups 是真实组名（不再恒为「归档」）
    let gfile = sub.join("pic.png");
    fs::write(&gfile, b"p").unwrap();
    fs_core::ingest(&cfg2, vec![gfile.to_string_lossy().to_string()]);
    let bb = fs_core::list_batches(&cfg2);
    let grp_ok = bb.iter().any(|b| b.groups.iter().any(|(g, _)| g == "图片"));
    report("事务: 批次分组显示真实组名", grp_ok, format!("{:?}", bb.first().map(|b| &b.groups)));

    let pass = checks.iter().all(|c| c["ok"].as_bool().unwrap_or(false));
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({"pass": pass, "checks": checks, "root": root})).unwrap());
    // 不留垃圾：每次自检的临时目录随手清掉（历史累积过 24 个孤儿目录）
    let _ = fs::remove_dir_all(&root);
    std::process::exit(if pass { 0 } else { 1 });
}
