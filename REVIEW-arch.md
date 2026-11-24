# NestDesk 架构与代码质量审查（REVIEW-arch）

仓库：`E:\myzcode\.zcode\workspace\draft-25440958-6335-442d-aea5-4f4a4f0cfe02\nestdesk`
范围：`nestdesk-app/src/**`、`nestdesk-app/src-tauri/**`（lib.rs / fs_core.rs / layer.rs / tauri.conf.json / capabilities）
方法：源码逐行审读 + **实测** `cargo check`（离线，tauri 2.12.7）+ 独立探针工程逐字复制 `import_yaml` 做对抗性输入测试 + Tauri 上游源码（registry 内 vendored tauri-2.12.7 / tauri-macros-2.7.6）实证参数/ACL 行为。
严重度：H=High，M=Medium，L=Low。全部 finding 均带 `path:line` 证据。

---

## 总览

| # | ID | 摘要 | 严重度 | 位置 |
|---|----|------|--------|------|
| 1 | FD-H01 | import_yaml 无法解析本应用自己的导出格式；`exts:`/`extensions:` 键不匹配、列表项 `- name:` 无锚点 | High | fs_core.rs:629-638 vs 642-707 |
| 2 | FD-H02 | applySugg 读 stale/空 groups → settings_save 整体替换用户分组表（配置丢失竞态） | High | App.tsx:313-325,364 |
| 3 | FD-H03 | move_any 校验旁路：`sha256_of(src)` 返回 None 时无条件删源文件、跳过校验 | High | fs_core.rs:117-127 |
| 4 | FD-H04 | `--demo`/`--selftest` 无 cfg 门控编入生产 exe；`--demo` 对真实桌面执行收取+复原 | High | lib.rs:520-551, main.rs:10-13 |
| 5 | FD-H05 | selftest 启动路径大量 unwrap、未检查 `ents[0]`、`assert_eq!`/`expect`：journal 不可写时以裸 panic 退出、无 JSON、非零退出码 | High | lib.rs:560-562,583,592,607,646,698,709,710 |
| 6 | FD-H06 | 自定义命令未进 ACL manifest → 运行时 ACL 对本应用命令整体放行；6 个窗口共用同一权限集，least-privilege 不可落地 | High | build.rs:1-3, capabilities/default.json:5-8, tauri webview/mod.rs:2080 |
| 7 | FD-M01 | RwLock 全量 `.read().unwrap()`/`.write().unwrap()` + release `panic=abort`：任意命令或后台线程单点 panic 即杀全进程 | Medium | lib.rs:92,97,216,303; Cargo.toml:32 |
| 8 | FD-M02 | theme_set 手工拼 JSON：`format!("{{\"theme\":\"{theme}\"}}")`，主题名含 `"`/`\` 即破坏 config.json | Medium | lib.rs:180 |
| 9 | FD-M03 | save_url：不建目标目录（依赖 ingest 先创建分组目录）、dest 父目录缺失时两次相同的 rename 均失败、误导性 "download failed"、`.part` 残留、100MB 检查在下载完成后 | Medium | fs_core.rs:178-216 |
| 10 | FD-M04 | GroupRule(lib.rs) 与 GroupRule2(fs_core) 双轨类型；unused import `GroupRule2`/`Mutex`/`UNIX_EPOCH as _U` | Medium | lib.rs:5,11; fs_core.rs:392 |
| 11 | FD-M05 | BatchInfo.groups 恒为「归档」单桶：批次分组明细死数据，TS 永远显示「归档(N)」 | Medium | fs_core.rs:511,514 |
| 12 | FD-M06 | undo 重写整个 journal 文件（读-改-写）与并发 ingest 交错 → journal 条目永久丢失 | Medium | fs_core.rs:274-317 |
| 13 | FD-M07 | find_dups 把同尺寸文件整体 `std::fs::read` 入内存，大文件 OOM | Medium | fs_core.rs:563 |
| 14 | FD-M08 | import_yaml 容错面：单引号 `'weird ext'` 破碎为 `"'weird"`+`"ext'"`；bracket 分支无字符集过滤接受 `[x`/`jpg]`；行内注释使 extensions 整条失效；任意嵌套 YAML 被当分组收编 | Medium | fs_core.rs:643-700 |
| 15 | FD-M09 | import_yaml 产出的组名（空串、`C:/evil`、`../../outside`）无任何清洗直通到 ingest 目录 join（Path::join 绝对路径替换根 / `..` 穿越 inbox 根）；UI 侧同样无校验 | Medium | fs_core.rs:177-178,246; lib.rs:288-307; App.tsx:424 |
| 16 | FD-M10 | App.tsx 单文件职责爆炸：584 行 7 组件、Settings 234 行内含 5 个 tab、~60 处内联样式、接口重复声明、GROUP_ICON 缺自定义组显示 undefined | Medium | App.tsx 全文 |
| 17 | FD-M11 | real_cfg()/desktop_dir() 每次调用都起 PowerShell 解析桌面路径：所有命令均叠加进程启动开销；失败静默回退 OneDrive 假桌面 | Medium | lib.rs:29-40,109-118 |
| 18 | FD-M12 | CSP null：无内容安全策略兜底（与 transparent 窗口叠加风险敞口） | Medium | tauri.conf.json:96-98 |
| 19 | FD-M13 | 错误契约不统一：多数 command 返回裸结构体，出错只能 panic(abort)；仅 dup_scan/music_open 用 Result<String,String> | Medium | lib.rs:122-316 vs 235,406 |
| 20 | FD-M14 | styles.css：`user-select:none` 继承进 textarea，便利贴文本不可选不可复制；night/mist 小字对比度 <4.5 未达 WCAG AA；无 focus 样式 | Medium | styles.css:43,85-89 |
| 21 | FD-M15 | 初始布局碰撞：settings(1080,90,660×780) 初始矩形覆盖 clock/sysmon/note/todo 的初始坐标 | Medium | tauri.conf.json:70-94 |
| 22 | FD-L01 | layer.rs 在 edition 2024 下产生 16 条 E0133 unsafe-call 警告；`#![allow(dead_code)]` 全局压制卫生 lint | Low | layer.rs:1-2,cargo check 输出 |
| 23 | FD-L02 | note_set 静默忽略写入错误；`#[cfg_attr(mobile,...)]` 与 windows-only 项目不符 | Low | lib.rs:165-167;436 |
| 24 | FD-N01 | 【正面结论】TS/Rust 参数名当前无漂移：Tauri 自动 camelCase↔snake_case（keepNewest↔keep_newest 工作正常）——是护栏而非缺陷 | Informational | App.tsx:359; lib.rs:241 |

