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
    pub groups: Option<Vec<(String, Vec<String>)>>, // 控制台自定义分组表；None=内置默认
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    pub batch: String,
    pub old: String,
    pub new: String,
    pub ts: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Moved { pub old: String, pub new: String, pub group: String }
#[derive(Serialize, Clone, Debug)]
pub struct Failed { pub path: String, pub reason: String }
#[derive(Serialize, Clone, Debug)]
pub struct IngestResult { pub batch: String, pub moved: Vec<Moved>, pub failed: Vec<Failed> }
#[derive(Serialize, Clone, Debug)]
pub struct UndoReport { pub restored: usize, pub renamed: usize, pub missing: Vec<String>, pub batches_left: usize }

/// 内置默认分组（= 分类体系底稿，控制台可增删改）
pub fn default_groups() -> Vec<(String, Vec<String>)> {
    vec![
        ("图片".to_string(), vec!["png","jpg","jpeg","gif","webp","bmp","ico","svg","tiff","heic"].iter().map(|s| s.to_string()).collect()),
        ("文档".to_string(), vec!["pdf","doc","docx","xls","xlsx","ppt","pptx","txt","md","rtf","csv","epub","odt"].iter().map(|s| s.to_string()).collect()),
        ("压缩包".to_string(), vec!["zip","rar","7z","tar","gz","bz2","xz","iso"].iter().map(|s| s.to_string()).collect()),
        ("安装包".to_string(), vec!["exe","msi","appx","msix"].iter().map(|s| s.to_string()).collect()),
        ("视频".to_string(), vec!["mp4","mkv","avi","mov","wmv","flv","webm"].iter().map(|s| s.to_string()).collect()),
        ("音频".to_string(), vec!["mp3","wav","flac","aac","ogg","m4a"].iter().map(|s| s.to_string()).collect()),
        ("代码".to_string(), vec!["js","ts","tsx","jsx","json","rs","py","go","java","c","h","cpp","cs","html","css","sh","bat","ps1","sql","toml","yaml","yml"].iter().map(|s| s.to_string()).collect()),
        ("文件夹".to_string(), vec![]),
        ("其他".to_string(), vec![]),
    ]
}

/// 按控制台配置的分组表归类；无匹配 → 「其他」（或表尾）
pub fn group_resolve(p: &Path, groups: &[(String, Vec<String>)]) -> String {
    if p.is_dir() {
        return if groups.iter().any(|(n, _)| n == "文件夹") {
            "文件夹".to_string()
        } else {
            groups.last().map(|(n, _)| n.clone()).unwrap_or_else(|| "其他".into())
        };
    }
    let ext = p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
    if let Some(e) = ext {
        for (n, exts) in groups {
            if exts.contains(&e) {
                return n.clone();
            }
        }
    }
    if groups.iter().any(|(n, _)| n == "其他") {
        "其他".to_string()
    } else {
        groups.last().map(|(n, _)| n.clone()).unwrap_or_else(|| "其他".into())
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn split_name(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.to_string(), String::new()),
    }
}

pub fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let (mut stem, ext) = split_name(name);
    // Windows MAX_PATH 防呆：目录+扩展名之外给 stem 留的预算不足时截断
    let budget = 250usize.saturating_sub(dir.to_string_lossy().chars().count() + ext.chars().count() + 10);
    if budget >= 8 && stem.chars().count() > budget {
        stem = stem.chars().take(budget).collect();
    }
    let mut cand = dir.join(format!("{stem}{ext}"));
    if cand.exists() {
        let mut i = 2u32;
        loop {
            if i <= 10_000 {
                let c = dir.join(format!("{stem} ({i}){ext}"));
                if !c.exists() { cand = c; break; }
                i += 1;
            } else {
                // exists 异常（网络盘/链接回路）时不再无限循环，退化为时间戳后缀
                let c = dir.join(format!("{stem}-{}{ext}", now_ms() % 100_000));
                if !c.exists() { cand = c; break; }
                cand = dir.join(format!("{stem}-{}-{}{ext}", now_ms() % 100_000, std::process::id()));
                break;
            }
        }
    }
    cand
}

