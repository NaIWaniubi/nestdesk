# NestDesk 性能与稳定性审查（perf 专项）

- 日期：2026-11-24；审查角色：性能与稳定性审查员（只找毛病）
- 范围：`nestdesk-app/src-tauri/src/{lib.rs, fs_core.rs, layer.rs, main.rs}`、`nestdesk-app/src/App.tsx`
- 方法：静态走查 + **实测**（临时 bench 二进制直接调用 `fs_core` 同源算法，跑完即删；`--selftest` 按指定命令实测计时）
- 机器：Windows 11 (10.0.22631)，AMD Ryzen 7 7435H（8C/16T），31.7 GB RAM，NVMe SSD，PowerShell 5.1
- 构建配置：release = opt-level=3 + lto=true + codegen-units=1 + panic=abort + strip=true（Cargo.toml）

> 注意：本工作树在审查期间被并发修改（并行修复流）。每条 finding 标注状态：
> 「仍存在」= 当前树仍有效；「已修复(观察)」= 审查中已被并发修复，仅作记录。

---

## 0. 实测数字总表

所有耗时为本机实跑（median 或单次，已注明）。「debug」= dev profile，「release」= 发布 profile。

| 场景 | debug | release |
|---|---|---|
| `--selftest`（234 样本文件 + undo_all，~230 文件系统操作） | 3.0–4.9 s | 3.2–3.8 s |
| `powershell -NoProfile` 冷启动 ×12（avg / best） | avg **874 ms**，best 675 ms | — |
| `[Environment]::GetFolderPath('Desktop')` 一次查询 | 783 ms | — |
| snapshot（桌面 1000 文件 + 收件箱 999 文件，1999 条扫描） | 121 ms | 77 ms |
| 纯 read_dir+stat 枚举基线 @1000 条目 | 104 ms | 57 ms |
| **collect_desktop @1000 个文件**（归类+唯一名探测+rename+journal） | **4420 ms** | **2908 ms** |
| undo_all @1000 条日志 | 3244 ms | 3614 ms |
| desk_items @1000 桌面文件 | 150 ms | 149 ms |
| suggest_plan(3) | 77 ms | 78 ms |
| find_dups：20k×256B 全同内容（单 size 桶，实测 sets=1、count=20000） | 8108 ms | 5227 ms |
| find_dups：30k×256B 分 300 个 size 桶 | 64816 ms | 12813 ms |
| find_dups：1500×512KB=768MB 全同内容 | 35491 ms（21.1 MB/s） | 1251 ms（**599 MB/s**） |
| .NET 对照：纯读 20k×256B 文件字节 | 2577 ms | — |
| read_entries 解析 journal：1k / 10k / 50k 行 | 9 / 94 / 515 ms | 1 / 16 / 83 ms（≈10.3 / 1.66 µs/行） |
| Win32 单窗口（20000 次均值）：GetWindowRect / ClientToScreen+GetClientRect / hwnd() 取句柄 | 2.3–6.5 / 2.4–4.5 / ~0.05 µs | — |
| SetLayeredWindowAttributes（LWA_ALPHA） | **38–76 µs/call** | — |
| SetWindowPos(HWND_BOTTOM)（对自有控制台窗口，跨进程目标方差大） | 300 µs–5.8 ms/call | — |

磁盘产物：`target/debug` 共 5.2 GB；`target/release` 1.3 GB；debug exe 15.3 MB（+117 MB PDB），release exe 4.7 MB。
全新 `cargo build --release`（空 target/release，LTO+codegen-units=1）：本机墙钟约 **60 分钟**出包；其后增量复验仅 65 s。

---

## 1. selftest 的规模与耗时（问题 1）

**实测**：`cd src-tauri && time ./target/debug/nestdesk-app.exe --selftest`
- debug：3 次分别为 4858 / 3862 / 2800 ms（约 3–4 s）
- release（当时版本）：3231 / 3797 / 3728 ms —— **比 debug 只快 10–20%，噪声内**
- 结论：该 harness 是 I/O 型测试，优化 profile 几乎买不到时间；但每次发版跑它是秒级成本，本身可接受。