计数：**High 6 / Medium 15 / Low 2 / Informational 1，合计 24 条**（Critical 0）。
另有编译器实测证据支撑 4 条 hygiene 类子项（fs_core.rs:450,645 `let mut` 冗余；layer.rs 16×E0133）。

---

## High

### FD-H01 — import_yaml 无法解析本应用自己的导出格式（往返失败）
- `nestdesk-app/src-tauri/src/fs_core.rs:629-638`（export_yaml）：导出键为 `groups:` + `- name: X` + **`exts:` [png, jpg]**。
- `nestdesk-app/src-tauri/src/fs_core.rs:658-664`（anchor）：锚点只认 `- createfolder:` / `- create_folder:` / `- folder:` 或裸 `name:`——**`- name:` 形式从不匹配任何锚点**。
- `nestdesk-app/src-tauri/src/fs_core.rs:671`：扩展名键只认 `strip_prefix("extensions:")`，**导出用的 `exts:` 不被识别**。
- 探针实测（逐字复制的函数 + 喂入 export_yaml 原样输出）：
  - `groups:\n  - name: 文档\n    exts: [pdf, docx]\n` → **ERR: 未解析出任何归纳文件夹规则**
- 后果：UI 的「导出 yml → 导入 yml」闭环为空转；用户导出后想回导必然失败。且 lib.rs:207-222 静默返回 current_settings，前端仍显示「导入 N 个归纳文件夹」（N 为现存数），属**静默假成功**。