pub(crate) fn sha256_of(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 16];
    loop {
        match std::io::Read::read(&mut file, &mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return None,
        }
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// cross-volume transport: copy then SHA-256 verify then delete source (DeskBox pattern); same-volume stays atomic rename.
/// 关键不变量：**哈希不可得即拒绝搬运**，绝不出现「未校验就删源」。
fn move_any(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p)?;
    }
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    if src.is_dir() {
        // 目录：先对整棵树取哈希快照，复制后逐文件比对
        let mut want: Vec<(PathBuf, String)> = Vec::new();
        hash_tree(src, src, &mut want)?;
        if copy_tree(src, dst).is_err() {
            let _ = std::fs::remove_dir_all(dst);
            return Err(std::io::Error::other("copy failed"));
        }
        let mut got: Vec<(PathBuf, String)> = Vec::new();
        if hash_tree(dst, dst, &mut got).is_err() {
            let _ = std::fs::remove_dir_all(dst);
            return Err(std::io::Error::other("verify failed: target unreadable, source kept"));
        }
        want.sort_by(|a, b| a.0.cmp(&b.0));
        got.sort_by(|a, b| a.0.cmp(&b.0));
        if want != got {
            let _ = std::fs::remove_dir_all(dst);
            return Err(std::io::Error::other("verify failed: tree hash mismatch, source kept"));
        }
        std::fs::remove_dir_all(src).map_err(|e| {
            std::io::Error::other(format!("copy verified but source delete failed (both copies kept): {e}"))
        })?;
        return Ok(());
    }
    let want = sha256_of(src).ok_or_else(|| {
        std::io::Error::other("sha256 failed: source unreadable, move aborted (source kept)")
    })?;
    if copy_tree(src, dst).is_err() {
        let _ = std::fs::remove_file(dst);
        return Err(std::io::Error::other("copy failed"));
    }
    if sha256_of(dst).as_deref() != Some(want.as_str()) {
        let _ = std::fs::remove_file(dst);
        return Err(std::io::Error::other("verify failed: hash mismatch, source kept"));
    }
    std::fs::remove_file(src).map_err(|e| {
        std::io::Error::other(format!("copy verified but source delete failed (both copies kept): {e}"))
    })?;
    Ok(())
}