**仍存在的问题**：
1. **临时目录从不清理**（lib.rs:681-887，selftest_body 无 `remove_dir_all`）：实测每次运行在 `%TEMP%` 遗留 **232 个文件 / 315 KB**；清理前该目录已累积 **24 个孤儿 selftest 目录 ≈ 10 MB**。长期 CI 会持续膨胀。
2. 234 个样本文件的构造 + undo_all 全量重建，对产品回归面意义有限，边际收益低。

**已修复(观察)**：`--demo` 会搬动真实桌面（SPEC 曾记录 demo 355/355）；当前树已改为需 `NESTDESK_DEMO=1` 显式确认 + `#[cfg(any(debug_assertions, feature="harness"))]` 门控（main.rs:7-17、lib.rs:626-631）。`--selftest` 同理被 cfg 门控出 release 二进制——这同时消除了「release 下 windows_subsystem=windows 导致 JSON 输出无处可去」的历史坑。✔

**建议**：把 selftest 挪进 `#[cfg(test)]` 或独立 `tests/`，并在结尾无条件 `remove_dir_all(root)`；CI 用 debug 跑即可（3–4 s）。

---

## 2. 2 秒主循环：系统调用数与合成器代价（问题 2）

先回答「是否跨越 IPC/WA」：**不跨**。查 tao 0.37.3 源码（window.rs:177-225 → util.rs:57-67）：
`outer_position()` = 直接 `GetWindowRect`，`inner_size()` = `ClientToScreen + GetClientRect`，`hwnd()` = 结构体字段读取。均为进程内直接 Win32 调用，没有跨线程同步消息往返——因此主循环不构成 IPC 死锁风险（这是好消息，须写进结论）。

**每秒系统调用数**：每 tick（2 s）每窗口 5 次调用（hwnd 取句柄、outer_position、inner_size、set_alpha、pin_bottom 的 IsIconic+SetWindowPos）→ **≈13–14 calls/s（5 窗口合计）**。绝对量可忽略。

**仍存在的问题**：
1. **set_alpha 每 tick 无条件调用**（lib.rs:591-598）：alpha 未变也刷。实测 SLWA 单窗 **38–76 µs**（是 GetWindowRect 的 15–30 倍），其代价不只是 API 延迟——每次调用强制 DWM 对分层表面重新 alpha-blend；5 窗 × 每 2 s = 每分钟 150 次无谓的合成器重绘。`alpha_live` 命令（lib.rs:184-193）已是事件入口，应改为「设置变更时才下发」。
2. **pin_bottom 每 tick SetWindowPos(HWND_BOTTOM)**（lib.rs:599）：实测同参数 SWP 在自有控制台窗口上 **300 µs–5.8 ms/call**（方差来自目标窗口所属进程的同步处理）。对应用内分层窗通常更低，但每 2 s 重复沉底没有必要：Z 序稳定后只需在「有窗口压上来」时重钉；同时它正是 Win11 桌面图标闪动的可疑来源之一。
3. 首轮行为：`sleep(2000)` 在前（lib.rs:568），启动后前 2 s 内面板既不分层也不沉底——瞬态表现问题，非性能。

**建议**：循环里缓存上次应用的 alpha 与 hwnd 集合；SetWindowPos(HWND_BOTTOM) 仅在 Z 序被扰动后调用（或干脆只在 ensure_layered 时做一次 + WM_WINDOWPOSCHANGED 钩子）。

---

## 3. collect_desktop 的三次 O(n) 扫描（问题 3）