### FD-H02 — applySugg 竞态导致用户分组表被整体替换
- `nestdesk-app/src/App.tsx:364`：`useEffect(() => { invoke<AppSettings>("settings_get").then((s)=>{setGroups(...)}) }, [])` —— 异步加载，无 await 保护。
- `nestdesk-app/src/App.tsx:320-325`：`const merged = [...groups]`（此时可能仍为 `[]`）→ `invoke("settings_save", { groups: merged, alpha })`。
- `nestdesk-app/src-tauri/src/lib.rs:288-307`：`settings_save` 内 `s.groups = groups;` 是**整体替换**，不是 merge。
- 链路：冷启动磁盘慢 / settings_get 未返回时点击「✨ 智能推荐方案」→「应用推荐」，则 `merged` 仅含建议项（如 CAD图纸/字体），**图片/文档/安装包等既有分组全部消失**。与「状态在异步完成前被读取并回写」同型竞态（H02 变体见 FD-H02 同节，alpha 编辑在 save 早于加载完成时被丢弃，App.tsx:366-371）。

### FD-H03 — move_any 校验旁路：源不可读即无条件删除
- `nestdesk-app/src-tauri/src/fs_core.rs:110-129`：
  - :114 rename 失败 → :117 `let src_hash = sha256_of(src);` 可能为 `None`（File::open 失败，如 AV 瞬时占用/共享冲突）；
  - :118-120 copy_tree 成功；
  - :121 `if let Some(ref want) = src_hash { ...校验... }` —— **None 时整个校验块被跳过**；
  - :127 `let _ = if src.is_dir() { remove_dir_all } else { remove_file };` **无论如何删除源**。
- 即：源文件不可哈希（但可复制）的场景下，复制结果不经任何校验即删源，属于数据静默损坏/截断窗口。同段对**目录**跨卷搬运也完全不走 SHA 校验（`sha256_of` 只处理文件）。设计意图（:109 注释「copy then SHA-256 verify then delete」）与 :121-127 实现不符。

### FD-H04 — demo_run/selftest 编译进生产 exe；`--demo` 触碰真实桌面
- `nestdesk-app/src-tauri/src/main.rs:6-13`：`--selftest` → `nestdesk_app_lib::selftest()`；`--demo` → `nestdesk_app_lib::demo_run()`。
- `nestdesk-app/src-tauri/src/lib.rs:520-524`：`demo_run` 用的是 **`real_cfg()`**（真实 desktop、`user_home/Nest/收件箱`、真实 journal），随后真实 `collect_desktop` 移动用户桌面全部散落文件，最后才 `undo_all` 复原。任何本地进程均可发现 exe 路径并执行 `nestdesk.exe --demo` 触发真实文件搬运；中途崩溃则桌面处于已收取状态（journal 可恢复，但非显然）。
- `lib.rs:520/554` 两个入口均为普通 `pub fn`，**没有 `#[cfg(test)]`/feature 门控**：发布二进制永久携带该面（体积+攻击面+误触风险）。

### FD-H05 — selftest 失败模式：panic 代替 JSON，CI 门禁失真
- `nestdesk-app/src-tauri/src/lib.rs:560-562`：`let _ = fs::create_dir_all(&desktop);` —— **错误被吞**；随后：
  - :583 / :592 / :607 / :635 / :674-675 / :693-694 / :706-707 共 10+ 处 `fs::write(...).unwrap()`：temp root 不可写（磁盘满/TMP 只读）时**在首个 report 记录之前 panic**。
  - :646 `let victim = ents[0].new.clone();` —— **Vec 未检查索引**。关联事实：`ingest` 对 `append_entries` 的错误全部吞掉（fs_core.rs:256-258），journal 写不进但 IngestResult 仍报 moved=120，随后 read_entries 返回空 → `ents[0]` 直接越界 panic。
  - :698 `assert_eq!(bl.len(), 2);`、:710 `.expect("dup set")`、:709 `.unwrap()`：任一不满足即裸 panic。
