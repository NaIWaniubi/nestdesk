# 归巢 NestDesk

Windows 桌面分区收纳 + 改装小组件 + **一键复原**（自用 MVP v0.7）

## v0.7 新增：磁盘空间可视化 + 炫酷系统监视 + 6 套皮肤

### 磁盘空间（WinDirStat 范式，控制台「空间」tab）
- **squarified treemap**：矩形面积严格等于文件/目录大小（实测 7.1MB→55.6%、5.7MB→44.4%）
- 目录按**主导文件类型**着色（不再所有目录同色），文件按扩展名着色，20+ 类配色
- 点击任意目录**下钻**，面包屑逐级返回；悬停显示完整路径/大小/占比
- 按类型图例可点选过滤；磁盘容量条（所有盘符 used/total/%）
- 扫描限额：120k 节点 / 5s 软上限，超限标记「已截断」；自动跳过
  `node_modules/.git/target/dist/.venv/__pycache__` 等 14 类噪音目录
- 实测：项目目录 135 文件/1.1MB/235ms（跳过 4 个噪音目录）；桌面 2.56GB/5623 文件 5s 截断
- **只读分析，从不删除/移动任何文件**

### 系统监视（btop/bottom 范式）
- CPU / 内存各一条 **60 点时序 sparkline**（2s 采样 = 2 分钟趋势，渐变填充 + 发光描边）
- 数值按负载变色（>60% 琥珀、>85% 红）
- **磁盘环形图**（GetDiskFreeSpaceExW，64 位、支持 >2TB）+ **电池环形**（充电中/插电/低电三色）
- 实测真实读数：CPU 31%、内存 27.5GB/87%、C: 93%、充电中 69%

### 视觉升级
- 皮肤 4 → **6 套**：新增「极光 Aurora」（青绿→紫径向渐变）与「赛博 Cyber」（品红霓虹）
- 卡片加内发光 + 顶部高光玻璃分层；主题切换 0.45s 渐变
- 修复：控制台窗口此前从不应用 `data-theme`（所有主题变量为 undefined），现已全局应用

## v0.6 新增：桌面监听 + 托盘快捷键 + 资讯雷达

### 桌面自动归档（opt-in，默认关）
控制台「更多」→ 勾选「开启监听」。**只搬开启之后新出现的文件**：
开启那一刻桌面上的所有文件被记为基线，永不自动搬走；新文件需连续两轮（约 10s）指纹一致才搬；
`.crdownload/.part/.tmp` 等下载中临时文件与隐藏文件一律跳过。同样过 journal，可一键复原。
实测：357 个基线文件零移动，新建文件 10s 内自动进格，undo 后字节级归位。

> 事故与修复记录见 `PLAN-v0.6.md` §4：初版把首轮快照当基线，曾在开启后把 354 个历史文件全部搬走，
> 已用 journal 复原 354/354（missing=0），并新增 `--undo-all-offline` 纯命令行救援入口防复发。

### 托盘 + 全局快捷键
托盘菜单：⚡ 收取桌面 / ↩ 复原上批 / ⏸ 暂停面板 / ⚙ 打开控制台 / ✕ 退出。
全局快捷键：`Ctrl+Alt+N` 收取桌面 · `Ctrl+Alt+H` 显隐全部面板 · `Ctrl+Alt+Z` 复原上批。
「暂停面板」会隐藏 6 个桌面面板并停止钉扎（全屏游戏/演示场景），恢复后位置与透明度不变。

### 资讯雷达（自接 API 定时收资讯）
控制台「雷达」tab：
- **源**：任意 RSS/Atom URL，或任意 JSON API（自动识别数组位置与 `title|name|headline`、
  `link|url|html_url`、`summary|description` 字段名）；内置 HN 头条 / GitHub 趋势 / 阮一峰周刊 / 少数派
- **关键词过滤**：任一命中即收（空 = 全收）
- **定时抓取**：10–1440 分钟可调，后台线程执行；结果落 `radar.json`（+.bak 自愈），保留最近 200 条，按 link 去重
- **消费**：每条可「→ 待办」一条条转成 todo；桌面「📡 资讯雷达」卡片显示最新 6 条（只读权限）

实测：真实抓取 Hacker News 10 条；重复抓取 +0（去重生效）；关键词 `rust/ai` 过滤后无关项 0。

