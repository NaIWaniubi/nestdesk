import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { invoke } from "@tauri-apps/api/core";
import { squarify, extColor } from "./treemap";

// ---------- 皮肤 ----------
const THEMES = ["glass", "glassdark", "night", "aurora", "cyber", "sunset", "contrast", "mist"] as const;
type Theme = (typeof THEMES)[number];

function useTheme(): [Theme, () => void] {
  const [theme, setTheme] = useState<Theme>("night");
  const apply = (t: string) => {
    const th = (THEMES.includes(t as Theme) ? t : "night") as Theme;
    document.documentElement.dataset.theme = th;
    setTheme(th);
  };
  useEffect(() => {
    invoke<string>("theme_get").then(apply);
  }, []);
  const cycle = () => {
    const next = THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length];
    apply(next);
    invoke("theme_set", { theme: next });
  };
  return [theme, cycle];
}

// ---------- 工具 ----------
const pad = (n: number) => String(n).padStart(2, "0");
function fmtBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  if (b < 1048576) return `${(b / 1024).toFixed(1)} KB`;
  if (b < 1073741824) return `${(b / 1048576).toFixed(1)} MB`;
  return `${(b / 1073741824).toFixed(2)} GB`;
}
const GROUP_ICON: Record<string, string> = {
  图片: "🖼️", 文档: "📄", 压缩包: "🗜️", 安装包: "📦",
  视频: "🎬", 音频: "🎵", 代码: "💻", 文件夹: "📁", 其他: "❓",
};

// ---------- 时钟 ----------
function Clock() {
  const [, cycle] = useTheme();
  const [now, setNow] = useState(new Date());
  const [lunar, setLunar] = useState("");
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);
  // Glance：每天一次走 .NET 原生日历取农历（DeskBox Glance 同款思路，零数据风险）
  useEffect(() => {
    invoke<string>("lunar_now").then(setLunar).catch(() => {});
    const t = setInterval(() => invoke<string>("lunar_now").then(setLunar).catch(() => {}), 3600_000);
    return () => clearInterval(t);
  }, []);
  const wd = ["日", "一", "二", "三", "四", "五", "六"][now.getDay()];
  return (
    <div className="card grab">
      <div className="head"><b>时钟</b><span className="sp" />
        <span className="badge nodrag" onClick={cycle} title="换肤">◐</span>
      </div>
      <div className="clock-body">
        <div className="clock-time">{pad(now.getHours())}:{pad(now.getMinutes())}<span style={{ fontSize: 20 }}>:{pad(now.getSeconds())}</span></div>
        <div className="clock-date">{now.getFullYear()}–{pad(now.getMonth() + 1)}–{pad(now.getDate())} 周{wd}</div>
        {lunar && <div className="clock-date" style={{ color: "var(--accent)" }}>{lunar}</div>}
      </div>
    </div>
  );
}

// ---------- 系统监视（v0.7：btop/bottom 范式 — 时序曲线 + 环形磁盘） ----------
interface SysInfo {
  cpu: number; mem_total: number; mem_used: number;
  battery: { percent: number; ac: boolean; charging: boolean } | null;
  disk: { drive: string; total: number; free: number; used: number; pct: number } | null;
}

/// 60 点历史 → SVG sparkline（渐变填充 + 发光描边）
function Spark({ data, color, max = 100 }: { data: number[]; color: string; max?: number }) {
  const W = 100, H = 26;
  if (data.length < 2) return <div style={{ height: H }} />;
  const step = W / Math.max(data.length - 1, 1);
  const pts = data.map((v, i) => `${(i * step).toFixed(2)},${(H - Math.min(v, max) / max * (H - 2) - 1).toFixed(2)}`);
  const line = `M${pts.join(" L")}`;
  const area = `${line} L${W},${H} L0,${H} Z`;
  const gid = `g${color.replace(/[^a-z0-9]/gi, "")}${data.length}`;
  return (
    <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" style={{ width: "100%", height: H, display: "block", overflow: "visible" }}>
      <defs>
        <linearGradient id={gid} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity="0.55" />
          <stop offset="100%" stopColor={color} stopOpacity="0.02" />
        </linearGradient>
      </defs>
      <path d={area} fill={`url(#${gid})`} />
      <path d={line} fill="none" stroke={color} strokeWidth="1.2" strokeLinejoin="round" vectorEffect="non-scaling-stroke"
        style={{ filter: `drop-shadow(0 0 2px ${color})` }} />
    </svg>
  );
}

/// 环形进度（用于磁盘 / 电池）
function Ring({ pct, label, sub, color }: { pct: number; label: string; sub: string; color: string }) {
  const r = 16, c = 2 * Math.PI * r;
  const v = Math.max(0, Math.min(100, pct));
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <svg width="42" height="42" viewBox="0 0 42 42" style={{ flex: "none" }}>
        <circle cx="21" cy="21" r={r} fill="none" stroke="var(--bar-bg)" strokeWidth="4" />
        <circle cx="21" cy="21" r={r} fill="none" stroke={color} strokeWidth="4" strokeLinecap="round"
          strokeDasharray={`${(v / 100) * c} ${c}`} transform="rotate(-90 21 21)"
          style={{ transition: "stroke-dasharray .6s ease-out", filter: `drop-shadow(0 0 3px ${color})` }} />
        <text x="21" y="24" textAnchor="middle" style={{ fontSize: 9, fill: "var(--text)", fontWeight: 600 }}>{Math.round(v)}</text>
      </svg>
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 11, color: "var(--text)" }}>{label}</div>
        <div style={{ fontSize: 9, color: "var(--dim)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{sub}</div>
      </div>
    </div>
  );
}

function Sysmon() {
  const [info, setInfo] = useState<SysInfo | null>(null);
  const [cpuHist, setCpuHist] = useState<number[]>([]);
  const [memHist, setMemHist] = useState<number[]>([]);
  const [, cycle] = useTheme();
  useEffect(() => {
    let alive = true;
    const tick = () => invoke<SysInfo>("get_sysinfo").then((s) => {
      if (!alive) return;
      setInfo(s);
      // 前端保留 60 点（2s × 60 = 2 分钟趋势），后端不存历史
      const cpu = s.cpu >= 0 ? s.cpu * 100 : 0;
      const mem = s.mem_total ? (s.mem_used / s.mem_total) * 100 : 0;
      setCpuHist((h) => [...h, cpu].slice(-60));
      setMemHist((h) => [...h, mem].slice(-60));
    }).catch(() => {});
    tick();
    const t = setInterval(tick, 2000);
    return () => { alive = false; clearInterval(t); };
  }, []);
  const cpu = info && info.cpu >= 0 ? Math.round(info.cpu * 100) : 0;
  const mem = info && info.mem_total ? Math.round((info.mem_used / info.mem_total) * 100) : 0;
  const bat = info?.battery;
  const dsk = info?.disk;
  const cpuColor = cpu > 85 ? "#f97362" : cpu > 60 ? "#f5a524" : "var(--accent)";
  const memColor = mem > 88 ? "#f97362" : mem > 70 ? "#f5a524" : "#56c596";
  return (
    <div className="card grab">
      <div className="head"><b>系统</b><span className="sp" />
        <span className="badge nodrag" onClick={cycle} title="换肤">◐</span>
      </div>
      <div className="mon-rows" style={{ gap: 7 }}>
        <div className="mon-row">
          <div className="lbl"><span>CPU</span><span style={{ color: cpuColor }}>{cpu}%</span></div>
          <Spark data={cpuHist} color={cpuColor} />
        </div>
        <div className="mon-row">
          <div className="lbl"><span>内存</span><span style={{ color: memColor }}>{info ? fmtBytes(info.mem_used) : "--"} · {mem}%</span></div>
          <Spark data={memHist} color={memColor} />
        </div>
        <div style={{ display: "flex", gap: 10, alignItems: "center", marginTop: 2 }}>
          {dsk && <Ring pct={dsk.pct} color={dsk.pct > 90 ? "#f97362" : "var(--accent)"}
            label={`${dsk.drive} 磁盘`} sub={`剩 ${fmtBytes(dsk.free)} / ${fmtBytes(dsk.total)}`} />}
          {bat && <Ring pct={bat.percent} color={bat.charging ? "#56c596" : bat.percent < 20 ? "#f97362" : "#f5a524"}
            label={bat.charging ? "充电中" : bat.ac ? "已插电" : "电池"} sub={`${bat.percent}%`} />}
        </div>
        <div className="chip" style={{ fontSize: 9, paddingTop: 0 }}>
          {info ? `${fmtBytes(info.mem_total)} RAM` : ""}{bat ? "" : " · 台式机"}
        </div>
      </div>
    </div>
  );
}