- 与 `Cargo.toml:32` `panic = "abort"`（release profile）叠加：进程立即死亡，stdout 不产出任何 JSON，`--selftest` 的退出码语义（0/1）退化为任意非零码且无结构化报告——**自动化门禁不可信**。
- 另：`main.rs:7` `selftest(); return;` 之后还有 `return`，与 lib.rs:722 自带的 `process::exit` 冗余。

### FD-H06 — 自定义命令未进 ACL：运行时整体放行 + 全窗口共享权限集
- `nestdesk-app/src-tauri/build.rs:1-3`：只有 `tauri_build::build()`，**未调用 `AppManifest::new().commands(&[...])`**（上游文档明确要求，见 tauri-build-2.7.6/src/lib.rs:47-48）。
- 实证：`src-tauri/gen/schemas/acl-manifests.json` 仅含 `core/core:app/core:event/...` 键，**grep `settings_save`/`shelf_ingest` 命中 0**。
- 上游门控（vendored tauri-2.12.7/src/webview/mod.rs:2080）：ACL reject 只在 `(plugin_command.is_some() || has_app_acl_manifest || !is_local)` 时生效。本项目 `has_app_acl_manifest=false` 且本地调用 → **所有自定义命令对任何本地 webview 无条件放行**（authority.rs:462-489 resolve_access 对无条目命令返回 None，但该 None 因上述门控而被跳过）。
- `nestdesk-app/src-tauri/capabilities/default.json:5-8`：单 capability 覆盖 6 个窗口、仅 `core:default`。即便将来补上 AppManifest，也是 6 窗全量相同：**clock/note/todo 小面板可达 `music_open`（外开任意已配置播放器进程，lib.rs:406-429）、`dup_archive`（跨目录移动）、`settings_save`**——least-privilege 在结构上不成立，需按窗口拆 capability + 补 build.rs。

---

## Medium

### FD-M01 — RwLock 全部 unwrap，panic=abort 下单点崩全局
- `lib.rs:92` `r.read().unwrap()`（current_settings，被后台钉扎线程每 2 秒调用，lib.rs:486-491）；
- `lib.rs:97` save_settings_file 内 `r.read().unwrap()`；
- `lib.rs:216` `*r.write().unwrap()`（yaml_import）、`lib.rs:303` `*r.write().unwrap()`（settings_save）；
- `lib.rs:449` `.expect("NestDesk 启动失败")`；
- `nestdesk-app/src-tauri/src/layer.rs:131` `CPU_PREV.lock().unwrap()`（get_sysinfo 每 2 秒 tick 调用）。
- 一旦锁中毒（任一持锁者 panic）→ 后续任意一次 invoke 或轮询 tick 直接 abort 全进程。建议统一 `unwrap_or_else(|e| e.into_inner())`。