/// 递归收集 (相对路径, SHA-256)；任一文件不可哈希即整棵搬运失败。
fn hash_tree(root: &Path, base: &Path, out: &mut Vec<(PathBuf, String)>) -> std::io::Result<()> {
    for e in std::fs::read_dir(root)? {
        let e = e?;
        let p = e.path();
        let rel = p.strip_prefix(base).unwrap_or(&p).to_path_buf();
        if p.is_dir() {
            hash_tree(&p, base, out)?;
        } else {
            let h = sha256_of(&p).ok_or_else(|| {
                std::io::Error::other(format!("sha256 failed: {}", p.display()))
            })?;
            out.push((rel, h));
        }
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

// journal 读-改-写与并发 append 的互斥锁：undo 期间禁止新条目插入，避免整体重写丢行
static JOURNAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn journal_guard() -> std::sync::MutexGuard<'static, ()> {
    JOURNAL_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn read_entries(cfg: &Config) -> Vec<Entry> {
    let Ok(data) = std::fs::read_to_string(&cfg.journal) else { return Vec::new(); };
    data.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

fn append_entries(cfg: &Config, entries: &[Entry]) -> std::io::Result<()> {
    let _g = journal_guard();
    use std::io::Write;
    if let Some(p) = cfg.journal.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&cfg.journal)?;
    for e in entries {
        writeln!(f, "{}", serde_json::to_string(e).unwrap())?;
    }
    Ok(())
}

/// S-B: http(s) download-to-grid (DeskBox multi-source drop), passes journal, 100MB guard.
/// 复原语义：下载没有「原本地路径」，journal 用 `@url|` 哨兵记录来源；undo 时删除落点 = 撤销下载。
pub fn save_url(cfg: &Config, groups: &[(String, Vec<String>)], url: &str) -> IngestResult {
    let batch = format!("{}-url", now_ms());
    let mut moved = Vec::new();
    let mut failed = Vec::new();
    let mut entries = Vec::new();
    let lower = url.to_ascii_lowercase();
    let is_http = lower.starts_with("http://") || lower.starts_with("https://");
    if !is_http {
        failed.push(Failed { path: url.to_string(), reason: String::from("not an http(s) link") });
        return IngestResult { batch, moved, failed };
    }
    let clean: String = url.split('?').next().unwrap_or(url)
        .split('/').filter(|x| !x.is_empty()).last().unwrap_or("download")
        .chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*')).collect();
    let name = if clean.is_empty() || !clean.contains('.') { format!("download-{}.bin", now_ms() % 100000) } else { clean };
    let group = group_resolve(&PathBuf::from(&name), groups);
    let dest = unique_dest(&cfg.inbox_root.join(safe_group(&group)), &name);
    if let Some(p) = dest.parent() {
        if let Err(e) = std::fs::create_dir_all(p) {
            failed.push(Failed { path: url.to_string(), reason: format!("cannot create target dir: {e}") });
            return IngestResult { batch, moved, failed };
        }
    }
    let tmp = dest.with_extension("part");
    // 参数走 stdin 而非命令行拼接，规避全部引号转义问题
    let script = "$ProgressPreference='SilentlyContinue'; $u=[Console]::In.ReadLine(); $p=[Console]::In.ReadLine(); try { Invoke-WebRequest -Uri $u -OutFile $p -UseBasicParsing } catch { exit 3 }";
    let mut child = match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn() {
            Ok(c) => c,
            Err(e) => {
                failed.push(Failed { path: url.to_string(), reason: format!("spawn failed: {e}") });
                return IngestResult { batch, moved, failed };
            }
        };
    {
        use std::io::Write;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = writeln!(stdin, "{}", url);
            let _ = writeln!(stdin, "{}", tmp.to_string_lossy());
        }
    }
    // 120s 硬超时：黑洞 URL 不允许永久占住 handler 线程
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let ok_download = matches!(status, Some(s) if s.success());
    if !ok_download {
        let reason = if status.is_none() { "download timeout (120s), killed" } else { "download failed" };
        failed.push(Failed { path: url.to_string(), reason: reason.into() });
        let _ = std::fs::remove_file(&tmp);
        return IngestResult { batch, moved, failed };
    }
    if let Ok(meta) = std::fs::metadata(&tmp) {
        if meta.len() > 100 * 1024 * 1024 {
            let _ = std::fs::remove_file(&tmp);
            failed.push(Failed { path: url.to_string(), reason: String::from("over 100MB, skipped") });
            return IngestResult { batch, moved, failed };
        }
    }
    if let Err(e) = std::fs::rename(&tmp, &dest) {
        let _ = std::fs::remove_file(&tmp);
        failed.push(Failed { path: url.to_string(), reason: format!("move into inbox failed: {e}") });
        return IngestResult { batch, moved, failed };
    }
    let new = dest.to_string_lossy().to_string();
    entries.push(Entry {
        batch: batch.clone(),
        old: format!("@url|{url}"), // 哨兵：undo 时删除落点（撤销下载），而不是把文件搬到 URL
        new: new.clone(),
        ts: now_ms(),
    });
    if let Err(e) = append_entries(cfg, &entries) {
        failed.push(Failed {
            path: new,
            reason: format!("downloaded but journal write failed (file kept, cannot undo): {e}"),
        });
        return IngestResult { batch, moved, failed };
    }
    moved.push(Moved { old: url.to_string(), new, group });
    IngestResult { batch, moved, failed }
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
        let groups = cfg.groups.clone().unwrap_or_else(default_groups);
        let g = group_resolve(&src, &groups);
        let dest = unique_dest(&cfg.inbox_root.join(safe_group(&g)), &name);
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
        if !new_p.exists() {
            missing.push(e.new.clone());
            continue;
        }
        // URL 下载条目：没有「原本地路径」，复原 = 撤销下载（删除落点）
        if e.old.starts_with("@url|") {
            let r = if new_p.is_dir() { std::fs::remove_dir_all(new_p) } else { std::fs::remove_file(new_p) };
            match r {
                Ok(()) => restored += 1,
                Err(_) => missing.push(e.new.clone()),
            }
            continue;
        }
        let old_p = Path::new(&e.old);
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
            match move_any(new_p, &cand) {
                Ok(()) => { renamed += 1; restored += 1; }
                Err(_) => missing.push(e.new.clone()),
            }
        } else if move_any(new_p, old_p).is_ok() {
            restored += 1;
        } else {
            missing.push(e.new.clone());
        }
    }
    // 重写日志：去掉已回放的行（tmp → 备份旧日志 → 原子替换；失败回滚，绝不丢历史）
    let keep: Vec<Entry> = total.into_iter().filter(|e| !sel_batches.contains(&e.batch)).collect();
    if let Some(p) = cfg.journal.parent() {
        let _ = fs::create_dir_all(p);
    }
    let tmp = cfg.journal.with_extension("jsonl.tmp");
    let bak = cfg.journal.with_extension("jsonl.bak");
    let body: String = keep.iter().filter_map(|e| serde_json::to_string(e).ok()).map(|s| format!("{s}\n")).collect();
    if fs::write(&tmp, body).is_ok() {
        if cfg.journal.exists() {
            let _ = fs::rename(&cfg.journal, &bak);
        }
        if fs::rename(&tmp, &cfg.journal).is_err() {
            // 第二步失败：把旧日志放回原位（已物理复原的文件最多被重复报告，不会丢历史）
            let _ = fs::rename(&bak, &cfg.journal);
        } else {
            let _ = fs::remove_file(&bak);
        }
    }
    let left: HashSet<String> = keep.iter().map(|e| e.batch.clone()).collect();
    UndoReport { restored, renamed, missing, batches_left: left.len() }
}