// ---------- 便利贴 ----------
function Note() {
  const [text, setText] = useState("");
  const [saved, setSaved] = useState(false);
  const [, cycle] = useTheme();
  const timer = useRef<number>(0);
  useEffect(() => { invoke<string>("note_get").then(setText); }, []);
  const onChange = (v: string) => {
    setText(v);
    setSaved(false);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      invoke("note_set", { text: v }).then(() => setSaved(true));
    }, 500);
  };
  return (
    <div className="card grab">
      <div className="head"><b>便利贴</b><span className="sp" />
        <span className="note-saved">{saved ? "✓ 已保存" : ""}</span>
        <span className="badge nodrag" onClick={cycle} title="换肤">◐</span>
      </div>
      <div className="note-body">
        <textarea className="nodrag" placeholder="写点什么…（自动保存，重启不丢）" value={text} onChange={(e) => onChange(e.target.value)} />
      </div>
    </div>
  );
}

// ---------- 收纳框 ----------
interface GroupStat { group: string; count: number; bytes: number; files: string[] }
interface Snapshot {
  desktop_files: number;
  desktop: { name: string; size: number; is_dir: boolean }[];
  inbox: GroupStat[];
  inbox_total: number;
  journal_len: number;
  journal_bytes: number;
}
interface IngestResult { batch: string; moved: { old: string; new: string; group: string }[]; failed: { path: string; reason: string }[] }
interface UndoReport { restored: number; renamed: number; missing: string[]; batches_left: number }