### FD-M02 — theme_set 手工拼接 JSON，主题名未转义
- `nestdesk-app/src-tauri/src/lib.rs:180`：`` format!("{{\"theme\":\"{theme}\"}}") ``。TS 侧目前只允许 4 个枚举值（App.tsx:7），但 command 本身接受任意字符串：任何含 `"` 或 `\` 的值（如 `"x\"]}}"`）都会**永久性写坏 config.json**（music_set 读它会直接失败，直到手工删文件）。同仓已有正确示范：lib.rs:351-358 music_set 用 `serde_json` 读写。请照抄。

### FD-M03 — save_url：目录假设、双重重命名、误导报错、大流量浪费
- `fs_core.rs:178` `unique_dest(&cfg.inbox_root.join(&group), &name)` 与 `fs_core.rs:179` tmp 同目录，但**无 `create_dir_all(dest.parent())`**（对比 move_any fs_core.rs:111-113 有）。分组目录仅由 ingest 创建 → 「首次归档某扩展名前先拖 URL」必然失败，报错却是笼统的 "download failed"（fs_core.rs:202-205），实际是目标目录不存在。
- `fs_core.rs:214-216`：`if std::fs::rename(&tmp,&dest).is_err() { let _ = std::fs::rename(&tmp,&dest); }` —— 两条完全相同的语句，第二条无意义；若首次失败，剩余 `.part` 永不清除（下载失败/超限路径都有 remove，唯独 rename 双败路径没有）→ inbox 组内 `.part` 垃圾无界累积。
- `fs_core.rs:207-213`：100MB 守卫在 Invoke-WebRequest 完整落盘**之后**才检查（浪费带宽/时间；上限本身成立）。

### FD-M04 — 类型/导入卫生
- `lib.rs:5` use 列表含 `GroupRule2`，但 lib.rs 从不使用该名字（只经由 SuggestPlan 字段间接出现）——编译器实测 warning：`unused import: GroupRule2`。
- `lib.rs:11` unused import `Mutex`（实际在闭包内局部再 import，lib.rs:473）。
- `fs_core.rs:392` `use std::time::UNIX_EPOCH as _U;` —— 显式压制的死导入；now_ms（:72）直接用了 ::std::time::SystemTime 却不用这个别名。
- 结构双轨：`lib.rs:43-44` 定义 `GroupRule{name,exts}`，`fs_core.rs:437` 又定义 `GroupRule2{name,exts}`，两组几乎同型类型分居两模块（AppSettings.groups 用前者，suggest_plan 用后者），应归一到 fs_core/model 或 types.rs 并单一出口。

### FD-M05 — 批次分组明细是常量死数据
- `fs_core.rs:509-512`：`*entry.1.entry(String::from("归档")).or_insert(0) += 1;` —— 所有批次的组分布恒写入名为「归档」的单桶，真实组名从未进入 BatchInfo.groups（:503/:514）。
- 前端 `App.tsx:453` 据此渲染「归档(N)」——对 5 个 tab 的「事务」页是**纯噪音**：用户永远看不到「图片×3 / 文档×1」。功能意图（fs_core.rs:503 注释隐含分组统计）与实现不符。

### FD-M06 — journal 读-改-写与并发 ingest 交错丢条目
- `fs_core.rs:274-317`（undo_entries）：`all = read_entries`（一次性全量读）→ 回放选中批次 → :312-314 **用 fs::write + rename 整体重写 journal**。
- `fs_core.rs:149-159` append_entries 独立 append。并发场景：undo 读取后、重写前，另一线程 ingest 追加新行（ingest 吞掉自己的 append 错误也无感知）→ **该新条目在重写中永久丢失，而对应文件已被移动**，复原能力被摧毁。命令均为 untyped invoke，无任何互斥/文件锁。建议写 journal 的专用 Mutex 或 journal.db。

### FD-M07 — 查重整文件入内存
- `fs_core.rs:563`：`let h = std::fs::read(p).map_err(...)?; // 大文件也硬读，内存换取确定性`。同一尺寸桶内所有等大文件同时驻留内存：收件箱里 3 份 2GB 同尺寸镜像 → 瞬间 6GB+ 映射压力，OOM 风险由「收件箱内容」这一用户可控输入触发。建议流式分块哈希 + 大小桶上限。

### FD-M08 — import_yaml 容错面的具体碎点（探针实测，函数逐字复制自 fs_core.rs:642-707）
| 输入向量 | 解析结果 | 说明 |
|---|---|---|
| `'weird ext'`（单引号扩展名） | `["'weird", "ext'"]` | :674 只 `trim_matches('"')`，单引号未剥离 |
| `[png, ,jpg], extra: [x]` | `["[x", "jpg]", "png"]` | bracket 分支（:673-684）**无字符集过滤**，而 `- ` 单项分支（:696）有 `alphanumeric\|\.'.' 限定 → 同类约束两套标准 |
| `[a:b, ok.txt]` | `["ok.txt"]` | `a:b` 被静默丢（无告警） |
| `extensions: [zip] # comment` | ERR（整条 extensions 失效 → 组无 exts 被丢弃 → Err） | :673 `body.ends_with(']')` 被行内注释破坏 |
| `server:\n  name: FromServer\n  extensions: [zip]` | OK，收编无关 YAML 的嵌套 name/extensions | 解析器不认结构，任何对象的这两个键都会被当成「归纳文件夹」写进 settings.json |
| `name: A\r\nextensions: [zip]\r\n`（CRLF） | OK | trim() 吃掉 `\r`，此向量通过 |
| `  - name: 文档`（列表项，最常规 YAML） | ERR（见 FD-H01） | anchor 不匹配 `- name:` |