pub fn undo_last(cfg: &Config) -> UndoReport {
    let _g = journal_guard();
    let all = read_entries(cfg);
    let Some(last_batch) = all.last().map(|e| e.batch.clone()) else {
        return UndoReport { restored: 0, renamed: 0, missing: vec![], batches_left: 0 };
    };
    let sel: Vec<Entry> = all.iter().filter(|e| e.batch == last_batch).cloned().collect();
    undo_entries(cfg, sel, all)
}

pub fn undo_all(cfg: &Config) -> UndoReport {
    let _g = journal_guard();
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
                    // 服务端裁剪：每格最多回传 60 条路径（UI 本来也只显示 60 条）。
                    // 2 万收件箱文件曾造成每次刷新 ~2.2MB JSON 过 IPC。
                    if files.len() < 60 {
                        files.push(f.path().to_string_lossy().to_string());
                    }
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

// ================= v0.3 融合模块 =================

/// 领域扩展名映射（智能推荐底稿；覆盖不到的才建议新增格）
fn domain_table() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("图片", &["png","jpg","jpeg","gif","webp","bmp","ico","svg","tiff","heic","psd","raw"]),
        ("文档", &["pdf","doc","docx","xls","xlsx","ppt","pptx","txt","md","rtf","csv","epub","odt","pages"]),
        ("视频", &["mp4","mkv","avi","mov","wmv","flv","webm","m4v","ts"]),
        ("音频", &["mp3","wav","flac","aac","ogg","m4a","opus","wma"]),
        ("代码", &["js","ts","tsx","jsx","json","rs","py","pyw","go","java","c","h","cpp","cs","kt","rb","sh","bat","ps1","sql","toml","yaml","yml","xml","html","css","vue","svelte","dart","php","lua","asm","pas","gradle"]),
        ("安装包", &["exe","msi","appx","msix","msp","apk","ipa"]),
        ("压缩包", &["zip","rar","7z","tar","gz","bz2","xz","iso","cab","lzh","tgz"]),
        ("CAD图纸", &["dwg","dxf","step","stp","iges","igs"]),
        ("字体", &["ttf","otf","woff","woff2","fon"]),
        ("密钥与证书", &["pfx","key","pem","crt","cert","pub","priv","ssh","asc"]),
        ("日志与备份", &["log","bak","tmp","dump","err","old"]),
    ]
}