**仍存在的问题（当前最贵的用户可见路径）**：
- 实测 collect_desktop @1000 个桌面文件 = **debug 4420 ms / release 2908 ms**，折合 **≈2.9–4.4 ms/文件**。
- 分解（@1000，纯 FS 基线 release 57 ms）：read_dir 枚举 + is_file/metadata ≈ 60–70 ms；其余 ~2.85 s 全部来自逐文件 syscall + 实时杀软对新目标的扫描：每文件至少 4 次元数据操作（unique_dest 的 `exists()` stat、move_any 的 `create_dir_all(parent)`、`rename`、ingest 的 `src.exists()`），NTFS 上每次新建/改名都触发 AV on-create。
- 按本机真实桌面（499 条目）折算：收取桌面 ≈ **1.0–1.6 s**；1000 文件桌面 ≈ 3–4.4 s（点击后白屏感）。
- 「三次扫描」的具体账：collect_desktop 自身 1 次全目录扫描（fs_core.rs:373-381）+ ingest 逐文件；前端 Shelf 动作后 `refresh()` 又跑**全量 snapshot**（desktop read_dir + 每格 read_dir + journal 全量解析）。三趟都是 O(n) 全量，无增量、无差异合并。

**建议**：批内维护「已占用名」HashSet，省掉绝大多数 `exists()` 探测（每文件 -1 stat ≈ syscall 量 -25%）；跨卷搬运的 copy→SHA 校验→删源（fs_core.rs:127-167）是对的方向，但注意 `sha256_of(src)` 先读、`sha256_of(dst)` 后读 = 大文件最坏 **2× 全量读**。

---

## 4. find_dups 收件箱 10 万小文件（问题 4）

**仍存在的问题**：
1. **吞吐天花板（单线程、全量哈希）**：实测 768 MB 同内容文件 = release **1251 ms（599 MB/s）**、debug 35491 ms（21 MB/s）。按此推算：10 万张 4 MB 照片（400 GB）≈ **11 分钟不可取消**；10 万小文件按 426 µs/个 ≈ **43 s**（release）。
2. **无进度、不可取消、无分块**：dup_scan 是同步命令，前端只有一句「扫描完成」。长扫描期间用户只能干等；且任一文件不可哈希 → 整轮返回 Err，前功尽弃（fs_core.rs:697 `ok_or_else(...Err)`）——半途而废的代价与全量一样大。
3. **dup_archive 二次全扫**：lib.rs:302 对整个收件箱再跑一遍 find_dups 再过滤归档，等于扫描成本 ×2（768 MB 场景 +1.25 s release）。
4. **大集合回传前端**：实测 20k 同内容文件 → sets=1、paths 长度 20000。DupSet.paths 全量序列化过 IPC（~1.5–2 MB JSON），前端再 `join("\n")` 渲染上万行 div——渲染进程卡死风险。**当前无 cap**。
5. 内存：按 size 分桶持有全部路径（HashMap<u64, Vec<String>>）+ by_sha 桶。当前树已改流式 64 KB 缓冲（fs_core.rs:110 sha256_of），整文件硬读入内存的问题**已修复(观察)**；但 10 万条路径的常驻映射仍要 ~25–35 MB，且按 wasted 排序在超大数据下也有成本。

**建议**：size 桶内先做「头部 N 字节采样哈希」预筛，仅对候选组做全量 SHA（对照片类收益巨大）；sets.paths 服务端截断（如 200/组，前端给「查看全部」懒加载）；dup_archive 复用 dup_scan 的结果而不是重扫。

---

## 5. App.tsx 前端成本（问题 5）

**仍存在的问题**：
1. **refresh() 全量 snapshot、无节流**：Shelf 在挂载、每次 drop、每条 URL 下载完成、以及**每个按钮动作**（act）后都全量刷新（App.tsx refresh 定义于 Shelf 组件）。snapshot 本身 release @2k 文件 77 ms，但每次都伴随 journal 全量解析（10k 行 16 ms、50k 行 83 ms）与全部收件箱格 read_dir；连续多拖放时叠加。
2. **payload 随收件箱线性膨胀且无服务端裁剪**：snapshot 把每个格的**全部文件名**回传（当前树改为完整路径 fs_core.rs:503，更大）。估算：2 万收件箱文件 → 每次刷新 ≈ **2.2–2.5 MB JSON**（序列化+IPC+JS 解析+React diff）。UI 每格只显示 60 个。desktop 侧有 take(60)（fs_core.rs:516），inbox 侧没有对等裁剪。
3. **桌面勾选列表静默截断 120**（App.tsx desk.slice(0,120)）：超过 120 个桌面文件时其余项不可选且无提示。本机真实桌面已达 499 条目，默认就超限。
4. details 展开后每文件一个 span + 双击/右键 handler（当前树的改进），60 项上限保住了 DOM；但配合第 2 条的大 payload，面板内存峰值仍由全量数据决定。
5. Sysmon 2 s 定时器走纯 Win32（get_sysinfo 无进程拉起）——这条没问题；Clock 农历每小时拉起一次 PowerShell（约 0.87 s CPU 尖峰/小时，App.tsx:53 + lib.rs:400-417）。

