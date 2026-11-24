# NestDesk v0.4 五视角审查汇总（REVIEW-SUMMARY）

> 基线：HEAD bf29e5f（v0.4）· 日期：2026-11-24
> 状态：**5/5 全部交卷**（功能 / Win32 / 数据 / 架构 / 性能）。
> 原始报告：`REVIEW-feature.md`（20）· `REVIEW-win32.md`（10）· `REVIEW-data.md`（11）· `REVIEW-arch.md`（24）· `REVIEW-perf.md`（15）

## 1. 计数

| 视角 | Critical/P0 | High/P1 | Medium/P2 | Low/Info | 合计 |
|---|---|---|---|---|---|
| 功能对标 UX | 1 | 7 | 12 | 0 | 20 |
| Win32 窗口层 | 0 | 2（高/中高） | 5 | 3 | 10 |
| 数据安全事务 | 2 | 6 | 3 | 0 | 11 |
| 架构代码质量 | 0 | 6（High） | 15 | 3 | 24 |
| 性能稳定 | 0 | 3（高） | 4（中/中高） | 8 | 15 |
| **原始合计** | **3** | **24** | **39** | **14** | **80** |

跨视角重复 4 组（move_any 校验旁路 ×2、find_dups 全读 ×2、journal 重写 ×2、placeholder 乱码 ×2），去重后 **≈61 条独立缺陷**。

## 2. 已由主审亲核实锤（非转述）

| ID | 断言 | 证据 |
|---|---|---|
| F-01（P0） | URL 下载 journal `old` 存的是 URL 字符串 | `fs_core.rs:218` `old: url.to_string()`；undo 必进 missing |
| 数据 P0#1 / FD-H03 | `sha256_of` 返回 None 时校验整块跳过、仍删源 | `fs_core.rs:117` `let src_hash = sha256_of(src);` → `if let Some(ref want)` → `:127` 无条件 remove |
| 数据 P0#2 | `save_url` 子进程无超时 | `fs_core.rs:201` `child.wait()` |
| FD-H01 | 自家导出格式导不回来 | export 写 `- name:` + `exts:`；import 只认 `- createfolder:`/`- folder:`/裸 `name:` 与 `extensions:`（`fs_core.rs:629-638 vs 658-671`） |
| FD-H02 | 推荐方案应用时读空 groups 整体覆盖 | `App.tsx:320` `const merged=[...groups]` + `settings_get` 异步无 guard；`lib.rs:293` `s.groups = groups` 整体替换 |
| FD-H04 | `--demo` 无门控且操作真实桌面 | `main.rs:6-13` 裸分派；`lib.rs:520` `real_cfg()` |
| FD-H06 | 自定义命令未进 ACL，6 窗共享权限 | `build.rs` 仅 `tauri_build::build()`；`grep settings_save gen/schemas/acl-manifests.json` = 0 |
| W-01 | 负坐标原样落盘原样恢复 | `lib.rs` Geom 存 `pos.x/pos.y` 无钳制 |
| W-03 | 每 2s `ShowWindow(SW_RESTORE)` 激活抢焦点 | `layer.rs pin_bottom` |
| F-03 | 透明度滑块拖动不 invoke | `App.tsx` 滑块仅 `setAlpha` 本地状态 |
| F-04 | placeholder 被 JS 转义成 `^L` | `App.tsx:497` 实为 `\f` 控制字符（cat -A 实锤） |

## 3. P0 级（阻断「可复原」核心承诺，v0.5 第一批必修）

1. **F-01 URL 下载不可复原**：`old` 改存本地锚点或 `undo_entries` 识别非本地路径条目 → 明确报「外部下载，无原路」而不是混入 missing。
2. **数据 P0#1 move_any 静默无校验删源**：`sha256_of` None 即 `Err`，禁止进入 copy→delete。
3. **数据 P0#2 save_url 无超时**：`wait_timeout(120s)` + kill + 清 `.part` + 明确错误。
4. **FD-H01 YAML 往返失败**（升 P0：README 宣传的导入导出闭环为空转，且假成功）：import 增补 `- name:` 锚点与 `exts:` 键，lib.rs 不得静默回退。
5. **FD-H02 推荐覆盖用户分组**（升 P0：可致配置丢失）：`applySugg` 先 await settings_get 或后端改 merge 语义。

## 4. 按批次的修复计划（v0.5）

### B1 数据安全（P0，全部带回归断言）
- move_any 校验失败即 Err（含目录跨卷：逐文件校验或明确「目录不校验」并在结果标注）。
- save_url：超时 kill、`create_dir_all(dest.parent())`、删重复 rename、失败清 `.part`、100MB 预检。
- undo journal 原子替换：tmp → rename(journal→bak) → rename(tmp→journal)，失败回滚 bak；serde 错误传播。
- find_dups 改流式 `sha256_of`；`_` 前缀排除收窄为 `_重复待清理` 精确名。
- unique_dest 上限 10000 + MAX_PATH 检查；ingest 目录过滤（或 UI 明确确认）。
- settings .bak 自愈（parse 失败先读 .bak）。