#[derive(Serialize, Clone, Debug)]
pub struct DeskItem { pub path: String, pub name: String, pub size: u64, pub is_dir: bool, pub group: String }

pub fn desk_items(cfg: &Config, groups: &[(String, Vec<String>)]) -> Vec<DeskItem> {
    let mut out = Vec::new();
    let rd = |dir: &Path| -> Option<Vec<std::fs::DirEntry>> { std::fs::read_dir(dir).ok().map(|r| r.filter_map(|e| e.ok()).collect()) };
    if let Some(es) = rd(&cfg.desktop) {
        for e in es {
            let p = e.path();
            let is_dir = p.is_dir();
            let size = if is_dir { 0 } else { e.metadata().map(|m| m.len()).unwrap_or(0) };
            out.push(DeskItem {
                path: p.to_string_lossy().to_string(),
                name: e.file_name().to_string_lossy().to_string(),
                size, is_dir,
                group: group_resolve(&p, groups),
            });
        }
    }
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out
}

#[derive(Serialize, Clone, Debug)]
pub struct ExtStat { pub ext: String, pub count: usize }
#[derive(Serialize, Clone, Debug)]
pub struct GroupRule2 { pub name: String, pub exts: Vec<String> }
#[derive(Serialize, Clone, Debug)]
pub struct SuggestPlan {
    pub stats: Vec<ExtStat>,
    pub suggested: Vec<GroupRule2>,
    pub reason: String,
}


/// 智能推荐：扫描桌面散落文件的扩展名分布，建议当前体系未覆盖、且达到阈值的领域格。
pub fn suggest_plan(cfg: &Config, threshold: usize) -> SuggestPlan {
    // histogram from desktop top-level files + inbox top-level files
    let mut hist: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let scan_dir = |dir: &Path, hist: &mut std::collections::BTreeMap<String, usize>| {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
                        *hist.entry(ext.to_ascii_lowercase()).or_insert(0) += 1;
                    }
                }
            }
        }
    };
    scan_dir(&cfg.desktop, &mut hist);
    scan_dir(&cfg.inbox_root, &mut hist);
    let stats: Vec<ExtStat> = hist.into_iter().map(|(e, c)| ExtStat { ext: e, count: c }).collect();

    // 当前体系已覆盖的扩展名集合
    let covered: std::collections::HashSet<String> = cfg.groups.clone().unwrap_or_else(default_groups)
        .iter()
        .flat_map(|(_, es)| es.iter().cloned())
        .collect();
    let known_names: std::collections::HashSet<String> =
        cfg.groups.clone().unwrap_or_else(default_groups).iter().map(|(n, _)| n.clone()).collect();

    let tbl = domain_table();
    let mut sugg: Vec<GroupRule2> = Vec::new();
    for (dom, exts) in &tbl {
        if known_names.contains(*dom) {
            continue; // 体系里已有同名格，无需推荐
        }
        let mut miss = Vec::new();
        let mut total = 0;
        for e in exts.iter() {
            if !covered.contains(*e) {
                if let Some(c) = stats.iter().find(|s| s.ext == *e) {
                    total += c.count;
                    miss.push((*e).to_string());
                }
            }
        }
        if total >= threshold && !miss.is_empty() {
            sugg.push(GroupRule2 { name: dom.to_string(), exts: miss });
        }
    }
    SuggestPlan {
        suggested: sugg.clone(),
        stats,
        reason: if sugg.is_empty() { "当前体系已覆盖全部扩展名分布".into() } else { format!("发现 {} 个未覆盖且达 {} 份的领域，建议新增格", sugg.len(), threshold) },
    }
}