另有：行内 `#` 只整行识别（:656），组名后缀注释会原样进入 name（见探针 `文档 # 含注释`）。该手写行级解析器无任何 YAML 语义/缩进概念（所有缩进被 trim 抹平），与真实 YAML（如多文档、流式、`key: value` 标量）边界模糊，误解析面如上表。

### FD-M09 — 组名无清洗直通到归档目录 join
- `fs_core.rs:177-178`：`group_resolve` 结果直接 `cfg.inbox_root.join(&group)`；`fs_core.rs:246` 同型 `inbox_root.join(&g)`。
- 探针证实 import_yaml 可产出：`name: C:/evil`（OK 解析）、`name: ../../outside`（OK 解析）、`name: ""` + ext（OK：`("",["zip"])`）。
- `PathBuf::join` 对绝对路径**替换根**（落出 Nest 目录）、对 `..` 穿透 inbox 根——即「归纳文件夹」名等价于目录注入位。Rust 侧唯一防线是 `settings_save` 的 `groups.is_empty()` 拒绝（lib.rs:289-291），**组名内容零校验**；前端同样无校验（App.tsx:424 直接绑定任意输入，`save()` App.tsx:367 只 trim 非空）。至少要禁绝对路径、`..`、`:`、控制字符。

### FD-M10 — App.tsx 单文件职责爆炸（含若干伴生缺陷）
- 体量：584 行，7 个组件；Settings 单组件 234 行（App.tsx:290-523）且内含 5 个 tab 的全部状态（desk/pick/groups/alpha/saved/msg/sugg/batches/bpick/dups/dsha/keepNew/player 共 13 组 state）+ 各自 invoke 编排。
- 伴生缺陷一：`interface GroupRule/AppSettings` **重复声明**（App.tsx:276-277 与 :287-288 完全相同）——TS declaration merging 静默吞掉，靠 IDE 才能发现。
- 伴生缺陷二：`GROUP_ICON`（App.tsx:36-39）只含内置 9 组；推荐/自定义组（CAD图纸、字体…）渲染为字面量 `undefined`（App.tsx:258 `{GROUP_ICON[g.group]}`）。
- 伴生缺陷三：~60 处内联 style 对象（App.tsx:390-521 全程），styles.css 只承载了 4 个 widget 的样式层，Settings 页完全绕开样式表，主题变量靠手写 `var(--...)` 复制粘贴——改皮肤要双处同步。
- 伴生缺陷四：App.tsx:497 占位符文字疑似损坏：`placeholder="播放器 exe 完整路径，如 E:\myzcodefoliafolia.exe"`（"folia" 应为 Folia 路径的半拼接产物）。
- 拆分建议边界（按现有块切即可低风险落地）：
  1. `hooks/useTheme.ts` ← App.tsx:10-26；
  2. `features/Clock.tsx` ← :42-69、`Sysmon.tsx` ← :76-111、`Note.tsx` ← :114-139、`Todo.tsx` ← :535-572、`Shelf.tsx` ← :154-273（连同其 DragDrop/URL-drop/undo 状态机）；
  3. `features/settings/{index.tsx,DesktopTab.tsx,GroupsTab.tsx,TransactionsTab.tsx,DuplicatesTab.tsx,MoreTab.tsx}` ← Settings 各 tab 块（:403-513）；
  4. `types.ts` ← 全部 interface（顺手消重声明）；`styles.css` 保留 token+widget 层，补 Settings 专用类名替代内联样式。

