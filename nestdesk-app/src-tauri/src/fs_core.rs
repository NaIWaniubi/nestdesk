// fs_core.rs — 归档/分类/journal/复原核心逻辑。纯文件系统，无 tauri 依赖，可独立自测。
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Config {
    pub desktop: PathBuf,   // 散落文件来源
    pub inbox_root: PathBuf, // 归档目标根（Nest/收件箱）
    pub journal: PathBuf,    // 事务日志（JSONL，append）
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    pub batch: String,
    pub old: String,
    pub new: String,
    pub ts: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Moved { pub old: String, pub new: String, pub group: &'static str }
#[derive(Serialize, Clone, Debug)]
pub struct Failed { pub path: String, pub reason: String }
#[derive(Serialize, Clone, Debug)]
pub struct IngestResult { pub batch: String, pub moved: Vec<Moved>, pub failed: Vec<Failed> }
#[derive(Serialize, Clone, Debug)]
pub struct UndoReport { pub restored: usize, pub renamed: usize, pub missing: Vec<String>, pub batches_left: usize }

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn group_of(p: &Path) -> &'static str {
    if p.is_dir() {
        return "文件夹";
    }
    match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("webp") | Some("bmp") | Some("ico") | Some("svg") | Some("tiff") | Some("heic") => "图片",
        Some("pdf") | Some("doc") | Some("docx") | Some("xls") | Some("xlsx") | Some("ppt") | Some("pptx") | Some("txt") | Some("md") | Some("rtf") | Some("csv") | Some("epub") | Some("odt") => "文档",
        Some("zip") | Some("rar") | Some("7z") | Some("tar") | Some("gz") | Some("bz2") | Some("xz") | Some("iso") => "压缩包",
        Some("exe") | Some("msi") | Some("appx") | Some("msix") => "安装包",
        Some("mp4") | Some("mkv") | Some("avi") | Some("mov") | Some("wmv") | Some("flv") | Some("webm") => "视频",
        Some("mp3") | Some("wav") | Some("flac") | Some("aac") | Some("ogg") | Some("m4a") => "音频",
        Some("js") | Some("ts") | Some("tsx") | Some("jsx") | Some("json") | Some("rs") | Some("py") | Some("go") | Some("java") | Some("c") | Some("h") | Some("cpp") | Some("cs") | Some("html") | Some("css") | Some("sh") | Some("bat") | Some("ps1") | Some("sql") | Some("toml") | Some("yaml") | Some("yml") => "代码",
        _ => "其他",
    }
}

fn split_name(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.to_string(), String::new()),
    }
}

pub fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let (stem, ext) = split_name(name);
    let mut cand = dir.join(name);
    let mut i = 2;
    while cand.exists() {
        cand = dir.join(format!("{stem} ({i}){ext}"));
        i += 1;
    }
    cand
}

fn move_any(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p)?;
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            // 跨卷等：复制后删除
            copy_tree(src, dst)?;
            let _ = if src.is_dir() { fs::remove_dir_all(src) } else { fs::remove_file(src) };
            Ok(())
        }
    }
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for e in fs::read_dir(src)? {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
    } else {
        fs::copy(src, dst)?;
    }
    Ok(())
}

