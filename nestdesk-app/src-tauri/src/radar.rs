// radar.rs — 资讯雷达：零依赖 RSS/Atom + 通用 JSON API 抓取、关键词过滤、去重、本地存储
// 抓取走 PowerShell Invoke-WebRequest（复用 save_url 的成熟姿势：stdin 传参，无引号地狱）。
// 解析只做「够用」级别：RSS 2.0 / Atom 的 item/entry 标题+链接+时间+摘要。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FeedItem {
    pub title: String,
    pub link: String,
    pub summary: String,
    /// 源 URL（便于回溯）
    #[serde(default)]
    pub source: String,
    /// 抓取时间（ms）
    #[serde(default)]
    pub ts: u64,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------- 抓取 ----------

/// HTTP GET，返回正文（超时 + body 大小上限，防黑洞与超大响应）。
/// 走「写临时文件再读回」：`[Console]::OpenStandardOutput().Write(string)` 要 byte[]，
/// 直接传字符串会抛异常；文件路径也是 save_url 已验证可靠的姿势，且能避开控制台编码破坏。
pub fn http_get(url: &str, timeout_secs: u64, max_bytes: usize) -> Result<String, String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("only http(s) supported".into());
    }
    let tmp = std::env::temp_dir().join(format!("nestdesk-radar-{}-{}.tmp", std::process::id(), now_ms()));
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; $u=[Console]::In.ReadLine(); $p=[Console]::In.ReadLine(); try {{ Invoke-WebRequest -Uri $u -OutFile $p -UseBasicParsing -TimeoutSec {timeout_secs} }} catch {{ exit 3 }}"
    );
    let mut child = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = writeln!(stdin, "{url}");
        let _ = writeln!(stdin, "{}", tmp.to_string_lossy());
    }
    // 硬超时：timeout_secs + 10s 余量后 kill
    let deadline = Instant::now() + Duration::from_secs(timeout_secs + 10);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("wait failed: {e}"));
            }
        }
    };
    let ok = matches!(status, Some(s) if s.success());
    if !ok {
        let _ = std::fs::remove_file(&tmp);
        return Err(match status {
            None => "timeout".into(),
            Some(s) => format!("http request failed (exit {:?})", s.code()),
        });
    }
    let bytes = std::fs::read(&tmp).map_err(|e| format!("read body failed: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    if bytes.is_empty() {
        return Err("empty response".into());
    }
    let take = bytes.len().min(max_bytes);
    let body = String::from_utf8_lossy(&bytes[..take]).to_string();
    Ok(body)
}

// ---------- XML 解析（RSS / Atom，够用级） ----------

/// 取第一个 `<tag>...</tag>` 的内容（去 CDATA、解码常见实体、去标签）。
fn pick(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let mut from = 0usize;
    while let Some(i) = xml[from..].find(&open) {
        let start = from + i;
        // 跳过 <tag> 与 <tag attr="...">
        let after = &xml[start + open.len()..];
        let gt = after.find('>')?;
        let head = &after[..gt];
        // 只接受完全同名（避免 <titleXxx>）
        if !head.is_empty() && !head.starts_with(' ') && !head.starts_with('/') && !head.starts_with('>') {
            from = start + open.len();
            continue;
        }
        let self_closing = head.ends_with('/');
        if self_closing {
            return Some(String::new());
        }
        let content_start = start + open.len() + gt + 1;
        let close = format!("</{tag}>");
        let rest = &xml[content_start..];
        let end = rest.find(&close)?;
        return Some(decode(&rest[..end]));
    }
    None
}

/// 取块：所有 `<item>...</item>` 或 `<entry>...</entry>`。
fn blocks(xml: &str, tag: &str) -> Vec<String> {
    let close = format!("</{tag}>");
    let open = format!("<{tag}");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(i) = xml[from..].find(&open) {
        let start = from + i;
        let after = &xml[start + open.len()..];
        let Some(gt) = after.find('>') else { break };
        let content_start = start + open.len() + gt + 1;
        let rest = &xml[content_start..];
        let Some(end) = rest.find(&close) else { break };
        out.push(rest[..end].to_string());
        from = content_start + end + close.len();
    }
    out
}