## 归档与复原（v0.5 起）

文件拖进收纳框 → 自动按类型归档到 `~/Nest/收件箱/<类别>/`，每一次移动都记进事务日志；
任何时候点「复原上批 / 全部复原 / 勾选批次复原」，文件按原路放回原位（字节级，可验）。
**浏览器链接拖进收纳框直接下载归档**（DeskBox 多来源范式）；下载条目复原 = 撤销下载（不误报 missing）。
桌面层组件：时钟（农历·生肖，Glance 思路）/ 系统监视 / 便利贴 / **待办**，4 套皮肤，
窗口位置自动记忆（layout.json + .bak，损坏自愈，屏外坐标自动拉回可见区）。
跨卷搬运走 copy→SHA-256 校验→删源（DeskBox verify-commit 模式）；**哈希不可得即拒绝搬运，绝不无校验删源**。
收纳框内文件**双击打开、右键定位所在文件夹**。

## v0.5：五视角审查 80 条 → P0/P1 已修
原始报告 `REVIEW-feature.md`（20）/ `REVIEW-win32.md`（10）/ `REVIEW-data.md`（11）/ `REVIEW-arch.md`（24）/ `REVIEW-perf.md`（15），汇总 `REVIEW-SUMMARY.md`。

- **数据安全**：`move_any` 校验失败或源不可哈希一律 abort 且保留源；目录跨卷逐文件哈希比对；
  `save_url` 120s 硬超时 + kill + 清 `.part` + 建目标目录；journal 改 tmp→bak→原子替换（失败回滚）；
  journal 读改写与 append 互斥；查重改流式 64KB 哈希（原整文件读入内存）；`_` 前缀误跳收窄为 `_重复待清理`。
- **YAML 往返**：`- name:` + `exts:` 与导出自洽，**导出→导入严格互逆**；组名禁 `/ \ : ..` 与控制字符（防目录注入）。
- **窗口层**：`SW_SHOWNOACTIVATE` 替代 `SW_RESTORE`（Win+D 后不再抢焦点）；死 HWND 条目自动清理；
  屏外/负坐标钳制；控制台纳入几何记忆；alpha/Z 序**只在变化时下发**（原每 2s 无谓重合成）。
- **最小权限**：`build.rs` 声明 AppManifest，6 窗口拆 6 份 capability——收纳框不能调 `music_open`/`dup_archive`/`settings_save`。实测 22 项授权矩阵全通过。
- **体验**：透明度滑块即时预览 + 独立保存；事务批次可展开文件清单；桌面勾选列表超 120 项可「显示全部」；
  `user-select:text` 恢复文本可复制；`focus-visible` 键盘可达；night/mist 对比度提到 WCAG AA；CSP 收敛到 `script-src 'self'`。
- **selftest 20/20**（v0.5 时；v0.6 扩到 26/26），自清临时目录，panic 也输出 JSON。

## 音乐（S-E：Folia 进程托管，非抄码）
控制台「更多」标签 → 填 Folia 路径（github.com/chthollyphile/folia-major）或其他播放器 exe → 「▶ 打开音乐」。
Folia 为 AGPL-3.0，NestDesk 未引入其源码；宿主关系=进程级托管。
注：Folia 官方仓库在 github.com/chthollyphile/folia-major（用户提供的 cht-hollyphile 拼写会 404）。

文件拖进收纳框 → 自动按类型归档到 `~/Nest/收件箱/<类别>/`，每一次移动都记进事务日志；
任何时候点「复原上批 / 全部复原 / 勾选批次复原」，文件按原路放回原位（字节级，可验）。
桌面层小组件：时钟 / 系统监视(CPU·内存·电池) / 便利贴，4 套皮肤（◐ 循环切换），
窗口位置自动记忆（layout.json + .bak 双备份）。

## 控制台（第五个窗口「归巢 · 控制台」）
五个标签页：
1. **桌面**：散落文件勾选表 + 每行推荐归类列 → 一键收取勾选项；
2. **归纳**：归纳文件夹增删改（兜底格不可删）+ ✨智能推荐方案（扩展名分布分析）
   + 应用推荐 + organize 兼容 YAML 导入导出；
3. **事务**：journal 批次列表 + 勾选批量回滚；
4. **查重**（融合 Czkawka）：收件箱递归 SHA-256 查重，勾选组归档非最新版到
   `_重复待清理/` —— **同样过 journal，随时一键复原**，省空间又零风险；
