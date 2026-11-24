import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { invoke } from "@tauri-apps/api/core";

// ---------- 皮肤 ----------
const THEMES = ["night", "sunset", "contrast", "mist"] as const;
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

// ---------- 系统监视 ----------
interface SysInfo {
  cpu: number; mem_total: number; mem_used: number;
  battery: { percent: number; ac: boolean; charging: boolean } | null;
}
function Sysmon() {
  const [info, setInfo] = useState<SysInfo | null>(null);
  const [, cycle] = useTheme();
  useEffect(() => {
    let alive = true;
    const tick = () => invoke<SysInfo>("get_sysinfo").then((s) => alive && setInfo(s)).catch(() => {});
    tick();
    const t = setInterval(tick, 2000);
    return () => { alive = false; clearInterval(t); };
  }, []);
  const cpu = info && info.cpu >= 0 ? Math.round(info.cpu * 100) : 0;
  const mem = info && info.mem_total ? Math.round((info.mem_used / info.mem_total) * 100) : 0;
  const bat = info?.battery;
  return (
    <div className="card grab">
      <div className="head"><b>系统</b><span className="sp" />
        <span className="badge nodrag" onClick={cycle} title="换肤">◐</span>
      </div>
      <div className="mon-rows">
        <div className="mon-row">
          <div className="lbl"><span>CPU</span><span>{cpu}%</span></div>
          <div className="bar"><i style={{ width: `${cpu}%` }} /></div>
        </div>
        <div className="mon-row">
          <div className="lbl"><span>内存</span><span>{info ? fmtBytes(info.mem_used) : "--"}</span></div>
          <div className="bar"><i style={{ width: `${mem}%` }} /></div>
        </div>
        <div className="chip">
          {bat === null || bat === undefined
            ? "🖥️ 台式机 · " + (info ? `${fmtBytes(info.mem_total)} RAM` : "")
            : `${bat.ac ? "🔌 插电" : "🔋 电池"}${bat.charging && bat.percent < 100 ? " · 充电中" : ""} · ${bat.percent}%`}
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
  type Tab = "桌面" | "归纳" | "事务" | "查重" | "更多";
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

  const tabs: { k: Tab; label: string }[] = [
    { k: "桌面", label: "桌面" }, { k: "归纳", label: "归纳" }, { k: "事务", label: "事务" },
    { k: "查重", label: "查重" }, { k: "更多", label: "更多" },
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
              {(["night", "sunset", "contrast", "mist"] as const).map((t) => (
                <span key={t} className="btn nodrag" onClick={() => { document.documentElement.dataset.theme = t; invoke("theme_set", { theme: t }); setSaved(`皮肤：${({ night: "夜航", sunset: "落日晚霞", contrast: "高对比", mist: "雨雾" } as Record<string, string>)[t]}`); }}>
                  {({ night: "夜航", sunset: "落日晚霞", contrast: "高对比", mist: "雨雾" } as Record<string, string>)[t]}
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


export default function App() {
  const label = getCurrentWindow().label;
  if (label === "shelf") return <Shelf />;
  if (label === "clock") return <Clock />;
  if (label === "sysmon") return <Sysmon />;
  if (label === "note") return <Note />;
  if (label === "settings") return <Settings />;
  if (label === "todo") return <Todo />;
  return <div className="card"><div className="head">未知面板 {label}</div></div>;
}