/// 去 CDATA / 实体解码 / 去 HTML 标签 / 压空白。
/// 顺序很关键：先解实体再去标签，否则 `&lt;b&gt;` 解码后留下的标签会漏掉。
fn decode(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if let Some(inner) = s.strip_prefix("<![CDATA[").and_then(|x| x.strip_suffix("]]>")) {
        s = inner.to_string();
    }
    let s = s
        .replace("&lt;", "<").replace("&gt;", ">")
        .replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'")
        .replace("&nbsp;", " ").replace("&amp;", "&");
    // 去掉（解码后暴露的）HTML 标签
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 解析 RSS 2.0 或 Atom；返回条目（不含 source/ts）。
pub fn parse_feed(xml: &str) -> Vec<FeedItem> {
    let is_atom = xml.contains("<feed") || xml.contains("<entry");
    let (block_tag, link_mode) = if is_atom { ("entry", "atom") } else { ("item", "rss") };
    let mut out = Vec::new();
    for b in blocks(xml, block_tag) {
        let title = pick(&b, "title").unwrap_or_default();
        if title.is_empty() { continue; }
        let link = if link_mode == "atom" {
            // Atom: <link href="..."/> 或 <link rel="alternate" href="..."/>
            atom_link(&b).unwrap_or_default()
        } else {
            pick(&b, "link").unwrap_or_default()
        };
        let summary = pick(&b, "description")
            .or_else(|| pick(&b, "summary"))
            .or_else(|| pick(&b, "content"))
            .unwrap_or_default();
        out.push(FeedItem {
            title,
            link: link.trim().to_string(),
            summary: truncate(&summary, 300),
            source: String::new(),
            ts: 0,
        });
    }
    out
}

fn atom_link(block: &str) -> Option<String> {
    // 找 <link ... href="URL" ... />
    let mut from = 0usize;
    while let Some(i) = block[from..].find("<link") {
        let start = from + i;
        let after = &block[start + 5..];
        let gt = after.find('>')?;
        let head = &after[..gt];
        if let Some(h) = head.find("href=\"") {
            let rest = &head[h + 6..];
            if let Some(end) = rest.find('"') {
                let url = &rest[..end];
                if !url.is_empty() { return Some(url.to_string()); }
            }
        }
        from = start + 5 + gt;
    }
    None
}

fn truncate(s: &str, n: usize) -> String {
    let mut t: String = s.chars().take(n).collect();
    if s.chars().count() > n { t.push('…'); }
    t
}

// ---------- JSON API（通用：自动挑字段名） ----------

/// 从任意 JSON 响应里提取条目：自动识别数组所在键 + 常见字段名。
pub fn parse_json_feed(body: &str, source: &str) -> Vec<FeedItem> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else { return Vec::new() };
    let arr = find_array(&v);
    let Some(arr) = arr else { return Vec::new() };
    let mut out = Vec::new();
    for obj in arr.iter().take(200) {
        let title = first_str(obj, &["title", "name", "headline", "question", "subject"]).unwrap_or_default();
        if title.trim().is_empty() { continue; }
        let link = first_str(obj, &["link", "url", "html_url", "permalink", "web_url", "href"]).unwrap_or_default();
        let summary = first_str(obj, &["summary", "description", "abstract", "excerpt", "body", "text"]).unwrap_or_default();
        out.push(FeedItem {
            title: truncate(title.trim(), 200),
            link: link.trim().to_string(),
            summary: truncate(&summary, 300),
            source: source.to_string(),
            ts: now_ms(),
        });
    }
    out
}

/// 深度优先找「元素是对象且含 title/name 类字段」的最大数组。
fn find_array(v: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    fn score(a: &Vec<serde_json::Value>) -> usize {
        a.iter().filter(|x| {
            x.is_object() && ["title", "name", "headline", "question"].iter()
                .any(|k| x.get(*k).map(|y| y.is_string()).unwrap_or(false))
        }).count()
    }
    fn walk<'a>(v: &'a serde_json::Value, best: &mut Option<(&'a Vec<serde_json::Value>, usize)>) {
        match v {
            serde_json::Value::Array(a) => {
                let s = score(a);
                if s > 0 && best.map(|(_, bs)| s > bs).unwrap_or(true) {
                    *best = Some((a, s));
                }
                for x in a { walk(x, best); }
            }
            serde_json::Value::Object(m) => {
                for (_, x) in m { walk(x, best); }
            }
            _ => {}
        }
    }
    let mut best = None;
    walk(v, &mut best);
    best.map(|(a, _)| a)
}

fn first_str(v: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            if !s.is_empty() { return Some(s.to_string()); }
        }
    }
    None
}

// ---------- 关键词过滤 ----------

pub fn matches_keywords(item: &FeedItem, keywords: &[String]) -> bool {
    let kws: Vec<String> = keywords.iter().map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).collect();
    if kws.is_empty() { return true; }
    let hay = format!("{} {}", item.title, item.summary).to_lowercase();
    kws.iter().any(|k| hay.contains(k))
}

// ---------- 聚合与存储 ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RadarStore {
    pub items: Vec<FeedItem>,
    /// 上次抓取时间
    #[serde(default)]
    pub last_fetch: u64,
    /// 抓取错误（最近一次，供 UI 显示）
    #[serde(default)]
    pub last_error: String,
    /// 累计抓取成功次数
    #[serde(default)]
    pub fetch_count: u64,
}

const RADAR_MAX_ITEMS: usize = 200;

pub fn store_path(data_dir: &Path) -> PathBuf { data_dir.join("radar.json") }