5. **更多**：四面板透明度滑块（30–255，≤2 秒生效）+ 皮肤切换。
设置持久化 `%APPDATA%\NestDesk\settings.json`（+.bak）。
实测：自定义分组走真桌面 354 文件链路（43 入「论文」格 + 311 兜底 → 全复原）；
alpha=60 经 GetLayeredWindowAttributes 回读验证 =60。

## 架构一句话
所有面板都是 top-level `WS_POPUP|LAYERED|TOOLWINDOW` 窗口 + `HWND_BOTTOM` 沉底 +
2 秒钉扎循环（Win+D/最小化自动回魂）——即 Fences 的同款窗口层；
文件操作全部过 journal（JSONL），复原=逆序回放，从不动真图标、从不清注册表。

## 开发运行
```bash
cd nestdesk-app
npm install            # 国内镜像：见 .npmrc（npmmirror）；cargo 走 rsproxy（~/.cargo/config.toml）
npm run tauri dev      # 开发模式，四个桌面面板直接出现
npm run tauri build    # 发布 exe
```

## 自检（不碰真实桌面，纯 temp 目录，跑完自清）
```bash
cargo run --bin nestdesk-app --manifest-path src-tauri/Cargo.toml -- --selftest
```
覆盖 **28 项**断言：批量归档、7 类扩展名分类（含大写/未知/中文文件名）、跨批同名冲突 (2) 后缀、
已删文件的 missing 报告、全部复原字节级一致、幂等重复复原、undo_last 批次粒度、
智能推荐、勾选批次复原、查重归档可复原、URL 下载 undo 撤销下载、YAML 导出→导入往返、
YAML 拒绝路径穿越组名、批次分组显示真实组名、
**监听基线文件不被搬走 / 新文件两轮后收 / 临时文件跳过**、**监听无新文件不产生空批次**、
**雷达 RSS 解析+关键词过滤 / Atom href / 通用 JSON 字段识别 / 存储 .bak 自愈**、
**磁盘扫描大小/文件数/扩展名聚合/跳过 node_modules**、**盘符容量 used+free=total**。

## 救援入口（事故急救，不启 GUI）
```bash
cargo run --bin nestdesk-app --manifest-path src-tauri/Cargo.toml -- --undo-all-offline
```
只读 journal 并全部复原，输出 JSON 证据（restored/missing/desktop_after/inbox_after）。
用于批量误搬后 GUI 不方便启动的场景；实测 354/354 复原、missing=0。

## 真实桌面全链路演示（需显式确认，会移动真实桌面文件）
```bash
NESTDESK_DEMO=1 cargo run --bin nestdesk-app --manifest-path src-tauri/Cargo.toml -- --demo
```
实测记录：355 个散落文件 → 7 分类格 → undo_all 355/355 全复原，missing=0。
（未设 `NESTDESK_DEMO=1` 时 `--demo` 拒绝执行并以退出码 2 结束；release 构建默认不含测试入口，需 `--features harness`。）
环境变量 `NESTDESK_NOPIN=1` 可关闭桌面层钉扎（调试用）。

## 目录
- `spike/` — S0 桌面层可行性验证（零依赖裸 Win32 FFI，结论见上级 SPEC.md §7）
- `nestdesk-app/src-tauri/src/layer.rs` — 桌面层 & 系统信息 FFI（零第三方依赖）
- `nestdesk-app/src-tauri/src/fs_core.rs` — 归档/分类/journal/复原核心（纯 fs，可独立测试）
- `nestdesk-app/src-tauri/src/lib.rs` — Tauri 命令 + 布局持久化 + 钉扎线程 + selftest
- `nestdesk-app/src/App.tsx` — 四面板单入口（按 window label 路由）
- `../SPEC.md` — 规格：兼容矩阵 C1–C12 / 验收断言 A1–A10 / 阶段切片

## 已知边界
- 从「以管理员身份运行」的资源管理器拖入会被 Windows UIPI 拦（用「⚡收取桌面」兜底）
- OneDrive 云端占位文件（文件未落地）会被跳过——journal 机制保证这类操作永远不会丢文件
- 复原依赖文件未被你自己手动删除/挪走；缺失项会在界面明示而不是假装成功