pub fn read_entries(cfg: &Config) -> Vec<Entry> {
    let Ok(data) = fs::read_to_string(&cfg.journal) else { return Vec::new() };
    data.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

fn append_entries(cfg: &Config, entries: &[Entry]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(p) = cfg.journal.parent() {
        fs::create_dir_all(p)?;
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&cfg.journal)?;
    for e in entries {
        writeln!(f, "{}", serde_json::to_string(e).unwrap())?;
    }
    Ok(())
}

pub fn ingest(cfg: &Config, paths: Vec<String>) -> IngestResult {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let batch = format!("{}-{}", now_ms(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let mut moved = Vec::new();
    let mut failed = Vec::new();
    let mut entries = Vec::new();
    for p in paths {
        let src = PathBuf::from(&p);
        let name = match src.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => { failed.push(Failed { path: p.clone(), reason: "非法路径".into() }); continue; }
        };
        if !src.exists() {
            failed.push(Failed { path: p.clone(), reason: "文件不存在（可能被占用移动过）".into() });
            continue;
        }
        // 已在收纳体系内的不再二次收
        if src.starts_with(&cfg.inbox_root) {
            continue;
        }
        let g = group_of(&src);
        let dest = unique_dest(&cfg.inbox_root.join(g), &name);
        match move_any(&src, &dest) {
            Ok(()) => {
                let new = dest.to_string_lossy().to_string();
                entries.push(Entry { batch: batch.clone(), old: p.clone(), new: new.clone(), ts: now_ms() });
                moved.push(Moved { old: p, new, group: g });
            }
            Err(e) => failed.push(Failed { path: p, reason: e.to_string() }),
        }
    }
    if !entries.is_empty() {
        let _ = append_entries(cfg, &entries);
    }
    IngestResult { batch, moved, failed }
}

pub fn collect_desktop(cfg: &Config) -> IngestResult {
    let mut paths = Vec::new();
    if let Ok(rd) = fs::read_dir(&cfg.desktop) {
        for e in rd.flatten() {
            if e.path().is_file() {
                paths.push(e.path().to_string_lossy().to_string());
            }
        }
    }
    ingest(cfg, paths)
}

fn undo_entries(cfg: &Config, sel: Vec<Entry>, total: Vec<Entry>) -> UndoReport {
    let mut restored = 0usize;
    let mut renamed = 0usize;
    let mut missing = Vec::new();
    let sel_batches: HashSet<String> = sel.iter().map(|e| e.batch.clone()).collect();
    for e in sel.iter().rev() {
        let new_p = Path::new(&e.new);
        let old_p = Path::new(&e.old);
        if !new_p.exists() {
            missing.push(e.new.clone());
            continue;
        }
        if old_p.exists() {
            // 原位置被占：挪到旁边并记冲突
            let dir = old_p.parent().unwrap_or(Path::new("."));
            let name = old_p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let (stem, ext) = split_name(&name);
            let mut i = 1;
            let cand = loop {
                let c = if i == 1 { dir.join(format!("{stem} ·复原{ext}")) } else { dir.join(format!("{stem} ·复原{i}{ext}")) };
                if !c.exists() { break c; }
                i += 1;
            };
            let _ = move_any(new_p, &cand);
            renamed += 1;
            restored += 1;
        } else if move_any(new_p, old_p).is_ok() {
            restored += 1;
        } else {
            missing.push(e.new.clone());
        }
    }
    // 重写日志：去掉已回放的行
    let keep: Vec<Entry> = total.into_iter().filter(|e| !sel_batches.contains(&e.batch)).collect();
    if let Some(p) = cfg.journal.parent() {
        let _ = fs::create_dir_all(p);
    }
    let tmp = cfg.journal.with_extension("jsonl.tmp");
    let body: String = keep.iter().map(|e| format!("{}\n", serde_json::to_string(e).unwrap())).collect();
    let _ = fs::write(&tmp, body);
    let _ = fs::rename(&tmp, &cfg.journal);
    let left: HashSet<String> = keep.iter().map(|e| e.batch.clone()).collect();
    UndoReport { restored, renamed, missing, batches_left: left.len() }
}

pub fn undo_last(cfg: &Config) -> UndoReport {
    let all = read_entries(cfg);
    let Some(last_batch) = all.last().map(|e| e.batch.clone()) else {
        return UndoReport { restored: 0, renamed: 0, missing: vec![], batches_left: 0 };
    };
    let sel: Vec<Entry> = all.iter().filter(|e| e.batch == last_batch).cloned().collect();
    undo_entries(cfg, sel, all)
}

pub fn undo_all(cfg: &Config) -> UndoReport {
    let all = read_entries(cfg);
    undo_entries(cfg, all.clone(), all)
}

#[derive(Serialize, Clone)]
pub struct DesktopFile { pub name: String, pub size: u64, pub is_dir: bool }
#[derive(Serialize, Clone)]
pub struct GroupStat { pub group: String, pub count: usize, pub bytes: u64, pub files: Vec<String> }
#[derive(Serialize, Clone)]
pub struct Snapshot {
    pub desktop_files: usize,
    pub desktop: Vec<DesktopFile>,
    pub inbox: Vec<GroupStat>,
    pub inbox_total: usize,
    pub journal_len: usize,
    pub journal_bytes: u64,
}

pub fn snapshot(cfg: &Config) -> Snapshot {
    let mut desktop: Vec<DesktopFile> = Vec::new();
    if let Ok(rd) = fs::read_dir(&cfg.desktop) {
        for e in rd.flatten() {
            let is_dir = e.path().is_dir();
            let size = if is_dir { 0 } else { e.metadata().map(|m| m.len()).unwrap_or(0) };
            desktop.push(DesktopFile { name: e.file_name().to_string_lossy().to_string(), size, is_dir });
        }
    }
    desktop.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(b.size.cmp(&a.size)));
    let mut groups: HashMap<String, (usize, u64, Vec<String>)> = HashMap::new();
    if let Ok(rd) = fs::read_dir(&cfg.inbox_root) {
        for g in rd.flatten() {
            let gp = g.path();
            if !gp.is_dir() { continue; }
            let name = g.file_name().to_string_lossy().to_string();
            let mut cnt = 0usize;
            let mut bytes = 0u64;
            let mut files = Vec::new();
            if let Ok(rd2) = fs::read_dir(&gp) {
                for f in rd2.flatten() {
                    cnt += 1;
                    bytes += f.metadata().map(|m| m.len()).unwrap_or(0);
                    files.push(f.file_name().to_string_lossy().to_string());
                }
            }
            files.sort();
            *groups.entry(name).or_default() = (cnt, bytes, files);
        }
    }
    let mut inbox: Vec<GroupStat> = groups.into_iter().map(|(group, (count, bytes, files))| GroupStat { group, count, bytes, files }).collect();
    inbox.sort_by(|a, b| b.count.cmp(&a.count));
    let journal_bytes = fs::metadata(&cfg.journal).map(|m| m.len()).unwrap_or(0);
    let ents = read_entries(cfg);
    Snapshot {
        desktop_files: desktop.iter().filter(|d| !d.is_dir).count(),
        desktop: desktop.into_iter().take(60).collect(),
        inbox_total: inbox.iter().map(|g| g.count).sum(),
        inbox,
        journal_len: ents.len(),
        journal_bytes,
    }
}