// ---- journal 批次管理（勾选复原）----
#[derive(Serialize, Clone, Debug)]
pub struct BatchInfo { pub batch: String, pub ts: u64, pub count: usize, pub groups: Vec<(String, usize)>, pub files: Vec<String> }

pub fn list_batches(cfg: &Config) -> Vec<BatchInfo> {
    let all = read_entries(cfg);
    let mut map: std::collections::HashMap<String, (u64, std::collections::HashMap<String, usize>, Vec<String>)> = std::collections::HashMap::new();
    for e in &all {
        let entry = map.entry(e.batch.clone()).or_insert((e.ts, std::collections::HashMap::new(), Vec::new()));
        entry.0 = entry.0.max(e.ts);
        // 从落点路径反推真实分组（收件箱/<组>/...），而不是恒写「归档」
        let g = std::path::Path::new(&e.new)
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "归档".into());
        *entry.1.entry(g).or_insert(0) += 1;
        if entry.2.len() < 200 {
            entry.2.push(e.new.clone());
        }
    }
    let mut out: Vec<BatchInfo> = map.into_iter()
        .map(|(batch, (ts, groups, files))| BatchInfo { batch, ts, count: groups.values().sum(), groups: groups.into_iter().collect(), files })
        .collect();
    out.sort_by(|a, b| b.ts.cmp(&a.ts));
    out
}

pub fn undo_batches(cfg: &Config, sel: Vec<String>) -> UndoReport {
    let _g = journal_guard();
    let all = read_entries(cfg);
    let sel_set: std::collections::HashSet<String> = sel.into_iter().collect();
    let chosen: Vec<Entry> = all.iter().filter(|e| sel_set.contains(&e.batch)).cloned().collect();
    if chosen.is_empty() {
        return UndoReport { restored: 0, renamed: 0, missing: vec![], batches_left: 0 };
    }
    undo_entries(cfg, chosen, all)
}

// ---- 查重（Czkawka 思路：体积分组 + SHA-256）----
#[derive(Serialize, Clone, Debug)]
pub struct DupSet { pub sha: String, pub size: u64, pub count: usize, pub paths: Vec<String>, pub wasted: u64 }

pub fn find_dups(root: &Path) -> Result<Vec<DupSet>, String> {
    fn collect_samesize(dir: &Path, sizes: &mut std::collections::HashMap<u64, Vec<String>>) -> Result<(), String> {
        let rd = std::fs::read_dir(dir).map_err(|e| format!("read_dir failed: {e}"))?;
        for e in rd {
            let e = e.map_err(|x| x.to_string())?;
            let p = e.path();
            // 只跳过查重归档区本身；`_notes.txt`、`_backup/` 等合法命名照常参与
            if p.file_name().and_then(|n| n.to_str()).map(|n| n == "_重复待清理").unwrap_or(false) {
                continue;
            }
            if p.is_dir() {
                collect_samesize(&p, sizes)?;
            } else if p.is_file() {
                let sz = e.metadata().map_err(|x| x.to_string())?.len();
                sizes.entry(sz).or_default().push(p.to_string_lossy().to_string());
            }
        }
        Ok(())
    }

    let mut sizes: std::collections::HashMap<u64, Vec<String>> = std::collections::HashMap::new();
    collect_samesize(root, &mut sizes)?;

    let mut sets: Vec<DupSet> = Vec::new();
    for (_sz, paths) in sizes {
        if paths.len() < 2 { continue; }
        let mut by_sha: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for p in &paths {
            // 流式 64KB 缓冲，避免等大文件（视频/镜像）整读入内存
            let digest = sha256_of(Path::new(p)).ok_or_else(|| format!("hash failed: {p}"))?;
            by_sha.entry(digest).or_default().push(p.clone());
        }
        for (sha, ps) in by_sha {
            if ps.len() < 2 { continue; }
            let wasted = ps.len().saturating_sub(1) as u64 * _sz;

            sets.push(DupSet { sha: sha.chars().take(12).collect(), size: _sz, count: ps.len(), paths: ps, wasted });
        }
    }
    sets.sort_by(|a, b| b.wasted.cmp(&a.wasted));
    Ok(sets)
}