### FD-M11 — real_cfg()/desktop_dir() 每次调用重跑 PowerShell
- `lib.rs:29-40`：每次 `desktop_dir()` 都 `Command::new("powershell")` 拉起一个 powershell.exe 求 `[Environment]::GetFolderPath('Desktop')`。
- `lib.rs:109-118` real_cfg() 在每个 command 入口被调：`shelf_snapshot`(:123)、`ingest`(:127)、`collect_desktop`(:131)、`undo_last/all`(:135/139)、`snapshot` 链、`desk_items`(:189)、`suggest_plan`(:195)、`export_yaml`(:203)、`save_url`(:315)、`dup_scan/archive`(:237/248)、`batches/undo_batches`…—— **每次 UI 动作额外叠一个 powershell 冷启动开销（典型百毫秒级）+ JSON 重组**，结果在同一会话内其实不变，却无 OnceLock 缓存（init_settings 已有先例 :64-88，同款缓存姿态不一致）。
- `lib.rs:30-31` 注释自认回退风险：PowerShell 失败/返回空 → `USERPROFILE\Desktop`（OneDrive 重定向时的假路径），此后所有收取静默归错目录，且 real_cfg 与 settings_defaults（lib.rs:55-62）之间无一致性校验。幂等问题同源：desktop/inbox/journal 三路径每请求重建 Vec/HashMap，当前规模可容忍，但路径解析应 memoize。

### FD-M12 — CSP null
- `nestdesk-app/src-tauri/tauri.conf.json:96-98`：`"security": { "csp": null }` —— webview 无任何内容安全策略。窗口只加载本地 dist，实际暴露面有限，但叠加 transparent 窗口（:21 等）与拖放 URL 进入 webview 的路径（App.tsx:185-204）后没有纵深兜底。上游默认策略应为 `default-src 'self'; img-src 'self' asset: http://asset.localhost data:; style-src 'self' 'unsafe-inline'; script-src 'self'` 一类的显式值（至少把 script-src 收敛到 'self'）。

### FD-M13 — 错误契约不统一，可读性差
- 现状矩阵：`dup_scan`→`Result<Vec<DupSet>, String>`（lib.rs:235）✓、`music_open`→`Result<String,String>`（lib.rs:406）✓；其余 ~24 个 command 全部返回裸结构体/裸 String：**任何 io 错误只能靠 panic（release abort）表达，TS 侧 `catch {}` 全空**（如 refresh/chains 在 App.tsx 多处），用户零反馈。结合 FD-M01/FD-H05，建议统一 `type ApiError = ...` + thiserror，把 `?` 而非 unwrap 贯穿（Rust 侧共有 27 处 unwrap/expect，除测试外的启动路径占相当比例）。

### FD-M14 — styles.css 两处实质问题
- `nestdesk-app/src/styles.css:43` `.card { user-select: none; }` 继承链作用到 `.note-body textarea`（:85-89 **未显式恢复 `user-select:text`**）→ 便利贴内文本无法用鼠标选区/复制。对照同文件 :45-46 drag region 的刻意分区，此为遗漏而非设计。
- 对比度（按 WCAG 相对亮度手算，4 套主题 × --dim 作为 11px 次级文字）：
  - mist：`#5d7082` on `rgba(225,233,240,.88)` ≈ **4.19:1** < 4.5 ✗
  - night：`#7e91c0` on `rgba(15,20,40,.88)` ≈ 4.47:1 边缘不过 ✗
  - sunset：`#c98f6f` ≈ 5.1:1 ✓；contrast ✓。
  - 且全文无任何 `:focus-visible`（键盘可达性缺口，输入框依赖浏览器默认 outline，在 decorations:false 的透明面板里可能不可见）。

### FD-M15 — 初始窗口矩形互相覆盖
- `tauri.conf.json:70-83` todo(1180,40,260×300)、:28-41 clock(1420,40,240×150)、:42-55 sysmon(1420,210,…)、:56-69 note(1420,430,…)。
- `tauri.conf.json:85-94` settings(1080,90,660×780)：其矩形 x∈[1080,1740]×y∈[90,870] 与 clock/sysmon/note/todo 四者的初始矩形全部相交（如 clock 区 x∈[1420,1660]×y∈[40,190] ∩ settings ⊂ y∈[90,190] 非空），且 settings 在配置顺序中最后创建 → 默认叠在最上层，**首次启动看不到右列三个小面板**。layout.json 恢复只在有历史时规避（lib.rs:452-457）。坐标应按多显示器最小宽度推一遍，或首启即落一份 layout.json。