**建议**：snapshot 增加可选参数（group 过滤 / 分页 / 路径改相对名）；前端对 refresh 做 300–500 ms trailing debounce；桌面列表给出「显示全部/仅前 120」入口。

---

## 6. 布局落盘抖动与 SSD/HDD 损耗（问题 6）

量化结论：**损耗可忽略，问题不在磨损**。
- save_layout（lib.rs:324-335）每次 settle = write(tmp ~250 B) + rename(main→bak) + rename(tmp→main) = 4 次 FS 操作；
- 触发条件：几何静止 >3 s（lib.rs:611-618）。持续拖拽时 changed 恒真、不落盘；只有停顿才保存。按极端使用（每分钟 1 次 >3 s 停顿）×10 分钟 = ≤10 次保存 ≈ 40 次元数据操作、写入 <10 KB。对 SSD（TBW 数百 TB）与 HDD（≤40 次寻道）均无感。
- **真正的问题（仍存在）**：load_layout（lib.rs:316-322）**从不读取 layout.json.bak**——settings 有自愈（lib.rs:75-86）、layout 没有；主文件损坏即静默丢布局。此外 tmp/bak 两文件长期残留（无害但脏）。
- 顺带：init_settings() 在**每次启动**都无条件 save_settings_file()（lib.rs:96），2 次 rename + 1 次写，启动期无谓 IO（毫秒级，顺手可修）。

**建议**：load_layout 加 .bak 回退（与 settings 自愈对齐）；启动时「内容未变则不落盘」。

---

## 7. 内存常驻 vs DeskBox NativeAOT（问题 7）

**依据与口径**：
- 本机现存 **20 个 msedgewebview2.exe 进程，合计 895.8 MB 工作集**（含 VSCode/Edge 等其他应用的 webview，非 NestDesk 独占）。
- Tauri/WebView2 架构（wry 0.57.3 create_environment 每 webview 独立环境）：每个窗口一个 renderer 进程；browser/GPU/utility 为共享进程。NestDesk 启动即建 **6 个窗口**（tauri.conf.json），各载同一份 ~250 KB 的 React bundle 并各自持有 JS 堆。
- 估算（无 GUI 实测，避免打扰桌面）：NestDesk 空闲常驻 ≈ **350–500 MB 工作集**（共享 browser+GPU+utility ~120–180 MB + 6 × renderer 30–60 MB）。Rust 侧：debug exe 15.3 MB（断言/无 LTO）vs release exe 4.7 MB（strip+LTO），RSS 差约 10–30 MB——**相对 JS 引擎是零头**。
- DeskBox（FUSION-PLAN.md 记其栈为 C# WinUI3 + .NET10 NativeAOT + Rust Shell，单进程原生、无 JS 引擎）同类常驻显著更低（NativeAOT + 单进程通常 1/4–1/3）。结论：与竞品对比的**结构性劣势在渲染运行时，不在 Rust 优化档位**；release profile 对内存几乎无贡献，对 CPU-bound 路径（find_dups 哈希：debug 21 MB/s → release 599 MB/s，约 28×）才是决定性变量。

**建议**：若要与 DeskBox 比内存，方向是减窗口数/合并为单窗口标签页，而不是调 Rust profile；release 对 CPU 型 workload 必开。

