// disk.rs — 磁盘空间可视化后端（WinDirStat 范式：扫描树 + 扩展名分类 + Top-N）
// 只读分析，从不删除/移动任何文件；结果供前端画 squarified treemap。

use serde::Serialize;
use std::path::{Path, PathBuf};

/// 单个节点的扫描结果。为了控制 IPC 体积，只回传「直接子项」，
/// 下钻时前端再按需请求（path 参数）。
#[derive(Serialize, Clone, Debug)]
pub struct DiskNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// 该子树内文件数（目录才有意义）
    pub files: u64,
    /// 占比（相对于本次请求的根）
    pub pct: f64,
    /// 主扩展名（文件才有）
    pub ext: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct DiskScan {
    pub root: String,
    pub total: u64,
    pub total_files: u64,
    /// 直接子项，按大小降序
    pub children: Vec<DiskNode>,
    /// 扩展名聚合（Top 20）
    pub ext_stats: Vec<ExtAgg>,
    /// 扫描是否被限额截断
    pub truncated: bool,
    /// 扫描耗时（ms）
    pub elapsed_ms: u64,
    /// 跳过的目录数（噪音目录）
    pub skipped_dirs: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct ExtAgg {
    pub ext: String,
    pub size: u64,
    pub count: u64,
    pub pct: f64,
}

/// 噪音目录：开发产物/缓存，扫它们既慢又没意义。
fn is_noise_dir(name: &str) -> bool {
    const NOISE: [&str; 14] = [
        "node_modules", ".git", "target", "dist", ".next", ".nuxt", "__pycache__",
        ".venv", "venv", ".cache", "obj", "bin", ".idea", ".vs",
    ];
    NOISE.iter().any(|n| n.eq_ignore_ascii_case(name))
}

struct Budget {
    nodes: u64,
    max_nodes: u64,
    deadline: std::time::Instant,
    max_ms: u64,
    truncated: bool,
    skipped_dirs: u64,
}

impl Budget {
    fn exhausted(&mut self) -> bool {
        if self.truncated { return true; }
        let over_nodes = self.nodes >= self.max_nodes;
        let over_time = self.deadline.elapsed().as_millis() as u64 > self.max_ms;
        if over_nodes || over_time { self.truncated = true; }
        self.truncated
    }
}

/// 递归统计一个路径的总大小与文件数。
/// `ext` 是全局扩展名聚合；`local` 是当前顶层子项自己的聚合（用于算「主导类型」）。
fn walk(
    p: &Path,
    budget: &mut Budget,
    ext: &mut std::collections::HashMap<String, (u64, u64)>,
    local: &mut std::collections::HashMap<String, (u64, u64)>,
) -> (u64, u64) {
    if budget.exhausted() { return (0, 0); }
    let meta = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(_) => return (0, 0),
    };
    if meta.is_file() {
        budget.nodes += 1;
        let size = meta.len();
        let e = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        let key = if e.is_empty() { "无扩展名".to_string() } else { e };
        for m in [&mut *ext, &mut *local] {
            let slot = m.entry(key.clone()).or_insert((0, 0));
            slot.0 += size;
            slot.1 += 1;
        }
        return (size, 1);
    }
    if !meta.is_dir() { return (0, 0); }
    if p.file_name().and_then(|n| n.to_str()).map(is_noise_dir).unwrap_or(false) {
        budget.skipped_dirs += 1;
        return (0, 0);
    }
    let mut size = 0u64;
    let mut files = 0u64;
    let Ok(rd) = std::fs::read_dir(p) else { return (0, 0) };
    for e in rd.flatten() {
        if budget.exhausted() { break; }
        let (s, f) = walk(&e.path(), budget, ext, local);
        size += s;
        files += f;
    }
    (size, files)
}

/// 主导扩展名：按体积最大的那一类（目录用它决定颜色，取代"所有目录同色"）。
fn dominant_ext(local: &std::collections::HashMap<String, (u64, u64)>) -> String {
    local.iter()
        .max_by_key(|(_, (size, _))| *size)
        .map(|(k, _)| k.clone())
        .unwrap_or_default()
}