function Shelf() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [toast, setToast] = useState("");
  const [hot, setHot] = useState(false);
  const [armed, setArmed] = useState(false);
  const armTimer = useRef<number>(0);
  const [, cycle] = useTheme();
  const refresh = () => invoke<Snapshot>("shelf_snapshot").then(setSnap).catch(() => {});
  useEffect(() => {
    refresh();
    let un: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "over" || e.payload.type === "enter") setHot(true);
        else setHot(false);
        if (e.payload.type === "drop") {
          const paths = e.payload.paths;
          setToast("归档中…");
          invoke<IngestResult>("shelf_ingest", { paths }).then((r) => {
            setToast(r.failed.length
              ? `已归档 ${r.moved.length}，${r.failed.length} 个失败`
              : `已归档 ${r.moved.length} 个文件 → ${[...new Set(r.moved.map((m) => m.group))].join("/")}`);
            refresh();
          });
        }
      })
      .then((f) => (un = f));
    return () => un?.();
  }, []);

  // S-B: 浏览器里把链接拖到收纳框 → 下载归档（DeskBox 多来源拖放）
  const handleUrlDrop = async (e: React.DragEvent) => {
    const urls: string[] = [];
    const raw = e.dataTransfer.getData("text/uri-list") || e.dataTransfer.getData("text/plain");
    for (const line of raw.split(/\r?\n/)) {
      const u = line.trim();
      if (/^https?:\/\//i.test(u)) urls.push(u);
    }
    if (!urls.length) return;
    e.preventDefault();
    setToast("下载归档中…");
    let okN = 0, badN = 0;
    for (const u of urls) {
      try {
        const r = await invoke<IngestResult>("shelf_save_url", { url: u });
        okN += r.moved.length; badN += r.failed.length;
      } catch { badN++; }
    }
    setToast(`链接已下载 ${okN} 个` + (badN ? `，${badN} 个失败` : ""));
    refresh();
  };

  const act = async (name: string, fn: () => Promise<unknown>, ok: (r: unknown) => string) => {
    setToast(name + "…");
    try {
      const r = await fn();
      setToast(ok(r));
      refresh();
    } catch {
      setToast(name + "失败");
    }
  };
  const undoRep = (r: unknown) => {
    const x = r as UndoReport;
    return `复原 ${x.restored}${x.renamed ? `（${x.renamed} 个原址被占→挪旁）` : ""}${x.missing.length ? `，${x.missing.length} 个已被删除找不回` : ""}`;
  };

  return (
    <div className={"card" + (hot ? " drop-hot" : "")}
      onDragOver={(e) => { e.preventDefault(); }}
      onDrop={handleUrlDrop}>
      <div className="head grab">
        <b>🏠 归巢 · 桌面收件箱</b><span className="sp" />
        <span className="badge nodrag" onClick={cycle} title="换肤">◐</span>
      </div>
      <div className="head" style={{ paddingTop: 0 }}>
        <span className="btn nodrag" onClick={() => act("收取桌面散落文件", () => invoke<IngestResult>("shelf_collect_desktop"), (r) => {
          const x = r as IngestResult;
          return `收取 ${x.moved.length} 个文件`;
        })}>⚡ 收取桌面</span>
        <span className="btn nodrag" onClick={() => act("复原上一批", () => invoke("shelf_undo_last"), undoRep)}>↩ 复原上批</span>
        <span className="btn warn nodrag" onClick={() => {
          if (!armed) {
            setArmed(true);
            window.clearTimeout(armTimer.current);
            armTimer.current = window.setTimeout(() => setArmed(false), 3000);
            setToast("再点一次将把所有归档文件放回原位");
          } else {
            setArmed(false);
            act("全部复原", () => invoke("shelf_undo_all"), undoRep);
          }
        }}>{armed ? "⚠ 确认全部复原?" : "⎌ 全部复原"}</span>
      </div>
      <div className="shelf-body nodrag">
        {!snap && <div className="empty">载入中…</div>}
        {snap && snap.inbox.length === 0 && (
          <div className="empty">
            桌面空空如也，收纳框也干干净净 ✨<br />
            把文件拖进来，或点「⚡ 收取桌面」——<br />
            自动按 图片/文档/安装包… 分格归档，随时一键复原。
          </div>
        )}
        {snap?.inbox.map((g) => (
          <details className="group" key={g.group}>
            <summary>{GROUP_ICON[g.group]} {g.group}<span className="n">{g.count} 个 · {fmtBytes(g.bytes)}</span></summary>
            <div className="files">
              {g.files.slice(0, 60).map((p) => {
                const name = p.split(/[\\/]/).pop() || p;
                return (
                  <span key={p} className="nodrag" title={`${p}\n双击打开 · 右键定位`}
                    onDoubleClick={() => invoke("open_path", { path: p }).catch(() => setToast("打开失败，文件可能已移动"))}
                    onContextMenu={(e) => { e.preventDefault(); invoke("reveal_path", { path: p }).catch(() => {}); }}
                    style={{ display: "block", padding: "1px 0", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {name}
                  </span>
                );
              })}
              {g.files.length > 60 ? ` …共 ${g.files.length}` : ""}
            </div>
          </details>
        ))}
      </div>
      <div className="toast">{toast}</div>
      <div className="foot">
        <span>桌面散落 {snap?.desktop_files ?? "--"}</span>
        <span>已归档 {snap?.inbox_total ?? "--"}</span>
        <span>journal {snap?.journal_len ?? "--"} 条 / {snap ? fmtBytes(snap.journal_bytes) : "--"}</span>
        <span className="sp" style={{ flex: 1 }} />
        <span>Nest/收件箱</span>
      </div>
    </div>
  );
}

// ---------- 控制台设置 ----------
interface GroupRule { name: string; exts: string[] }
interface AppSettings { groups: GroupRule[]; alpha: Record<string, number> }
const ALPHA_KEYS = [
  ["shelf", "收纳框"], ["clock", "时钟"], ["sysmon", "系统监视"], ["note", "便利贴"], ["todo", "待办"],
] as const;

interface SuggestPlanT {
  stats: { ext: string; count: number }[];
  suggested: { name: string; exts: string[] }[];
  reason: string;
}

function Settings() {
  type Tab = "桌面" | "归纳" | "事务" | "查重" | "雷达" | "空间" | "更多";
  const [tab, setTab] = useState<Tab>("桌面");
  const [groups, setGroups] = useState<GroupRule[]>([]);
  const [alpha, setAlpha] = useState<Record<string, number>>({});
  const [saved, setSaved] = useState("");
  const [msg, setMsg] = useState("");

  // ---- 桌面勾选 ----
  interface DeskItemT { path: string; name: string; size: number; is_dir: boolean; group: string }
  const [desk, setDesk] = useState<DeskItemT[]>([]);
  const [pick, setPick] = useState<Record<number, boolean>>({});
  const [showAllDesk, setShowAllDesk] = useState(false);
  const DESK_LIMIT = 120;
  const deskShown = showAllDesk ? desk : desk.slice(0, DESK_LIMIT);
  const loadDesk = () => invoke<DeskItemT[]>("shelf_desk_items").then(setDesk).catch(() => {});
  useEffect(() => { loadDesk(); }, []);
  const collectPicked = async () => {
    const paths = desk.filter((_, i) => pick[i]).map((d) => d.path);
    if (!paths.length) { setMsg("勾选至少一个"); return; }
    const r = await invoke<IngestResult>("shelf_ingest", { paths });
    setSaved(`已归档 ${r.moved.length} 个${r.failed.length ? `，${r.failed.length} 个失败` : ""}`);
    setPick({}); loadDesk();
  };

  // ---- 智能推荐 / YAML ----
  interface SuggestT { name: string; exts: string[] }
  const [sugg, setSugg] = useState<SuggestT[]>([]);
  const suggest = async () => {
    const p = await invoke<SuggestPlanT>("plan_suggest");
    setMsg(p.reason || "");
    setSugg(p.suggested);
  };
  const applySugg = async () => {
    if (!sugg.length) { setMsg("先点「✨ 智能推荐方案」生成建议"); return; }
    // 先取后端最新设置再合并：冷启动 settings_get 未返回时点应用，不会把用户分组表覆盖成空
    const cur = await invoke<AppSettings>("settings_get");
    const merged = [...cur.groups];
    for (const s of sugg) if (!merged.some((g) => g.name === s.name)) merged.push({ name: s.name, exts: s.exts });
    const r = await invoke<AppSettings>("settings_save", { groups: merged, alpha: cur.alpha });
    setGroups(r.groups); setAlpha(r.alpha); setSugg([]); setSaved(`应用推荐：${sugg.length} 个新归纳文件夹`);
  };
  const doYamlExport = async () => { const t = await invoke<string>("yaml_export"); downloadText("nestdesk-groups.yaml", t); setMsg("已导出 yaml"); };
  const doYamlImport = async () => {
    const f = prompt("粘贴 organize/NestDesk 兼容的 yml 规则文本："); if (!f) return;
    try {
      const ns = await invoke<AppSettings>("yaml_import", { text: f });
      setGroups(ns.groups); setAlpha(ns.alpha); setSaved(`导入 ${ns.groups.length} 个归纳文件夹`);
    } catch (e) {
      setMsg(`导入失败：${String(e)}`);
    }
  };

  // ---- 事务批次 ----
  interface BatchT { batch: string; ts: number; count: number; groups: [string, number][]; files: string[] }
  const [batches, setBatches] = useState<BatchT[]>([]);
  const [bpick, setBPick] = useState<Record<number, boolean>>({});
  const loadB = () => invoke<BatchT[]>("shelf_batches").then(setBatches).catch(() => {});
  useEffect(() => { loadB(); }, []);
  const undoSel = async () => {
    const sel = batches.filter((_, i) => bpick[i]).map((b) => b.batch);
    if (!sel.length) { setMsg("勾选至少一批"); return; }
    const r = await invoke<UndoReport>("shelf_undo_selected", { batches: sel });
    setSaved(`复原 ${r.restored}${r.renamed ? `（${r.renamed} 挪旁）` : ""}${r.missing.length ? `，${r.missing.length} 缺失` : ""}`);
    setBPick({}); loadB(); loadDesk();
  };

  // ---- 查重 ----
  interface DupT { sha: string; size: number; count: number; paths: string[]; wasted: number }
  const [dups, setDups] = useState<DupT[]>([]);
  const [keepNew, setKeepNew] = useState(true);
  const [dsha, setDSha] = useState<Record<number, boolean>>({});
  const scanDups = async () => {
    try { const res = await invoke<DupT[]>("dup_scan"); setDups(res); setMsg(`扫描完成：${res.length} 组重复，可省 ${fmtBytes(res.reduce((a, b) => a + b.wasted, 0))}`); }
    catch { setMsg("扫描失败（收件箱可能不存在，先收取桌面再扫）"); }
  };
  const archDups = async () => {
    const shas = dups.filter((_, i) => dsha[i]).map((d) => d.sha);
    if (!shas.length) { setMsg("勾选至少一组"); return; }
    const r = await invoke<IngestResult>("dup_archive", { shas, keepNewest: keepNew });
    setSaved(`归档重复 ${r.moved.length} 个 → _重复待清理${r.failed.length ? `，失败 ${r.failed.length}` : ""}（可一键复原）`);
    setDSha({}); scanDups(); loadDesk();
  };

  useEffect(() => { invoke<AppSettings>("settings_get").then((s) => { setGroups(s.groups); setAlpha(s.alpha); }); }, []);

  const save = async () => {
    const clean = groups.map((g) => ({ ...g, name: g.name.trim() })).filter((g) => g.name);
    if (!clean.length) { setSaved("至少要保留一个归纳文件夹"); return; }
    const bad = clean.filter((g) => /[\/\\:]/.test(g.name) || g.name.includes("..") || g.name === ".");
    if (bad.length) { setSaved(`组名不能包含 / \\ : 或 ..：${bad.map((b) => b.name).join("、")}`); return; }
    if (!clean.some((g) => g.name === "其他")) clean.push({ name: "其他", exts: [] });
    try {
      const r = await invoke<AppSettings>("settings_save", { groups: clean, alpha });
      setGroups(r.groups); setAlpha(r.alpha); setSaved(`✓ 已保存：${r.groups.length} 个归纳文件夹`);
    } catch (e) {
      setSaved(`保存失败：${String(e)}`);
    }
  };

  // ---- 音乐宿主（S-E）：启动 Folia 或配置的播放器 ----
  const [player, setPlayer] = useState("");
  const openMusic = async () => {
    try { setMsg(await invoke<string>("music_open")); }
    catch (e) { setMsg(String(e)); }
  };
  const savePlayer = async () => {
    if (!player.trim()) return;
    setSaved(await invoke<string>("music_set", { path: player.trim() }));
  };

  // ---- v0.7 磁盘空间（WinDirStat 范式）----
  interface DiskNodeT { name: string; path: string; is_dir: boolean; size: number; files: number; pct: number; ext: string }
  interface DiskScanT { root: string; total: number; total_files: number; children: DiskNodeT[]; ext_stats: { ext: string; size: number; count: number; pct: number }[]; truncated: boolean; elapsed_ms: number; skipped_dirs: number }
  interface DriveT { drive: string; total: number; free: number; used: number; pct: number }
  const [ds, setDs] = useState<DiskScanT | null>(null);
  const [dpath, setDpath] = useState<string>("");
  const [droot, setDRoot] = useState<string>("");
  const [trail, setTrail] = useState<{ name: string; path: string }[]>([]);
  const [dsel, setDSel] = useState<DiskNodeT | null>(null);
  const [extFilter, setExtFilter] = useState<string>("");
  const [drives, setDrives] = useState<DriveT[]>([]);
  useEffect(() => {
    invoke<string>("disk_root").then((r) => { setDRoot(r); setDpath(r); }).catch(() => {});
    invoke<DriveT[]>("disk_drives").then(setDrives).catch(() => {});
  }, []);
  const scanDisk = async (p?: string) => {
    const target = p ?? dpath ?? droot;
    if (!target) return;
    setBusy(true);
    try {
      const r = await invoke<DiskScanT>("disk_scan", { path: target });
      setDs(r); setDpath(r.root); setDSel(null);
    } catch (e) { setMsg(`扫描失败：${String(e).slice(0, 120)}`); }
    finally { setBusy(false); }
  };
  useEffect(() => { if (droot) scanDisk(droot); }, [droot]);
  const drill = async (n: DiskNodeT) => {
    if (!n.is_dir) { setDSel(n); return; }
    setTrail((t) => [...t, { name: n.name, path: n.path }]);
    await scanDisk(n.path);
  };
  const goUp = async (idx: number) => {
    // idx = -1 表示回到根
    const next = idx < 0 ? droot : trail[idx].path;
    setTrail((t) => t.slice(0, idx + 1));
    await scanDisk(next);
  };

  // ---- v0.6 桌面监听 ----
  const [watch, setWatch] = useState(false);
  const [watchStat, setWatchStat] = useState<{ enabled: boolean; last_tick: number; last_moved: number }>({ enabled: false, last_tick: 0, last_moved: 0 });
  useEffect(() => {
    invoke<boolean>("watcher_get").then(setWatch).catch(() => {});
    const t = window.setInterval(() => { invoke<typeof watchStat>("watcher_status").then(setWatchStat).catch(() => {}); }, 3000);
    return () => window.clearInterval(t);
  }, []);
  const toggleWatch = async () => {
    const on = await invoke<boolean>("watcher_set", { on: !watch });
    setWatch(on);
    setSaved(on ? "已开启桌面自动归档（新文件落盘后自动进格）" : "已关闭桌面自动归档");
  };

  // ---- v0.6 资讯雷达 ----
  interface RadarItemT { title: string; link: string; summary: string; source: string; ts: number }
  interface RadarStoreT { items: RadarItemT[]; last_fetch: number; last_error: string; fetch_count: number }
  const [rCfg, setRCfg] = useState<{ enabled: boolean; interval_min: number; feeds: string[]; keywords: string[] }>({ enabled: false, interval_min: 60, feeds: [], keywords: [] });
  const [rStore, setRStore] = useState<RadarStoreT>({ items: [], last_fetch: 0, last_error: "", fetch_count: 0 });
  const [rFeedInput, setRFeedInput] = useState("");
  const [rKwInput, setRKwInput] = useState("");
  const [busy, setBusy] = useState(false);
  const loadRadar = () => {
    invoke<typeof rCfg>("radar_cfg").then((c) => { setRCfg(c); setRKwInput(c.keywords.join(", ")); }).catch(() => {});
    invoke<RadarStoreT>("radar_items").then(setRStore).catch(() => {});
  };
  useEffect(() => { loadRadar(); }, []);
  const saveRadar = async (patch: Partial<typeof rCfg>) => {
    const next = { ...rCfg, ...patch, keywords: rKwInput.split(/[,，\s]+/).map((k) => k.trim()).filter(Boolean) };
    const saved2 = await invoke<typeof rCfg>("radar_set", { cfg: next });
    setRCfg(saved2); setRKwInput(saved2.keywords.join(", "));
    setSaved(`雷达配置已保存：${saved2.feeds.length} 源 · ${saved2.keywords.length} 关键词 · ${saved2.enabled ? "定时开" : "定时关"}`);
  };
  const addFeed = async (url: string) => {
    const u = url.trim(); if (!u) return;
    if (rCfg.feeds.includes(u)) { setMsg("该源已存在"); return; }
    const next = [...rCfg.feeds, u];
    const saved2 = await invoke<typeof rCfg>("radar_set", { cfg: { ...rCfg, feeds: next, keywords: rKwInput.split(/[,，\s]+/).filter(Boolean) } });
    setRCfg(saved2); setRFeedInput("");
  };
  const delFeed = async (url: string) => {
    const saved2 = await invoke<typeof rCfg>("radar_set", { cfg: { ...rCfg, feeds: rCfg.feeds.filter((f) => f !== url), keywords: rKwInput.split(/[,，\s]+/).filter(Boolean) } });
    setRCfg(saved2);
  };
  const addBuiltin = async () => {
    const list = await invoke<[string, string][]>("radar_builtin");
    const next = [...rCfg.feeds];
    for (const [, u] of list) if (!next.includes(u)) next.push(u);
    const saved2 = await invoke<typeof rCfg>("radar_set", { cfg: { ...rCfg, feeds: next, keywords: rKwInput.split(/[,，\s]+/).filter(Boolean) } });
    setRCfg(saved2); setSaved(`已添加 ${list.length} 个内置源`);
  };
  const refreshRadar = async () => {
    setBusy(true);
    try {
      const n = await invoke<number>("radar_refresh");
      setSaved(`抓取完成：新增 ${n} 条`);
    } catch (e) { setMsg(`抓取失败：${String(e).slice(0, 160)}`); }
    finally { setBusy(false); loadRadar(); }
  };
  const toTodo = async (it: RadarItemT) => {
    await invoke("radar_to_todo", { title: it.title, link: it.link });
    setSaved("已加入待办");
  };

  const tabs: { k: Tab; label: string }[] = [
    { k: "桌面", label: "桌面" }, { k: "归纳", label: "归纳" }, { k: "事务", label: "事务" },
    { k: "查重", label: "查重" }, { k: "雷达", label: "雷达" }, { k: "空间", label: "空间" }, { k: "更多", label: "更多" },
  ];
  const updateExts = (i: number, raw: string) => setGroups((gs) => gs.map((g, j) => (j === i ? { ...g, exts: raw.split(/[,，\s]+/).map((e) => e.trim().toLowerCase()).filter(Boolean) } : g)));

  return (
    <div className="card" style={{ overflow: "hidden", display: "flex", flexDirection: "column", height: "100vh" }}>
      <div style={{ display: "flex", borderBottom: "1px solid var(--border)", background: "var(--bar-bg)" }}>
        {tabs.map((t) => (
          <span key={t.k} onClick={() => setTab(t.k)} style={{
            padding: "9px 14px", fontSize: 12, cursor: "pointer",
            color: tab === t.k ? "var(--accent)" : "var(--dim)",
            borderBottom: tab === t.k ? "2px solid var(--accent)" : "2px solid transparent",
          }}>{t.label}</span>
        ))}
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: "12px 14px" }}>
        {tab === "桌面" && (
          <>
            <div style={{ fontSize: 12, color: "var(--dim)", marginBottom: 6 }}>勾选要收取进归纳体系的文件（按系统推荐归类）：</div>
            {desk.length === 0 && <div style={{ fontSize: 11, color: "var(--dim)" }}>载入中…或桌面暂无文件</div>}
            {deskShown.map((d, i) => (
              <label key={d.path} style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 11, padding: "3px 0" }}>
                <input type="checkbox" checked={!!pick[i]} onChange={(e) => setPick((p) => ({ ...p, [i]: e.target.checked }))} />
                <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }} title={d.path}>{d.name}{d.is_dir ? "/" : ""}</span>
                <span style={{ color: "var(--dim)" }}>{fmtBytes(d.size)}</span>
                <span style={{ color: "var(--accent)", minWidth: 56, textAlign: "right" }}>{d.group}</span>
              </label>
            ))}
            {desk.length > DESK_LIMIT && (
              <div style={{ fontSize: 11, color: "var(--dim)", marginTop: 4 }}>
                仅显示前 {DESK_LIMIT} / 共 {desk.length} 项
                <span className="btn nodrag" style={{ marginLeft: 8 }} onClick={() => setShowAllDesk((v) => !v)}>
                  {showAllDesk ? "收起" : "显示全部"}
                </span>
              </div>
            )}
            <div style={{ marginTop: 10 }}><span className="btn nodrag" onClick={collectPicked}>⬇ 收取勾选的 {Object.values(pick).filter(Boolean).length} 个</span></div>
          </>
        )}

        {tab === "归纳" && (
          <>
            {groups.map((g, i) => (
              <div key={i} style={{ display: "flex", gap: 6, marginBottom: 6, alignItems: "center" }}>
                <input style={{ flex: 1, background: "var(--bar-bg)", border: "1px solid var(--border)", color: "var(--text)", borderRadius: 8, padding: "6px 9px", fontSize: 12 }}
                  value={g.name} onChange={(e) => setGroups((gs) => gs.map((h, j) => (j === i ? { ...h, name: e.target.value } : h)))} placeholder="文件夹名" />
                <input style={{ flex: 2, background: "var(--bar-bg)", border: "1px solid var(--border)", color: "var(--text)", borderRadius: 8, padding: "6px 9px", fontSize: 12 }}
                  value={g.exts.join(" ")} onChange={(e) => updateExts(i, e.target.value)} placeholder="扩展名" />
                <span className="btn nodrag" onClick={() => setGroups((gs) => (gs.length <= 1 ? gs : gs.filter((_, j) => j !== i)))}>✕</span>
              </div>
            ))}
            <div style={{ marginTop: 8, display: "flex", gap: 8, flexWrap: "wrap" }}>
              <span className="btn nodrag" onClick={() => setGroups((gs) => [...gs, { name: "", exts: [] }])}>＋ 加格</span>
              <span className="btn nodrag" onClick={suggest}>✨ 智能推荐方案</span>
              <span className="btn nodrag" onClick={applySugg} style={sugg.length ? {} : { opacity: 0.5 }}>应用推荐 ({sugg.length})</span>
              <span className="btn nodrag" onClick={doYamlExport}>导出 yml</span>
              <span className="btn nodrag" onClick={doYamlImport}>导入 yml</span>
              <span className="btn nodrag" onClick={save} style={{ padding: "6px 16px" }}>💾 保存归纳设置</span>
            </div>
            {sugg.length > 0 && (
              <div style={{ marginTop: 8, fontSize: 11, color: "var(--dim)" }}>推荐新增：
                {sugg.map((s, i) => <span key={i} style={{ display: "inline-block", margin: "2px 4px 2px 0", padding: "2px 8px", border: "1px solid var(--border)", borderRadius: 8 }}>{s.name} · {s.exts.join("/")}</span>)}
              </div>
            )}
          </>
        )}

        {tab === "事务" && (
          <>
            <div style={{ fontSize: 12, color: "var(--dim)", marginBottom: 6 }}>事务日志按批记录，勾选要回滚的批次：</div>
            {batches.length === 0 && <div style={{ fontSize: 11, color: "var(--dim)" }}>暂无事务记录</div>}
            {batches.map((b, i) => (
              <details key={b.batch} style={{ borderBottom: "1px solid var(--border)", padding: "2px 0" }}>
                <summary style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 11, padding: "4px 0", cursor: "pointer" }}>
                  <input type="checkbox" checked={!!bpick[i]} onClick={(e) => e.stopPropagation()} onChange={(e) => setBPick((p) => ({ ...p, [i]: e.target.checked }))} />
                  <span style={{ flex: 1 }}>{new Date(b.ts).toLocaleString()} · {b.count} 个文件 · {b.groups.map((g) => `${g[0]}(${g[1]})`).join("、")}</span>
                  <span style={{ color: "var(--dim)" }}>{b.batch.slice(-6)}</span>
                </summary>
                <div style={{ fontSize: 10, color: "var(--dim)", padding: "2px 0 6px 24px", whiteSpace: "pre-wrap" }}>
                  {b.files.slice(0, 20).map((p) => p.split(/[\\/]/).pop()).join("、")}
                  {b.files.length > 20 ? ` …共 ${b.count} 个` : ""}
                </div>
              </details>
            ))}
            <div style={{ marginTop: 10, display: "flex", gap: 8 }}>
              <span className="btn nodrag" onClick={undoSel}>↩ 复原勾选批次</span>
              <span className="btn nodrag" onClick={loadB}>↻ 刷新</span>
            </div>
          </>
        )}

        {tab === "查重" && (
          <>
            <div style={{ display: "flex", gap: 8, marginBottom: 10, alignItems: "center" }}>
              <span className="btn nodrag" onClick={scanDups}>🔍 扫描收件箱</span>
              <label style={{ fontSize: 11 }}><input type="checkbox" checked={keepNew} onChange={(e) => setKeepNew(e.target.checked)} /> 每组保留最新版</label>
            </div>
            {dups.length === 0 && msg && <div style={{ fontSize: 11, color: "var(--dim)" }}>{msg}</div>}
            {dups.map((d, i) => (
              <div key={d.sha} style={{ marginBottom: 8, border: "1px solid var(--border)", borderRadius: 8, padding: "6px 8px" }}>
                <label style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 11 }}>
                  <input type="checkbox" defaultChecked onChange={(e) => setDSha((s) => ({ ...s, [i]: e.target.checked }))} />
                  <span style={{ flex: 1 }}>{d.count}× {fmtBytes(d.size)} · 可省 {fmtBytes(d.wasted)} · {d.sha}</span>
                </label>
                <div style={{ fontSize: 10, color: "var(--dim)", marginTop: 3, paddingLeft: 22, whiteSpace: "pre-wrap" }}>{d.paths.join("\n")}</div>
              </div>
            ))}
            {dups.length > 0 && <div style={{ marginTop: 6 }}><span className="btn warn nodrag" onClick={archDups}>🗑 归档勾选组的非最新版 → _重复待清理</span></div>}
          </>
        )}

        {tab === "雷达" && (
          <>
            <div style={{ fontSize: 12, color: "var(--dim)", marginBottom: 8 }}>资讯雷达：自接 RSS/Atom 或 JSON API，按关键词定时抓取。</div>

            <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 8, flexWrap: "wrap" }}>
              <label style={{ fontSize: 11 }}><input type="checkbox" checked={rCfg.enabled} onChange={(e) => saveRadar({ enabled: e.target.checked })} /> 定时抓取</label>
              <span style={{ fontSize: 11, color: "var(--dim)" }}>每</span>
              <input type="number" min={10} max={1440} value={rCfg.interval_min}
                onChange={(e) => setRCfg((c) => ({ ...c, interval_min: Number(e.target.value) }))}
                onBlur={() => saveRadar({ interval_min: rCfg.interval_min })}
                style={{ width: 64, background: "var(--bar-bg)", border: "1px solid var(--border)", color: "var(--text)", borderRadius: 6, padding: "3px 6px", fontSize: 11 }} />
              <span style={{ fontSize: 11, color: "var(--dim)" }}>分钟</span>
              <span className="sp" style={{ flex: 1 }} />
              <span className="btn nodrag" onClick={refreshRadar} style={busy ? { opacity: 0.5 } : {}}>{busy ? "抓取中…" : "⟳ 立即抓取"}</span>
            </div>

            <div style={{ fontSize: 11, color: "var(--dim)", margin: "10px 0 4px" }}>关键词（逗号分隔；空 = 全收）</div>
            <input value={rKwInput} onChange={(e) => setRKwInput(e.target.value)} onBlur={() => saveRadar({})}
              placeholder="Rust, AI, 开源, 实习"
              style={{ width: "100%", boxSizing: "border-box", background: "var(--bar-bg)", border: "1px solid var(--border)", color: "var(--text)", borderRadius: 8, padding: "6px 9px", fontSize: 12, marginBottom: 10 }} />

            <div style={{ fontSize: 11, color: "var(--dim)", margin: "4px 0" }}>资讯源（{rCfg.feeds.length}）</div>
            <div style={{ display: "flex", gap: 6, marginBottom: 6 }}>
              <input value={rFeedInput} onChange={(e) => setRFeedInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") addFeed(rFeedInput); }}
                placeholder="https://example.com/feed.xml 或 JSON API"
                style={{ flex: 1, background: "var(--bar-bg)", border: "1px solid var(--border)", color: "var(--text)", borderRadius: 8, padding: "6px 9px", fontSize: 11 }} />
              <span className="btn nodrag" onClick={() => addFeed(rFeedInput)}>＋ 加源</span>
              <span className="btn nodrag" onClick={addBuiltin}>内置源</span>
            </div>
            {rCfg.feeds.map((f) => (
              <div key={f} style={{ display: "flex", gap: 6, alignItems: "center", fontSize: 10, padding: "2px 0" }}>
                <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: "var(--dim)" }} title={f}>{f}</span>
                <span className="btn nodrag" onClick={() => delFeed(f)}>✕</span>
              </div>
            ))}

            <div style={{ display: "flex", gap: 8, alignItems: "center", margin: "12px 0 6px" }}>
              <span style={{ fontSize: 12, color: "var(--dim)" }}>已收 {rStore.items.length} 条{rStore.last_fetch ? ` · 上次 ${new Date(rStore.last_fetch).toLocaleString()}` : ""}</span>
              <span className="sp" style={{ flex: 1 }} />
              {rStore.items.length > 0 && <span className="btn nodrag" onClick={async () => { setRStore(await invoke("radar_clear")); setSaved("已清空雷达列表"); }}>清空</span>}
            </div>
            {rStore.last_error && <div style={{ fontSize: 10, color: "#ff9d5c", marginBottom: 6, wordBreak: "break-all" }}>部分源失败：{rStore.last_error.slice(0, 200)}</div>}
            {rStore.items.slice(0, 80).map((it, i) => (
              <div key={`${it.link}-${i}`} style={{ borderBottom: "1px solid var(--border)", padding: "6px 0" }}>
                <div style={{ fontSize: 12, lineHeight: 1.4 }}>
                  {it.link ? <a href={it.link} target="_blank" rel="noreferrer" style={{ color: "var(--accent)", textDecoration: "none" }}>{it.title}</a> : it.title}
                </div>
                {it.summary && <div style={{ fontSize: 10, color: "var(--dim)", marginTop: 2 }}>{it.summary}</div>}
                <div style={{ display: "flex", gap: 8, marginTop: 3, fontSize: 10, color: "var(--dim)" }}>
                  <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{it.source}</span>
                  <span className="btn nodrag" style={{ fontSize: 9, padding: "1px 6px" }} onClick={() => toTodo(it)}>→ 待办</span>
                </div>
              </div>
            ))}
            {rStore.items.length === 0 && <div style={{ fontSize: 11, color: "var(--dim)" }}>还没有资讯。加源后点「⟳ 立即抓取」。</div>}
          </>
        )}

        {tab === "空间" && (() => {
          const items = (ds?.children ?? []).filter((c) => !extFilter || c.ext === extFilter || (c.is_dir && !extFilter));
          const rects = squarify(items.map((c) => ({ key: c.path, weight: c.size })), 600, 260);
          const byPath = new Map(items.map((c) => [c.path, c]));
          return (
            <>
              <div style={{ display: "flex", gap: 6, alignItems: "center", marginBottom: 6, flexWrap: "wrap" }}>
                <span className="btn nodrag" onClick={() => scanDisk(dpath)}>{busy ? "扫描中…" : "⟳ 重新扫描"}</span>
                <span style={{ fontSize: 10, color: "var(--dim)" }}>
                  {ds ? `${ds.total_files} 文件 · ${fmtBytes(ds.total)}${ds.truncated ? " · 已截断" : ""} · ${ds.elapsed_ms}ms${ds.skipped_dirs ? ` · 跳过 ${ds.skipped_dirs} 个噪音目录` : ""}` : "尚未扫描"}
                </span>
              </div>
              <div style={{ display: "flex", gap: 4, alignItems: "center", fontSize: 10, marginBottom: 6, flexWrap: "wrap" }}>
                <span className="btn nodrag" style={{ fontSize: 10, padding: "2px 7px" }} onClick={() => goUp(-1)}>📦 收件箱</span>
                {trail.map((t, i) => (
                  <span key={t.path} style={{ display: "flex", alignItems: "center", gap: 4 }}>
                    <span style={{ color: "var(--dim)" }}>›</span>
                    <span className="btn nodrag" style={{ fontSize: 10, padding: "2px 7px" }} onClick={() => goUp(i)}>{t.name}</span>
                  </span>
                ))}
                {extFilter && <span className="btn nodrag" style={{ fontSize: 10, padding: "2px 7px", color: "var(--accent)" }} onClick={() => setExtFilter("")}>过滤：.{extFilter} ✕</span>}
              </div>

              {items.length === 0 && <div style={{ fontSize: 11, color: "var(--dim)" }}>{ds ? "这个目录是空的（或全部被截断/跳过）" : "载入中…"}</div>}

              {items.length > 0 && (
                <div style={{ position: "relative", width: "100%", paddingBottom: "43%", borderRadius: 10, overflow: "hidden", border: "1px solid var(--border)", background: "var(--bar-bg)" }}>
                  <div style={{ position: "absolute", inset: 0 }}>
                    {rects.map((r) => {
                      const n = byPath.get(r.key);
                      if (!n) return null;
                      const w = r.w / 600 * 100, h = r.h / 260 * 100;
                      const color = extColor(n.ext);
                      const showLabel = w > 12 && h > 14;
                      return (
                        <div key={r.key}
                          onClick={() => drill(n)}
                          title={`${n.path}\n${fmtBytes(n.size)} · ${n.pct.toFixed(1)}%${n.is_dir ? ` · ${n.files} 个文件` : ""}`}
                          style={{
                            position: "absolute",
                            left: `${(r.x / 600) * 100}%`, top: `${(r.y / 260) * 100}%`,
                            width: `calc(${w}% - 2px)`, height: `calc(${h}% - 2px)`,
                            // 目录略微压暗以区分于同名文件，但仍继承主导类型色
                            background: n.is_dir
                              ? `linear-gradient(135deg, ${color}ee, ${color}77)`
                              : `linear-gradient(135deg, ${color}dd, ${color}88)`,
                            borderRadius: 3, cursor: "pointer", overflow: "hidden",
                            boxShadow: n.is_dir
                              ? "inset 0 0 0 1px rgba(255,255,255,.16), inset 0 -10px 18px -8px rgba(0,0,0,.45)"
                              : "inset 0 0 0 1px rgba(255,255,255,.12)",
                            transition: "filter .15s, transform .15s",
                          }}
                          onMouseEnter={(e) => { (e.currentTarget as HTMLDivElement).style.filter = "brightness(1.35)"; }}
                          onMouseLeave={(e) => { (e.currentTarget as HTMLDivElement).style.filter = ""; }}
                        >
                          {showLabel && (
                            <div style={{ fontSize: 9, color: "#fff", padding: "2px 3px", lineHeight: 1.2, textShadow: "0 1px 2px rgba(0,0,0,.8)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                              {n.is_dir ? "📁 " : ""}{n.name}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {dsel && (
                <div style={{ marginTop: 8, fontSize: 10, border: "1px solid var(--border)", borderRadius: 8, padding: "6px 8px" }}>
                  <div style={{ color: "var(--accent)", wordBreak: "break-all" }}>{dsel.path}</div>
                  <div style={{ color: "var(--dim)", marginTop: 2 }}>{fmtBytes(dsel.size)} · {dsel.pct.toFixed(1)}%{dsel.ext ? ` · .${dsel.ext}` : ""}</div>
                </div>
              )}

              {ds && ds.ext_stats.length > 0 && (
                <>
                  <div style={{ fontSize: 11, color: "var(--dim)", margin: "10px 0 4px" }}>按类型（点击过滤）</div>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                    {ds.ext_stats.map((e) => (
                      <span key={e.ext} onClick={() => setExtFilter(extFilter === e.ext ? "" : e.ext)}
                        style={{
                          display: "inline-flex", alignItems: "center", gap: 4, cursor: "pointer",
                          fontSize: 10, padding: "2px 7px", borderRadius: 8,
                          border: `1px solid ${extFilter === e.ext ? extColor(e.ext) : "var(--border)"}`,
                          background: extFilter === e.ext ? `${extColor(e.ext)}33` : "transparent",
                        }}>
                        <i style={{ width: 7, height: 7, borderRadius: 2, background: extColor(e.ext), display: "inline-block" }} />
                        {e.ext} · {fmtBytes(e.size)} · {e.count}
                      </span>
                    ))}
                  </div>
                </>
              )}

              {drives.length > 0 && (
                <>
                  <div style={{ fontSize: 11, color: "var(--dim)", margin: "10px 0 4px" }}>磁盘容量</div>
                  {drives.map((d) => (
                    <div key={d.drive} style={{ marginBottom: 5 }}>
                      <div className="lbl" style={{ display: "flex", justifyContent: "space-between", fontSize: 10, color: "var(--dim)" }}>
                        <span>{d.drive}</span>
                        <span style={{ color: "var(--text)" }}>{fmtBytes(d.used)} / {fmtBytes(d.total)} · {d.pct.toFixed(0)}%</span>
                      </div>
                      <div className="bar"><i style={{ width: `${d.pct}%` }} /></div>
                    </div>
                  ))}
                </>
              )}
            </>
          );
        })()}

        {tab === "更多" && (
          <>
            <div style={{ fontSize: 12, color: "var(--dim)", margin: "4px 0 8px" }}>面板透明度（实时生效）</div>
            {ALPHA_KEYS.map(([k, label]) => (
              <div key={k} className="mon-row" style={{ marginBottom: 6 }}>
                <div className="lbl"><span>{label}</span><span>{Math.round((alpha[k] ?? 250) / 255 * 100)}%</span></div>
                <input type="range" min={30} max={255} style={{ width: "100%", accentColor: "var(--accent)" }}
                  value={alpha[k] ?? 250} onChange={(e) => {
                    const v = Number(e.target.value);
                    setAlpha((a) => ({ ...a, [k]: v }));
                    // 即时预览：后端 <=2s 应用到 LWA_ALPHA（持久化由下方「保存」统一做）
                    invoke("alpha_live", { label: k, alpha: v }).catch(() => {});
                  }} />
              </div>
            ))}
            <div style={{ marginTop: 4 }}><span className="btn nodrag" onClick={save}>💾 保存透明度</span></div>

            <div style={{ fontSize: 12, color: "var(--dim)", margin: "14px 0 6px" }}>桌面自动归档（新文件落盘后自动进格）</div>
            <label style={{ fontSize: 11, display: "flex", gap: 6, alignItems: "center" }}>
              <input type="checkbox" checked={watch} onChange={toggleWatch} />
              开启监听（默认关，opt-in）
            </label>
            <div style={{ fontSize: 10, color: "var(--dim)", marginTop: 3 }}>
              {watch
                ? `运行中${watchStat.last_tick ? ` · 上次扫描 ${new Date(watchStat.last_tick).toLocaleTimeString()}${watchStat.last_moved ? ` · 上次归档 ${watchStat.last_moved} 个` : ""}` : " · 等待首次扫描"}`
                : "关闭时不会动桌面任何文件"}
              <br />两轮扫描指纹一致才搬，下载中的 .crdownload/.part 一律跳过；同样过 journal 可一键复原。
            </div>

            <div style={{ fontSize: 12, color: "var(--dim)", margin: "12px 0 8px" }}>音乐（S-E：Folia/其他播放器托管）</div>
            <div style={{ display: "flex", gap: 6, alignItems: "center", marginBottom: 6 }}>
              <input className="nodrag" style={{ flex: 1, background: "var(--bar-bg)", border: "1px solid var(--border)", color: "var(--text)", borderRadius: 8, padding: "6px 9px", fontSize: 12 }}
                placeholder="播放器 exe 完整路径，如 C:\Program Files\Folia\Folia.exe" value={player}
                onChange={(e) => setPlayer(e.target.value)} />
              <span className="btn nodrag" onClick={savePlayer}>存路径</span>
            </div>
            <span className="btn nodrag" onClick={openMusic} style={{ marginRight: 8 }}>▶ 打开音乐</span>
            <div style={{ fontSize: 10, color: "var(--dim)", marginTop: 4 }}>留空则用系统默认；推荐 Folia（github.com/chthollyphile/folia-major，AGPL）。</div>

            <div style={{ display: "flex", gap: 8 }}>
              {(["glass", "glassdark", "night", "aurora", "cyber", "sunset", "contrast", "mist"] as const).map((t) => (
                <span key={t} className="btn nodrag" onClick={() => { document.documentElement.dataset.theme = t; invoke("theme_set", { theme: t }); setSaved(`皮肤：${({ glass: "液态玻璃", glassdark: "深色玻璃", night: "夜航", aurora: "极光", cyber: "赛博", sunset: "落日晚霞", contrast: "高对比", mist: "雨雾" } as Record<string, string>)[t]}`); }}>
                  {({ glass: "液态玻璃", glassdark: "深色玻璃", night: "夜航", aurora: "极光", cyber: "赛博", sunset: "落日晚霞", contrast: "高对比", mist: "雨雾" } as Record<string, string>)[t]}
                </span>
              ))}
            </div>
            <div style={{ marginTop: 12, fontSize: 11, color: "var(--dim)" }}>透明度拖动即时预览，≤2 秒生效；点「💾 保存透明度」持久化到 settings.json。</div>
          </>
        )}
      </div>

      <div className="foot">
        <span style={{ color: saved ? "var(--accent)" : "var(--dim)" }}>{saved || msg || "设置随窗口位置自动记忆"}</span>
        <span className="sp" style={{ flex: 1 }} />
        <span className="badge nodrag" onClick={() => setTab(tab === "桌面" ? "归纳" : "桌面")} title="切换">⇄</span>
      </div>
    </div>
  );
}

function downloadText(name: string, text: string) {
  const blob = new Blob([text], { type: "text/yaml" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a"); a.href = url; a.download = name; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}


// ---------- Todo（S-D，DeskBox 快捷捕获范式）----------
interface TodoT { text: string; done: boolean; created: number }
function Todo() {
  const [list, setList] = useState<TodoT[]>([]);
  const [text, setText] = useState("");
  const [, cycle] = useTheme();
  const reload = () => invoke<TodoT[]>("todo_list").then(setList).catch(() => {});
  useEffect(() => { reload(); }, []);
  const add = async () => {
    if (!text.trim()) return;
    setList(await invoke<TodoT[]>("todo_add", { text: text.trim() }));
    setText("");
  };
  return (
    <div className="card grab">
      <div className="head"><b>✔ 待办</b><span className="sp" />
        <span className="badge nodrag" onClick={cycle} title="换肤">◐</span>
      </div>
      <div style={{ padding: "0 12px 8px" }}>
        <div style={{ display: "flex", gap: 6 }}>
          <input className="nodrag" style={{ flex: 1, background: "var(--bar-bg)", border: "1px solid var(--border)", color: "var(--text)", borderRadius: 8, padding: "6px 9px", fontSize: 12, outline: "none" }}
            placeholder="记一条…回车收录" value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") add(); }} />
          <span className="btn nodrag" onClick={add}>＋</span>
        </div>
      </div>
      <div className="shelf-body nodrag">
        {list.length === 0 && <div style={{ fontSize: 11, color: "var(--dim)", padding: 8 }}>空空如也<br />桌面想到什么随时记</div>}
        {list.map((t, i) => (
          <label key={i} style={{ display: "flex", gap: 8, alignItems: "center", fontSize: 12, padding: "4px 0" }}>
            <input type="checkbox" checked={t.done} onChange={() => invoke("todo_toggle", { i }).then(reload)} />
            <span style={{ flex: 1, textDecoration: t.done ? "line-through" : "none", opacity: t.done ? 0.5 : 1 }}>{t.text}</span>
            <span className="btn nodrag" onClick={() => invoke("todo_del", { i }).then(reload)}>✕</span>
          </label>
        ))}
      </div>
    </div>
  );
}


// ---------- 资讯雷达卡片（v0.6）----------
interface RadarItemT2 { title: string; link: string; summary: string; source: string; ts: number }
function Radar() {
  const [items, setItems] = useState<RadarItemT2[]>([]);
  const [, cycle] = useTheme();
  const reload = () => invoke<{ items: RadarItemT2[] }>("radar_items").then((s) => setItems(s.items.slice(0, 6))).catch(() => {});
  useEffect(() => {
    reload();
    const t = window.setInterval(reload, 60000);
    return () => window.clearInterval(t);
  }, []);
  return (
    <div className="card grab">
      <div className="head"><b>📡 资讯雷达</b><span className="sp" />
        <span className="badge nodrag" onClick={cycle} title="换肤">◐</span>
      </div>
      <div className="shelf-body nodrag">
        {items.length === 0 && <div style={{ fontSize: 11, color: "var(--dim)", padding: 8 }}>暂无资讯<br />控制台「雷达」tab 加源</div>}
        {items.map((it, i) => (
          <div key={`${it.link}-${i}`} style={{ padding: "5px 0", borderBottom: "1px solid var(--border)" }}>
            <div style={{ fontSize: 12, lineHeight: 1.35, overflow: "hidden", textOverflow: "ellipsis", display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical" }}
              onDoubleClick={() => it.link && invoke("open_path", { path: it.link }).catch(() => {})}
              title={it.link}>{it.title}</div>
          </div>
        ))}
      </div>
      <div className="foot"><span>最新 {items.length} 条</span><span className="sp" style={{ flex: 1 }} /><span>雷达</span></div>
    </div>
  );
}

function useApplyThemeOnce() {
  // 每个窗口（含控制台）都必须有 data-theme，否则所有主题色变量都是 undefined。
  useEffect(() => {
    invoke<string>("theme_get").then((t) => {
      document.documentElement.dataset.theme = THEMES.includes(t as Theme) ? t : "glass";
    }).catch(() => { document.documentElement.dataset.theme = "glass"; });
  }, []);
}

/// 液态玻璃：指针位置驱动镜面高光（CSS 变量 --mx/--my）
function useGlassPointer() {
  useEffect(() => {
    const onMove = (e: PointerEvent) => {
      const el = (e.target as HTMLElement | null)?.closest?.(".card") as HTMLElement | null;
      if (!el) return;
      const r = el.getBoundingClientRect();
      el.style.setProperty("--mx", `${e.clientX - r.left}px`);
      el.style.setProperty("--my", `${e.clientY - r.top}px`);
    };
    window.addEventListener("pointermove", onMove, { passive: true });
    return () => window.removeEventListener("pointermove", onMove);
  }, []);
}

export default function App() {
  useApplyThemeOnce();
  useGlassPointer();
  const label = getCurrentWindow().label;
  if (label === "shelf") return <Shelf />;
  if (label === "clock") return <Clock />;
  if (label === "sysmon") return <Sysmon />;
  if (label === "note") return <Note />;
  if (label === "settings") return <Settings />;
  if (label === "todo") return <Todo />;
  if (label === "radar") return <Radar />;
  return <div className="card"><div className="head">未知面板 {label}</div></div>;
}