/// 归档重复项：非最新版移入 收件箱/_重复待清理/<sha12>/，同样过 journal（可一键复原）。
pub fn archive_dups(cfg: &Config, sets: &[DupSet], keep_newest: bool) -> IngestResult {
    let batch = format!("{}-dup", now_ms());
    let mut moved = Vec::new();
    let mut failed = Vec::new();
    let mut entries = Vec::new();
    for s in sets {
        let targets: Vec<String> = if keep_newest {
            let mut sorted = s.paths.clone();
            sorted.sort_by(|a, b| {
                let ta = std::fs::metadata(a).and_then(|m| m.modified()).ok();
                let tb = std::fs::metadata(b).and_then(|m| m.modified()).ok();
                tb.cmp(&ta)
            });
            sorted.into_iter().skip(1).collect()
        } else {
            s.paths.clone()
        };
        for p in targets {
            let src = PathBuf::from(&p);
            if !src.exists() {
                failed.push(Failed { path: p.clone(), reason: "文件已被删除或移动".into() });
                continue;
            }
            // 允许在收件箱内部移动到 _重复待清理（同样过 journal）；已在 _重复待清理 的不重复归档
            let dup_dir = cfg.inbox_root.join("_重复待清理");
            if src.starts_with(&dup_dir) { continue; }

            let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let dest = unique_dest(&cfg.inbox_root.join("_重复待清理").join(&s.sha), &name);
            match move_any(&src, &dest) {
                Ok(()) => {
                    let new = dest.to_string_lossy().to_string();
                    entries.push(Entry { batch: batch.clone(), old: p.clone(), new: new.clone(), ts: now_ms() });
                    moved.push(Moved { old: p, new, group: "_重复待清理".into() });
                }
                Err(e) => failed.push(Failed { path: p, reason: e.to_string() }),
            }
        }
    }
    if !entries.is_empty() {
        let _ = append_entries(cfg, &entries);
    }
    IngestResult { batch, moved, failed }
}


// ---- YAML 导入导出（organize 常用子集）----
/// 组名安全化：禁止绝对路径 / `..` 穿越 / 冒号 / 控制字符；非法名回落「其他」。
pub fn safe_group(name: &str) -> String {
    let n = name.trim().trim_matches('"').trim_matches('\'').trim();
    let bad = n.is_empty()
        || n.chars().count() > 64
        || n.contains('/')
        || n.contains('\\')
        || n.contains(':')
        || n.contains("..")
        || n == "."
        || n.chars().any(|c| c.is_control());
    if bad { "其他".to_string() } else { n.to_string() }
}

fn clean_group_name(raw: &str) -> Option<String> {
    let n = raw.trim().trim_matches('"').trim_matches('\'').trim();
    if n.is_empty() || n.chars().count() > 64 { return None; }
    if n.chars().any(|c| c.is_control()) { return None; }
    if n.contains('/') || n.contains('\\') || n.contains(':') || n.contains("..") || n == "." { return None; }
    Some(n.to_string())
}

fn clean_ext_token(raw: &str) -> Option<String> {
    let e = raw.trim().trim_matches('"').trim_matches('\'').trim().trim_start_matches('.');
    if e.is_empty() || e.len() > 16 { return None; }
    if e.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
        Some(e.to_ascii_lowercase())
    } else {
        None
    }
}

