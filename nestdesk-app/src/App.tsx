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
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
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
    <div className={"card" + (hot ? " drop-hot" : "")}>
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
            <div className="files">{g.files.slice(0, 60).join("、")}{g.files.length > 60 ? ` …共 ${g.files.length}` : ""}</div>
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
  ["shelf", "收纳框"], ["clock", "时钟"], ["sysmon", "系统监视"], ["note", "便利贴"],
] as const;

function Settings() {
  const [groups, setGroups] = useState<GroupRule[]>([]);
  const [alpha, setAlpha] = useState<Record<string, number>>({});
  const [saved, setSaved] = useState("");

  useEffect(() => {
    invoke<AppSettings>("settings_get").then((s) => {
      setGroups(s.groups);
      setAlpha(s.alpha);
    });
  }, []);

  const updateExts = (i: number, raw: string) => {
    const exts = raw.split(/[,，\s]+/).map((e) => e.trim().toLowerCase()).filter(Boolean);
    setGroups((gs) => gs.map((g, j) => (j === i ? { ...g, exts } : g)));
  };
  const addGroup = () => setGroups((gs) => [...gs, { name: "", exts: [] }]);
  const removeGroup = (i: number) =>
    setGroups((gs) => (gs.length <= 1 ? gs : gs.filter((_, j) => j !== i)));

  const save = async () => {
    // 校验：空名行跳过；至少要有一格
    const clean = groups.map((g) => ({ ...g, name: g.name.trim() })).filter((g) => g.name);
    if (clean.length === 0) {
      setSaved("至少要保留一个归纳文件夹");
      return;
    }
    if (!clean.some((g) => g.name === "其他")) {
      clean.push({ name: "其他", exts: [] }); // 兜底格，保证未匹配文件有归宿
    }
    await invoke("settings_save", { groups: clean, alpha });
    setGroups(clean);
    setSaved(`✓ 已保存：${clean.length} 个归纳文件夹，透明度即时生效`);
  };

  return (
    <div className="card" style={{ overflow: "auto" }}>
      <div className="head"><b>🏠 归巢 · 控制台</b><span className="sp" /></div>

      <div style={{ padding: "4px 14px 8px", fontSize: 12, color: "var(--dim)" }}>
            归纳文件夹 = 文件归档的分类格。扩展名空格或逗号分隔；「其他」是未匹配文件的兜底格（自动追加、不可删）。
          </div>
      <div style={{ padding: "0 14px 10px" }}>
        {groups.map((g, i) => (
          <div key={i} style={{ display: "flex", gap: 6, marginBottom: 6, alignItems: "center" }}>
            <input className="nodrag"
              style={{ flex: 1, background: "var(--bar-bg)", border: "1px solid var(--border)",
                color: "var(--text)", borderRadius: 8, padding: "6px 9px", fontSize: 12, outline: "none" }}
              value={g.name}
              onChange={(e) => setGroups((gs) => gs.map((h, j) => (j === i ? { ...h, name: e.target.value } : h)))}
              placeholder="文件夹名，如「论文」" />
            <input className="nodrag"
              style={{ flex: 2, background: "var(--bar-bg)", border: "1px solid var(--border)",
                color: "var(--text)", borderRadius: 8, padding: "6px 9px", fontSize: 12, outline: "none" }}
              value={g.exts.join(" ")}
              onChange={(e) => updateExts(i, e.target.value)}
              placeholder="扩展名，如 pdf doc png" />
            <span className="btn nodrag" onClick={() => removeGroup(i)}>✕</span>
          </div>
        ))}
        <span className="btn nodrag" onClick={addGroup}>＋ 添加归纳文件夹</span>
      </div>

      <div style={{ padding: "0 14px 8px", fontSize: 12, color: "var(--dim)" }}>面板透明度（实时生效）</div>
      <div style={{ padding: "0 14px 10px" }}>
        {ALPHA_KEYS.map(([k, label]) => (
          <div key={k} className="mon-row" style={{ marginBottom: 4 }}>
            <div className="lbl"><span>{label}</span><span>{Math.round((alpha[k] ?? 250) / 255 * 100)}%</span></div>
            <input type="range" min={30} max={255}
              style={{ width: "100%", accentColor: "var(--accent)" }}
              value={alpha[k] ?? 250}
              onChange={(e) => setAlpha((a) => ({ ...a, [k]: Number(e.target.value) }))} />
          </div>
        ))}
      </div>

      <div style={{ padding: "0 14px 8px", fontSize: 12, color: "var(--dim)" }}>皮肤</div>
      <div style={{ padding: "0 14px 12px", display: "flex", gap: 8 }}>
        {(["night", "sunset", "contrast", "mist"] as const).map((t) => (
          <span key={t} className="btn nodrag"
            onClick={() => {
              document.documentElement.dataset.theme = t;
              invoke("theme_set", { theme: t });
              setSaved(`皮肤：${({ night: "夜航", sunset: "落日晚霞", contrast: "高对比", mist: "雨雾" } as Record<string, string>)[t]}（即时生效）`);
            }}>
            {({ night: "夜航", sunset: "落日晚霞", contrast: "高对比", mist: "雨雾" } as Record<string, string>)[t]}
          </span>
        ))}
      </div>

      <div style={{ padding: "0 14px 16px" }}>
        <span className="btn nodrag" onClick={save} style={{ padding: "8px 22px" }}>💾 保存设置</span>
      </div>
      <div className="toast">{saved || "设置会随窗口位置自动记忆"}</div>
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
  return <div className="card"><div className="head">未知面板 {label}</div></div>;
}