/// 扫描一个目录，返回其直接子项的大小排名 + 扩展名聚合。
/// `max_nodes` / `max_ms` 用于硬限额，超出即标记 truncated（前端提示"部分结果"）。
pub fn scan_dir(root: &Path, max_nodes: u64, max_ms: u64) -> DiskScan {
    let started = std::time::Instant::now();
    let mut budget = Budget {
        nodes: 0,
        max_nodes,
        deadline: started,
        max_ms,
        truncated: false,
        skipped_dirs: 0,
    };
    let mut ext: std::collections::HashMap<String, (u64, u64)> = std::collections::HashMap::new();
    let mut children: Vec<DiskNode> = Vec::new();
    let mut total = 0u64;
    let mut total_files = 0u64;

    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            if budget.exhausted() { break; }
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = p.is_dir();
            if is_dir && is_noise_dir(&name) {
                budget.skipped_dirs += 1;
                continue;
            }
            let mut local: std::collections::HashMap<String, (u64, u64)> = std::collections::HashMap::new();
            let (size, files) = walk(&p, &mut budget, &mut ext, &mut local);
            // 目录用「主导扩展名」着色；文件用自己的扩展名
            let de = if is_dir {
                dominant_ext(&local)
            } else {
                p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase()
            };
            total += size;
            total_files += files;
            children.push(DiskNode {
                ext: de,
                name,
                path: p.to_string_lossy().to_string(),
                is_dir,
                size,
                files,
                pct: 0.0,
            });
        }
    }

    children.sort_by(|a, b| b.size.cmp(&a.size));
    let denom = total.max(1) as f64;
    for c in &mut children {
        c.pct = (c.size as f64 / denom) * 100.0;
    }

    let mut ext_stats: Vec<ExtAgg> = ext
        .into_iter()
        .map(|(ext, (size, count))| ExtAgg { ext, size, count, pct: (size as f64 / denom) * 100.0 })
        .collect();
    ext_stats.sort_by(|a, b| b.size.cmp(&a.size));
    ext_stats.truncate(20);

    DiskScan {
        root: root.to_string_lossy().to_string(),
        total,
        total_files,
        children,
        ext_stats,
        truncated: budget.truncated,
        elapsed_ms: started.elapsed().as_millis() as u64,
        skipped_dirs: budget.skipped_dirs,
    }
}

/// 磁盘容量（由 layer 提供，此处只做类型定义便于共享）
#[derive(Serialize, Clone, Debug)]
pub struct DriveSpace {
    pub drive: String,
    pub total: u64,
    pub free: u64,
    pub used: u64,
    pub pct: f64,
}

/// 把路径所在盘符的容量信息整理成 DriveSpace。
pub fn drive_space_of(path: &Path) -> Option<DriveSpace> {
    let (total, free) = crate::layer::disk_free(path)?;
    let used = total.saturating_sub(free);
    Some(DriveSpace {
        drive: path.components().next()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_default(),
        total,
        free,
        used,
        pct: if total == 0 { 0.0 } else { (used as f64 / total as f64) * 100.0 },
    })
}

/// 列出常规可用盘符（A:..Z: 中存在的）
pub fn list_drives() -> Vec<String> {
    let mut out = Vec::new();
    for c in b'A'..=b'Z' {
        let root = format!("{}:\\", c as char);
        let p = PathBuf::from(&root);
        if p.exists() { out.push(root); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("nestdesk-disk-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn scan_sizes_and_ext_stats() {
        let root = tmpdir("scan");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), vec![b'x'; 100]).unwrap();
        std::fs::write(root.join("b.log"), vec![b'y'; 200]).unwrap();
        std::fs::write(root.join("sub/c.txt"), vec![b'z'; 300]).unwrap();
        let s = scan_dir(&root, 100_000, 5_000);
        assert_eq!(s.total, 600);
        assert_eq!(s.total_files, 3);
        // 直接子项：sub(300) > b.log(200) > a.txt(100)
        assert_eq!(s.children.len(), 3);
        assert_eq!(s.children[0].name, "sub");
        assert_eq!(s.children[0].size, 300);
        // txt 汇总 400 / 2 个
        let txt = s.ext_stats.iter().find(|e| e.ext == "txt").unwrap();
        assert_eq!(txt.size, 400);
        assert_eq!(txt.count, 2);
        assert!(!s.truncated);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn noise_dirs_are_skipped() {
        let root = tmpdir("noise");
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::write(root.join("node_modules/pkg/big.js"), vec![b'x'; 9999]).unwrap();
        std::fs::write(root.join("real.txt"), vec![b'y'; 10]).unwrap();
        let s = scan_dir(&root, 100_000, 5_000);
        assert_eq!(s.total, 10, "node_modules 必须被跳过");
        assert_eq!(s.skipped_dirs, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn node_budget_truncates() {
        let root = tmpdir("budget");
        for i in 0..200 {
            std::fs::write(root.join(format!("f{i}.bin")), vec![b'x'; 10]).unwrap();
        }
        let s = scan_dir(&root, 50, 5_000);
        assert!(s.truncated, "节点预算应触发 truncated");
        let _ = std::fs::remove_dir_all(&root);
    }
}
