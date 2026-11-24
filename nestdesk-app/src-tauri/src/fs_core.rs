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
    let (stem, ext) = split_name(name);
    let mut cand = dir.join(name);
    let mut i = 2;
    while cand.exists() {
        cand = dir.join(format!("{stem} ({i}){ext}"));
        i += 1;
    }
    cand
}

fn sha256_of(path: &Path) -> Option<String> {
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
fn move_any(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(p) = dst.parent() {
        std::fs::create_dir_all(p)?;
    }
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let src_hash = sha256_of(src);
    if copy_tree(src, dst).is_err() {
        return Err(std::io::Error::other("copy failed"));
    }
    if let Some(ref want) = src_hash {
        if sha256_of(dst).as_deref() != Some(want.as_str()) {
            let _ = std::fs::remove_file(dst);
            return Err(std::io::Error::other("verify failed: hash mismatch, source kept"));
        }
    }
    let _ = if src.is_dir() { std::fs::remove_dir_all(src) } else { std::fs::remove_file(src) };
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

pub fn read_entries(cfg: &Config) -> Vec<Entry> {
    let Ok(data) = std::fs::read_to_string(&cfg.journal) else { return Vec::new(); };
    data.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

fn append_entries(cfg: &Config, entries: &[Entry]) -> std::io::Result<()> {
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
    let dest = unique_dest(&cfg.inbox_root.join(&group), &name);
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
    let ok_download = matches!(child.wait(), Ok(s) if s.success());
    if !ok_download {
        failed.push(Failed { path: url.to_string(), reason: String::from("download failed") });
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
    if std::fs::rename(&tmp, &dest).is_err() {
        let _ = std::fs::rename(&tmp, &dest);
    }
    let new = dest.to_string_lossy().to_string();
    entries.push(Entry { batch: batch.clone(), old: url.to_string(), new: new.clone(), ts: now_ms() });
    moved.push(Moved { old: url.to_string(), new, group });
    let _ = append_entries(cfg, &entries);
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
        let dest = unique_dest(&cfg.inbox_root.join(&g), &name);
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

// ================= v0.3 融合模块 =================
use std::time::UNIX_EPOCH as _U;

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
    let mut scan_dir = |dir: &Path, hist: &mut std::collections::BTreeMap<String, usize>| {
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
pub struct BatchInfo { pub batch: String, pub ts: u64, pub count: usize, pub groups: Vec<(String, usize)> }

pub fn list_batches(cfg: &Config) -> Vec<BatchInfo> {
    let all = read_entries(cfg);
    let mut map: std::collections::HashMap<String, (u64, std::collections::HashMap<String, usize>)> = std::collections::HashMap::new();
    for e in &all {
        let entry = map.entry(e.batch.clone()).or_insert((e.ts, std::collections::HashMap::new()));
        entry.0 = entry.0.max(e.ts);
        *entry.1.entry(String::from("归档")).or_insert(0) += 1;
    }
    let mut out: Vec<BatchInfo> = map.into_iter()
        .map(|(batch, (ts, groups))| BatchInfo { batch, ts, count: groups.values().sum(), groups: groups.into_iter().collect() })
        .collect();
    out.sort_by(|a, b| b.ts.cmp(&a.ts));
    out
}

pub fn undo_batches(cfg: &Config, sel: Vec<String>) -> UndoReport {
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
    use sha2::{Digest, Sha256};

    fn collect_samesize(dir: &Path, sizes: &mut std::collections::HashMap<u64, Vec<String>>) -> Result<(), String> {
        let rd = std::fs::read_dir(dir).map_err(|e| format!("read_dir failed: {e}"))?;
        for e in rd {
            let e = e.map_err(|x| x.to_string())?;
            let p = e.path();
            if p.file_name().and_then(|n| n.to_str()).map(|n| n.starts_with('_')).unwrap_or(false) {
                continue; // skip _dup folders
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
            let h = std::fs::read(p).map_err(|e| e.to_string())?; // 大文件也硬读，内存换取确定性
            let mut hasher = Sha256::new();
            hasher.update(h.as_slice());
            let digest = format!("{:x}", hasher.finalize());
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
/// 导出：NestDesk 子集格式，可直接复制回 organize CLI 的 rules
pub fn export_yaml(groups: &[(String, Vec<String>)]) -> String {
    let mut out = String::from("groups:
");
    for (name, exts) in groups {
        out.push_str(&format!("  - name: {}
    exts: [{}]
", name, exts.join(", ")));
    }
    out
}

/// 导入：容忍 NestDesk 子集与 organize CLI 两种布局的捕捉器
/// organize CLI 样式：rules: [ { folder: X, extensions: [png] }, ... ] 或 - createfolder: X / extensions  [png]
pub fn import_yaml(text: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    let mut flush = |cur: &mut Option<(String, Vec<String>)>, out: &mut Vec<_>| {
        if let Some((name, mut exts)) = cur.take() {
            exts.sort();
            exts.dedup();
            if !exts.is_empty() {
                out.push((name, exts));
            }
        }
    };
    for raw in text.lines() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with('#') { continue; }
        // 文件夹名锚点
        let anchor = |l: &str| -> Option<String> {
            if let Some(rest) = l.strip_prefix("- createfolder:").or_else(|| l.strip_prefix("- create_folder:")).or_else(|| l.strip_prefix("- folder:")) {
                Some(rest.trim().trim_matches('"').to_string())
            } else if let Some(rest) = l.strip_prefix("name:") {
                Some(rest.trim().trim_matches('"').to_string())
            } else { None }
        };
        if let Some(name) = anchor(l) {
            flush(&mut cur, &mut out);
            cur = Some((name, Vec::new()));
            continue;
        }
        // extensions: [png jpg] 或 extensions:
        if let Some(arr) = l.strip_prefix("extensions:") {
            let body = arr.trim();
            if body.starts_with('[') && body.ends_with(']') {
                let inner = body.trim_matches(['[', ']']);
                let exts: Vec<String> = inner.split(|c| c == ',' || c == ' ')
                    .map(|x| x.trim().trim_matches('"').to_string())
                    .filter(|x| !x.is_empty() && !x.contains(':') )
                    .map(|x| x.to_ascii_lowercase())
                    .collect();
                if !exts.is_empty() {
                    if let Some(g) = cur.as_mut() { g.1.extend(exts); }
                }
            }
            continue;
        }
        // 单项列表组件 - extension: png / - png
        if l.starts_with("- ") {
            let item = &l[2..];
            if let Some(e) = item.strip_prefix("extension:").or_else(|| item.strip_prefix("ext:")) {
                let e = e.trim().trim_matches('"').to_string();
                if !e.is_empty() && !e.contains(':') {
                    if let Some(g) = cur.as_mut() { g.1.push(e.to_ascii_lowercase()); }
                }
            } else if !item.contains(':') && !item.starts_with('#') && !item.starts_with('-') {
                let e = item.trim_matches('"');
                if !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
                    if let Some(g) = cur.as_mut() { g.1.push(e.to_string()); }
                }
            }
        }
    }
    flush(&mut cur, &mut out);
    if out.is_empty() {
        return Err("未解析出任何归纳文件夹规则".into());
    }
    Ok(out)
}
