# 归巢 NestDesk — 桌面分区收纳 + 一键复原（自用 MVP 规格）

日期：2026-11-24 · 定位：自用 MVP（M1+M2）· 目标机：Windows 11 22631 x64
后续可扩展：桌面栏/Dock、mpv 音乐卡片（M3+，本 MVP 不做，架构预留）。

## 1. 一句话定义

在桌面上放"分区收纳框"（shelves），文件拖入或按规则自动归档进真实文件夹；
所有文件操作写入事务日志，支持任意深度**一键复原**回到操作前的桌面状态。

## 2. 核心设计决策

| 决策 | 选择 | 理由 |
|---|---|---|
| 收纳框本质 | 嵌入桌面层（Progman→WorkerW）的文件夹视图，**不碰真图标、不写注册表视图键** | 复原从"不可能"变"简单"；Fences/Nimi Places 同路线 |
| 技术栈 | Tauri 2 + React(Vite) + Rust(`windows` crate) + rusqlite/SQLite | 工具链已核：rustc 1.98-msvc、VS2026、Node 24、WebView2 157 全在 |
| 归档语义 | 真移动文件；目标默认 `%USERPROFILE%\Nest\<框名>\`，每框可改 | 与"展示层"分离，可审计可回滚 |
| 复原语义 | journal 逆序回放（op 级）+ 布局 JSON 重载 + "显示真图标"不受污染 | 无残留是架构保证，不是兜底承诺 |
| 冲突策略 | 同名不覆盖：自动加 ` (2)` 后缀，journal 记真实落点 | 复原按落点走，不猜 |
| 危险对象 | 云占位符(OneDrive)/网络盘/重解析点 默认**跳过并提示**，不静默搬 | 防断链；见兼容矩阵 |

## 3. 兼容矩阵（验收时逐项勾）

| # | 场景 | 期望 |
|---|---|---|
| C1 | Win+D 显示桌面→恢复 | shelves 位置不变、仍在桌面层（重挂机制） |
| C2 | 分辨率切换 / 125%/150% DPI 缩放 | 框与内容不糊不偏移（Per-Monitor V2） |
| C3 | 双显示器（含主屏变更） | 框跟随所属屏坐标，不跳没 |
| C4 | explorer.exe 重启（进程被杀拉起） | 自动重挂 WorkerW，位置/状态无感 |
| C5 | 系统重启 | 开机自启（若开启），从 DB+JSON 完整恢复 |
| C6 | 最大化应用窗口覆盖 | 框只在桌面可见（正确挂层），不浮于应用上 |
| C7 | 拖入被占用文件（Word 开着） | 重试 N 次后放弃并记 journal=skipped，不卡 UI |
| C8 | 路径>260 字符 / Unicode 中文文件名 | 长路径前缀 + UTF-8 全链路无乱码 |
| C9 | OneDrive「释放空间」占位符 | 检测重解析点，跳过并 toast 提示 |
| C10 | 框内文件被外部（资源管理器）改名/删除 | journal 复原时检测源缺失→报告"该文件已不可回滚"，其余照常回滚 |
| C11 | 管理员权限窗口的拖放 | UIPI：应用以普通权限运行，接受限制并记录（边界，见 §7） |
| C12 | 全屏游戏/演示 | 托盘一键"暂停 shelves"（隐藏+停止重挂） |

## 4. 阶段切片

### M1 桌面收纳框（先 spike 再功能）
- **S0 spike ✅ 已完成（2026-11-24，spike/ 目录，188KB 零依赖 exe）**：子窗口路线实测作废（§7），
  桌面层定案 = top-level `WS_POPUP|WS_EX_LAYERED|WS_EX_TOOLWINDOW` + `HWND_BOTTOM` + 2s 定时器回魂。
  已验证：清屏后桌面可见、被普通窗口正确压底、无重挂抖动。
- 框 CRUD：新建/改名/改色/调整大小/删除（删除框=可选"文件留在原地"）。
- 拖入归档：WebView2 收 HTML5 drop（拿到文件路径需走 Rust 端 `DroppingFiles`/IDropTarget 桥，spike 时验证）。
- 规则引擎 v1：per-box 规则 = 扩展名集合 / 文件名通配 / 大小 / mtime 年龄；`自动收件箱`框 = 桌面散落文件定时吸入。规则格式借鉴 organize（YAML 兼容子集），MVP 先 GUI 表单，YAML 导入导出留 M4。
- 托盘图标 + 全局显隐快捷键。

### M2 事务与一键复原
- journal（SQLite，WAL）：`batch(op_id, ts, kind, old_path, new_path, sha256?, status)`；
  每次归档/规则批量跑 = 一个 batch。
- **一键复原**：按 batch 逆序回放 → 文件移回原位（原路径被占则同策略加后缀）；
  全局"全部复原"清空 journal 回放。C10 缺失项进报告页。
- 布局持久化：框几何/颜色/规则存 `%APPDATA%\NestDesk\layout.json`（原子写 + 保留上一份 `.bak`）。
- 开机自启开关（注册表 Run 键——唯一允许的注册表写入，卸载即删）。

### M1.5 桌面小组件（与 M1 共用 surface 引擎）
小组件 = 收纳框的同构兄弟：同一个 top-level 沉底窗口引擎，只是内容不同、无文件系统语义。
- 每个组件独立窗口：无边框透明、可拖拽、位置随 layout.json 持久化；
- **组件包 v1（全离线数据源）**：
  - `clock` 时钟+日期（S/M 两尺寸）
  - `sysmon` CPU/内存实时监视（GetSystemTimes/GlobalMemoryStatusEx）
  - `battery` 电量+充电状态（GetSystemPowerStatus，笔记本场景刚需）
  - `note` 便利贴（内容存 SQLite，重启不丢）
  - `shelf-stats` 收纳框统计（联动 M1：每框文件数/体积，一键跳 journal）
- **组件包 v2（联网，TUN 开启才显示，失败自动降级为隐藏）**：
  - `weather`（open-meteo，免 key）
  - `countdown` 倒计时（赛事 deadline 用得上）
- **改装体系**：皮肤 = JSON（调色板/字号/圆角/透明度），内置 4 套（夜航/落日晚霞/高对比/雨雾）；
  布局 = layout.json 里每个 surface 的 x/y/w/h/skin/visible。改皮不改位。
- 验收：A8 组件在 Win+D 后存活且位置不变；A9 切皮肤保持几何与便利贴内容；A10 断网时 weather 静默隐藏不报错弹窗。

## 5. 验收断言（真机逐条跑，脚本化优先）

| # | 断言 | 方法 |
|---|---|---|
| A1 | spike 窗口在 Win+D 与 explorer 重启后仍在桌面层且坐标不变 | 手动 + `taskkill /f /im explorer.exe` 脚本 |
| A2 | 拖入 1 文件后，桌面源消失、框目录出现、journal 有记录 | 自动化脚本（PowerShell 造文件 + 读 SQLite） |
| A3 | 一键复原后文件回到**原盘符原路径**，字节级一致（SHA256 比对 200 文件混合类型样本） | 脚本 |
| A4 | 全部复原幂等：连按两次复原，第二次 no-op 且无报错 | 脚本 |
| A5 | 规则批量跑（桌面 500 杂物→5 框）耗时 < 10s，UI 不冻结 | 计时 |
| A6 | 重启后框布局/规则/journal 完整；关掉软件后桌面回到原生状态（真图标零污染） | 手动 |
| A7 | C7/C9/C10 三种异常路径各触发一次且提示正确、可复原 | 手动构造 |

## 8. 实现进度（滚动更新）
- ✅ v0.1（2026-11-24）：M1 收纳框（拖入+分类+收取桌面）、M1.5 组件（时钟/系统监视/便利贴+4皮肤+布局自动持久化）、
  M2 journal（JSONL，undo_last/undo_all，missing/conflict 报告）、内置 12 断言 selftest。
  代码 nestdesk-app/；spike 结论已固化进 layer.rs。
- ✅ v0.3（2026-11-24）：融合开源能力——控制台五标签（桌面勾选收取/归纳+✨智能推荐方案+organize 兼容 YAML 导入导出/事务批次勾选回滚/Czkawka 式 SHA-256 查重→_重复待清理过 journal 可复原/透明度+皮肤）；selftest 16/16。
- ✅ v0.4（2026-11-24）：双参照融合（vendor/folia AGPL、vendor/deskbox GPL，均只借范式不抄码）——S-A copy-verify 跨卷、S-B 链接拖放下载、S-C 农历时钟（.NET 日历实测正确：2026-11-24=农历十月十六·马年）、S-D 待办组件、S-E Folia 进程托管入口；selftest 16/16、demo 355/355 回归通过；详见 FUSION-PLAN.md。
- ⏳ 待办：托盘+全局快捷键、开机自启、年龄/大小规则、天气倒计时、剪贴板历史。

## 6. 非目标（MVP 明确不做）
Dock/桌面栏、完整音乐播放器（仅留 now-playing 数据接口给 music 组件）、规则 YAML 编辑器、多用户同步、macOS/Linux。
架构上以"另一个桌面层窗口 + sidecar 进程"接入 M3，不做重构级预留。

## 7. 已知边界（说实话的部分）
- ~~WorkerW 嵌入是未公开但十年稳定的机制~~ → **S0 实测结论（2026-11-24）：子窗口路线作废**。
  `SetLayeredWindowAttributes` 对子窗口无效（分层窗口必须是 top-level），挂 Progman/WorkerW 的框会全透明。
  正确架构 = Fences 本尊的做法：**top-level `WS_POPUP | WS_EX_LAYERED | WS_EX_TOOLWINDOW` + `SetWindowPos(HWND_BOTTOM)`**，
  用 2s 定时器兜底（Win+D 会最小化我们 → IsIconic 则 SW_RESTORE + 重新沉底）。已在 spike 上验证：
  清屏后桌面层可见 ✅、普通窗口之上自动沉底 ✅、日志无重挂抖动 ✅。
- **合成黑洞（v0.1 实测，最终定案）**：tauri `transparent:true` 窗口带 `WS_EX_NOREDIRECTIONBITMAP(0x40000)`，
  该样式窗口沉底后完全不参与桌面合成（A/B：PrintWindow 有内容、屏摄/GDI 均无）。
  修复 = 每窗**一次性**去掉 0x40000、补 `WS_EX_LAYERED + LWA_ALPHA(254)`（layer.rs::ensure_layered）；
  **样式只打一次**——每 2s 重写扩展样式 + DWM ACRYLIC backdrop 都会把窗口踢出合成（层层踩坑实录）。
  最终验证（pxcnt.ps1 GDI 读数）：note 区 RGB(199,218,236) < 邻区(209,228,246)，卡片参与合成 ✓。
- Explorer 重启对 top-level 方案天然无父链依赖，Z 序漂移由 2s 钉扎循环兜底。
- 跨权限拖放（从管理员资源管理器拖入）受 UIPI 限制，MVP 不解决，走"框上右键→从此路径导入"兜底。
- 复原的前提是文件仍在本机且未被用户手动删除（C10 报告机制兜底）。
- 待办：M1 用 Tauri 重写窗口层（`windows` crate 调同一组 API，WebView 内容层负责皮肤），真拖放需 `IDropTarget` 桥。