---

## 8. 其他顺带发现

- undo_all（fs_core.rs:460-463）`all.clone()` 全量克隆日志 Vec；undo_entries 重写 journal 时再全量序列化。日志长期无界增长（无 compaction，只有 undo 时压缩）时成本线性叠加（50k 行仅解析就 83 ms release）。
- move_any 跨卷分支对大目录做整树双哈希（hash_tree 前后各一遍，fs_core.rs:176-205），跨卷搬大树是最坏读放大。
- ingest 每次调用都 `unwrap_or_else(default_groups)` 重建 9 组 ~130 条扩展名字符串（fs_core.rs 循环内 ~350 行处）；collect_desktop 单次调用无所谓，按文件粒度调用就会放大。
- group_resolve 对扩展名是线性扫（exts.contains），用户每加一组分类表就乘一遍；百组级别才会显现。
- layer.rs 的 E0133 unsafe lint 警告若干（编译告警噪音，非性能）。

---

## 9. 状态小结

| # | 问题 | 状态 |
|---|---|---|
| 1 | selftest 不清临时目录 + 每次发版跑（I/O 型，profile 无关） | 仍存在（低）；release 已 cfg 门控 ✔ |
| 2 | set_alpha/pin_bottom 每 2 s 无条件刷 DWM/Z 序 | 仍存在（中） |
| 3 | collect/undo ≈3 ms/文件；前端动作触发全量 snapshot | 仍存在（高） |
| 4 | find_dups 单线程全量哈希、无进度、大集合回传 | 仍存在（高） |
| 5 | snapshot payload 随收件箱线性膨胀（现为完整路径）、refresh 无节流、桌面 120 静默截断 | 仍存在（中高） |
| 6 | 布局 .bak 从不被读取；损耗本身可忽略 | 仍存在（低） |
| 7 | WebView2 6 进程 ~350–500 MB 常驻；Rust profile 对内存无感 | 仍存在（结构性） |
| 附 | 每 command spawn powershell 取桌面路径（原实测 avg 874 ms） | 已修复(观察)：OnceLock memoize ✔ |
| 附 | --demo 搬动真实桌面 | 已修复(观察)：env 确认 + cfg 门控 ✔ |
| 附 | find_dups 整文件硬读内存 | 已修复(观察)：64 KB 流式缓冲 ✔ |
| 附 | RwLock poison / journal 互斥 / URL undo / 焦点抢占 / 死 HWND 清理 / 屏外钳制 | 已修复(观察) ✔ |

---

## 10. 最贵三条（量化结论）

1. **收取/复原的文件搬运税**：collect_desktop 1000 文件实测 2.9 s(release)–4.4 s(debug)，undo_all 1000 条 3.2–3.6 s，折合 ≈2.9–4.4 ms/文件，其中纯代码扫描只占 ~70 ms，其余是逐文件 4 次 NTFS 元数据操作 + 杀软 on-create。本机 499 条真实桌面一次「收取桌面」≈1.0–1.6 s 体感。头号可削减点：批内唯一名 HashSet（省每文件 1 次 stat，syscall 量约 -25%）。
2. **查重扫描**：768 MB 实测 release 1.25 s（599 MB/s）、debug 35.5 s（21 MB/s）；小文件 426 µs/个（2 万文件 5.2 s release）；400 GB 级媒体库 ≈ 11 分钟单线程、不可取消、中途任一小文件不可读即整轮作废，且归档动作还会把收件箱从头再扫一遍；超大集合还带 ~2 MB IPC/DOM 回传风险（实测 sets=1×20000）。
3. **全量快照风暴**：每次 UI 动作（含每次拖放/下载完成/按钮）都全量 snapshot + journal 全量解析（50k 行 83 ms release）+ 全部格 read_dir；当前 inbox files 以完整路径回传，2 万收件箱文件 = 每动作 ~2.2–2.5 MB JSON 过 IPC。历史上每个 command 还有 ~874 ms 的 PowerShell 税（现已 memoize 消除，属已修复项）。