pub fn load_store(path: &Path) -> RadarStore {
    let parse = |p: &Path| std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str::<RadarStore>(&s).ok());
    parse(path)
        .or_else(|| {
            // .bak 自愈，与 settings/layout 同策略
            let bak = path.with_extension("json.bak");
            if bak.exists() { parse(&bak) } else { None }
        })
        .unwrap_or_default()
}

pub fn save_store(path: &Path, store: &RadarStore) {
    let Ok(txt) = serde_json::to_string_pretty(store) else { return };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, txt).is_err() { return; }
    let bak = path.with_extension("json.bak");
    // 先保留旧内容（若存在），再原子替换
    if path.exists() {
        let _ = std::fs::copy(path, &bak);
    }
    if std::fs::rename(&tmp, path).is_err() { return; }
    // 首次保存也要有一份 .bak，否则主文件损坏时无从自愈
    if !bak.exists() {
        let _ = std::fs::copy(path, &bak);
    }
}

/// 一次完整抓取：拉所有 feed → 解析 → 关键词过滤 → 去重合并 → 落盘。
/// 返回本次**新增**条目数。
pub fn fetch_once(store_path: &Path, feeds: &[String], keywords: &[String]) -> Result<usize, String> {
    if feeds.is_empty() {
        return Err("未配置任何资讯源（RSS/Atom URL 或 JSON API URL）".into());
    }
    let mut store = load_store(store_path);
    let mut fresh: Vec<FeedItem> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for url in feeds.iter().take(20) {
        match http_get(url, 20, 2 * 1024 * 1024) {
            Ok(body) => {
                let trimmed = body.trim_start();
                let mut items = if trimmed.starts_with('{') || trimmed.starts_with('[') {
                    parse_json_feed(&body, url)
                } else {
                    let mut v = parse_feed(&body);
                    for it in &mut v {
                        it.source = url.clone();
                        it.ts = now_ms();
                    }
                    v
                };
                items.retain(|i| matches_keywords(i, keywords));
                fresh.extend(items);
            }
            Err(e) => errors.push(format!("{url}: {e}")),
        }
    }
    // 去重：link 优先，其次 title
    let mut seen: std::collections::HashSet<String> = store.items.iter().map(item_key).collect();
    let mut added = 0usize;
    for it in fresh {
        let k = item_key(&it);
        if seen.insert(k) {
            store.items.push(it);
            added += 1;
        }
    }
    // 新的在前 + 上限
    store.items.sort_by(|a, b| b.ts.cmp(&a.ts));
    store.items.truncate(RADAR_MAX_ITEMS);
    store.last_fetch = now_ms();
    store.last_error = errors.join(" | ");
    if errors.len() < feeds.len() || added > 0 {
        store.fetch_count += 1;
    }
    save_store(store_path, &store);
    if errors.len() == feeds.len() {
        return Err(store.last_error);
    }
    Ok(added)
}

fn item_key(i: &FeedItem) -> String {
    if !i.link.is_empty() { i.link.clone() } else { format!("title:{}", i.title) }
}

// ---------- 内置源（用户可直接勾选） ----------

pub fn builtin_feeds() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Hacker News 头条", "https://hnrss.org/frontpage"),
        ("GitHub 趋势（日）", "https://mshibanami.github.io/GitHubTrendingRSS/daily/all.xml"),
        ("阮一峰周刊", "https://feeds.feedburner.com/ruanyifeng"),
        ("少数派", "https://sspai.com/feed"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_parse_and_keywords() {
        let xml = r#"<rss><channel>
        <item><title>Rust 1.90 发布</title><link>https://a/1</link><description>编译器优化</description></item>
        <item><title>无关新闻</title><link>https://a/2</link><description>八卦</description></item>
        </channel></rss>"#;
        let items = parse_feed(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].link, "https://a/1");
        let kw = vec!["rust".to_string()];
        assert!(matches_keywords(&items[0], &kw));
        assert!(!matches_keywords(&items[1], &kw));
    }

    #[test]
    fn atom_parse() {
        let xml = r#"<feed><entry><title>Atom 标题</title><link rel="alternate" href="https://b/1"/><summary>摘要</summary></entry></feed>"#;
        let items = parse_feed(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].link, "https://b/1");
    }

    #[test]
    fn json_feed_autodetect() {
        let body = r#"{"data":{"list":[{"title":"T1","url":"https://c/1","description":"D"},{"title":"T2","html_url":"https://c/2"}]}}"#;
        let items = parse_json_feed(body, "https://api");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].link, "https://c/1");
        assert_eq!(items[1].link, "https://c/2");
    }

    #[test]
    fn cdata_and_entities() {
        let xml = r#"<rss><channel><item><title><![CDATA[Rust & C++]]></title><link>https://d/1</link><description>&lt;b&gt;加粗&lt;/b&gt; 与 &amp; 符号</description></item></channel></rss>"#;
        let items = parse_feed(xml);
        assert_eq!(items[0].title, "Rust & C++");
        assert_eq!(items[0].summary, "加粗 与 & 符号");
    }
}