---

## Low

### FD-L01 — layer.rs unsafe 卫生 + 全局 allow
- edition=2024（`nestdesk-app/src-tauri/Cargo.toml:6`）下实测 `cargo check` 输出 **16 条 E0133**：`layer.rs:74,78,88,96,101,103,104,110,113,114,116` 处 `pub unsafe fn` 体内的 Win32 调用需要内层 `unsafe {}` 块（unsafe_op_in_unsafe_fn 默认 warn）。
- `layer.rs:1-2` `#![allow(dead_code)]` 全局压制：整个文件的死代码/死 extern 声明（如 MemoryStatusEx 中大量从未参与计算的字段：dw_memory_load、ull_total_pagefile 等）都不会再报警——抑制面过宽，应收敛到具体项。

### FD-L02 — note_set 静默丢字；mobile 属性残留
- `lib.rs:165-167`：`fn note_set(text)` → `let _ = fs::write(...)`：磁盘满/占用时便利贴**静默丢字**，用户以为已保存（前端还乐观显示「✓ 已保存」于 500ms 后，App.tsx:120-126，与服务端结果无关）。
- `lib.rs:435-436` `#[cfg_attr(mobile, tauri::mobile_entry_point)]` + `Cargo.toml:14-15 crate-type 含 cdylib/rlib`：windows-only 应用保留 mobile 构建面，属无效配置噪音。

---

## Informational

### FD-N01 — TS ↔ Rust 参数名当前无漂移（Tauri 自动转换实证）
- 上游机制：tauri-macros-2.7.6/src/command/wrapper.rs:501 `key = key.to_lower_camel_case();`（默认 rename_all="camelCase"，:69-74 选项定义）。即 Rust snake_case 参数 ↔ JS camelCase 参数由宏自动桥接。
- 本仓活例：`dup_archive(shas, keepNewest)`（lib.rs:241）↔ App.tsx:359 `invoke("dup_archive", { shas, keepNewest })`；`settings_save(groups, alpha)`、`plan_suggest(threshold)`、`todo_toggle(i)` 等全部对齐。**当前无 live mismatch。**
- 但该转换只作用于**顶层 command 参数名**，嵌套结构体字段不做转换（故 AppSettings/GroupRule 等字段恰好全是单词名，侥幸不炸）。建议：在 types.ts 顶部注释固化「参数必须 camelCase」约定，并在 CI 加一条 invoke 名清单 lint，防未来加 `keep_newest` 时有人按直觉传 snake_case。

---

## 复现方法与证据可信度

1. **编译警告**（fd-M04/L01 依据）：
   `cd nestdesk-app/src-tauri && CARGO_NET_OFFLINE=true cargo check --message-format short`
   实测输出摘录（exit 0，16 warnings）：`unused import: std::time::UNIX_EPOCH as _U (fs_core.rs:392)`、`unused import: GroupRule2 (lib.rs:5)`、`unused import: Mutex (lib.rs:11)`、`variable does not need to be mutable (fs_core.rs:450, 645)`、`E0133 ×16 (layer.rs)`。
2. **YAML 探针**（FD-H01/M08/M09 依据）：独立工程逐字复制 fs_core.rs:642-707 的 import_yaml（无改动），喂入对抗向量，输出见 FD-M08 表格与 FD-H01/FD-M09 各条（ERR/OK 均为程序原样输出，非推断）。
3. **Tauri ACL/参数机制**（FD-H06/FD-N01 依据）：直接读 registry 内 vendored 源码 `tauri-2.12.7/src/webview/mod.rs:2036-2110`、`ipc/authority.rs:462-489`、`tauri-macros-2.7.6/src/command/wrapper.rs:480-510`；项目侧 `gen/schemas/acl-manifests.json` 内容与 `capabilities/default.json`、`build.rs` 三方对照（grep settings_save/shelf_ingest = 0）。
4. **WCAG 数字**（FD-M14）为手算相对亮度近似值，非仪器测量；mist/night 是否构成合规缺陷建议用 axe/lighthouse 复核。