/// 去掉行内注释：仅当 `#` 位于引号/方括号之外且前面是空白（URL 里的 # 不受影响）。
fn strip_inline_comment(l: &str) -> &str {
    let mut in_q = false;
    let mut depth = 0i32;
    let mut cut = l.len();
    for (i, ch) in l.char_indices() {
        match ch {
            '"' | '\'' => in_q = !in_q,
            '[' if !in_q => depth += 1,
            ']' if !in_q => depth -= 1,
            '#' if !in_q && depth <= 0 => {
                let before = &l[..i];
                if before.is_empty() || before.ends_with(' ') || before.ends_with('\t') {
                    cut = i;
                    break;
                }
            }
            _ => {}
        }
    }
    l[..cut].trim_end()
}

/// 导出：NestDesk 子集格式，可直接复制回 organize CLI 的 rules
pub fn export_yaml(groups: &[(String, Vec<String>)]) -> String {
    let mut out = String::from("groups:\n");
    for (name, exts) in groups {
        out.push_str(&format!("  - name: {}\n    exts: [{}]\n", name, exts.join(", ")));
    }
    out
}

/// 导入：容忍 NestDesk 子集与 organize CLI 两种布局的捕捉器
/// organize CLI 样式：rules: [ { folder: X, extensions: [png] }, ... ] 或 - createfolder: X / extensions  [png]
/// 与 export_yaml 严格互逆：`- name:` + `exts:` 必须能导回。
pub fn import_yaml(text: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    let flush = |cur: &mut Option<(String, Vec<String>)>, out: &mut Vec<(String, Vec<String>)>| {
        if let Some((name, mut exts)) = cur.take() {
            exts.sort();
            exts.dedup();
            if !exts.is_empty() {
                out.push((name, exts));
            }
        }
    };
    for raw in text.lines() {
        let l = strip_inline_comment(raw.trim());
        if l.is_empty() || l.starts_with('#') { continue; }
        // 文件夹名锚点：兼容 export 自身（`- name:`）与 organize（`- folder:` / `- createfolder:` / 裸 `name:`）
        let anchor = |l: &str| -> Option<String> {
            for prefix in ["- createfolder:", "- create_folder:", "- folder:", "- name:", "name:"] {
                if let Some(rest) = l.strip_prefix(prefix) {
                    return clean_group_name(rest);
                }
            }
            None
        };
        if let Some(name) = anchor(l) {
            flush(&mut cur, &mut out);
            cur = Some((name, Vec::new()));
            continue;
        }
        // extensions / exts 两种键（export 用 exts）
        if let Some(arr) = l.strip_prefix("extensions:").or_else(|| l.strip_prefix("exts:")) {
            let body = arr.trim();
            if body.starts_with('[') && body.ends_with(']') {
                let inner = body.trim_start_matches('[').trim_end_matches(']');
                for tok in inner.split(|c| c == ',' || c == ' ') {
                    if let Some(e) = clean_ext_token(tok) {
                        if let Some(g) = cur.as_mut() { g.1.push(e); }
                    }
                }
            }
            continue;
        }
        // 单项列表组件 - extension: png / - png
        if let Some(item) = l.strip_prefix("- ") {
            if let Some(e) = item.strip_prefix("extension:").or_else(|| item.strip_prefix("ext:")) {
                if let Some(x) = clean_ext_token(e) {
                    if let Some(g) = cur.as_mut() { g.1.push(x); }
                }
            } else if !item.contains(':') && !item.starts_with('#') && !item.starts_with('-') {
                if let Some(x) = clean_ext_token(item) {
                    if let Some(g) = cur.as_mut() { g.1.push(x); }
                }
            }
        }
    }
    flush(&mut cur, &mut out);
    if out.is_empty() {
        return Err("未解析出任何合法归纳文件夹规则（需 name/folder 锚点 + 至少一个扩展名；组名不允许包含 / \\ : .. 或控制字符）".into());
    }
    Ok(out)
}