### B2 Win32 窗口层
- W-01 负坐标/屏外钳制到最近显示器工作区（含最小可见面积）。
- W-03 `SW_RESTORE` → `SW_SHOWNOACTIVATE`，并把 IsIconic 恢复与钉扎分离。
- W-04 仅当 `GetWindow(h)==?`/Z 序异常时才重发 HWND_BOTTOM（或降低无条件重设）。
- W-05 STYLED 以 HWND 存活校验清理；样式改写移入窗口线程（或至少在 Tauri 主线程之外做存活检查）。
- W-06 settings 窗口几何纳入 layout；W-02 DPI 单位标注；W-10 shelf `focus:false`。

### B3 功能体验（承诺兑现）
- 托盘 + 全局快捷键（Ctrl+Alt+N 收取 / Ctrl+Alt+H 显隐 / 暂停 shelves）＝ C12。
- shelf 文件行：双击打开、右键「打开所在文件夹 / 播放（配置播放器）」。
- 透明度滑块即时 `apply_alpha` + 保存按钮同 tab；F-04 placeholder 重写。
- 事务 tab 批次展开文件清单（前 50 条）＋单条定位。
- selftest 增补：save_url undo、YAML 往返、目录跨卷、journal 并发（F-18/FD-H05）。

### B4 架构卫生
- build.rs AppManifest 列全部命令 + 按窗口拆 capability（最小权限）。
- theme_set 改 serde_json；统一错误契约（command 返回 Result）；RwLock 中毒自愈。
- import_yaml 组名清洗（禁绝对路径、`..`、`:`、控制字符）；`exts` 字符集统一。
- CSP 收敛 `script-src 'self'`；App.tsx 按 §FD-M10 边界拆分（hooks/features/types）。
- desktop_dir memoize（OnceLock）；GroupRule 归一到单类型；BatchInfo.groups 写真实组名。

## 5. 验收断言（v0.5 门禁）

1. `--selftest` 全绿且输出 JSON；新增 6 项回归（URL undo、YAML 往返、无哈希源拒绝、超时下载、负坐标钳制、分组 merge）。
2. `--demo` 真桌面 355/355 复原不变。
3. YAML：export → import 往返组数/扩展名完全一致。
4. 拖 URL 后 undo：文件移除且提示「外部下载」而非 missing。
5. 冷启动 1s 内点「应用推荐」：分组数只增不减。
6. 双屏拔插 + 负坐标启动：面板全部落在可见工作区。
7. `cargo check` 0 warning（unsafe 块、unused import 清零）。

## 6. 性能视角

实测（Ryzen 7 7435H / 31.7 GB / NVMe）：

| 场景 | debug | release |
|---|---|---|
| collect_desktop @1000 文件 | 4420 ms | **2908 ms**（≈2.9 ms/文件） |
| undo_all @1000 条 | 3244 ms | 3614 ms |
| find_dups 768 MB 同内容 | 35491 ms | **1251 ms（599 MB/s）** |
| find_dups 20k×256B | 8108 ms | 5227 ms |
| snapshot（2k 文件） | 121 ms | 77 ms |
| journal 解析 50k 行 | 515 ms | 83 ms |
| PowerShell 冷启动 | avg **874 ms** | — |

三条最贵问题与对策：
1. **文件搬运税 2.9–4.4 ms/文件**（每文件 ≥4 次 NTFS 元数据 + 杀软 on-create）→ 批内 `HashSet` 唯一名（省 1 次 stat/文件，syscall −25%）。
2. **查重单线程不可取消**，400 GB ≈11 min，`dup_archive` 还会把收件箱重扫一遍 → 路径上限 200/组、归档复用前端已有扫描结果。
3. **全量 snapshot 风暴**：每个动作全量 read_dir + journal 全解析；inbox 完整路径回传 2 万文件 ≈2.2–2.5 MB IPC → 服务端裁剪（60 条/格 + 总数）。
附：`set_alpha`/`pin_bottom` 每 2 s 无条件刷新（DWM 重绘 150 次/分钟）→ 变化才下发；`load_layout` 不读 .bak；selftest 不清临时目录；桌面勾选静默截断 120 → 已纳入 B5。

性能视角同时确认已被本轮修复的项：PowerShell memoize ✔、`--demo` 门控 ✔、查重流式哈希 ✔、锁中毒自愈 ✔、journal 互斥 ✔、URL undo ✔、焦点抢占 ✔、死 HWND 清理 ✔、屏外钳制 ✔。

## 7. 明确不修（本期）

- FD-N01 参数漂移（护栏结论，补注释即可）。
- UIPI 跨权限拖放（SPEC 已列为边界）。
- M10 的完全拆分只做「低风险切块」，不做全量重写。
