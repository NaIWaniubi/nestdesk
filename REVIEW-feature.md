# 归巢 NestDesk — 功能对标与 UX 审查

> 角色：功能对标与 UX 审查员（只找毛病，每条落到具体交互缺口）
> 参照：`vendor/deskbox/README.md`（Features，L48–77）+ `vendor/folia`；自述 `README.md`/`SPEC.md`/`FUSION-PLAN.md`
> 仓库：`E:\myzcode\.zcode\workspace\draft-25440958-6335-442d-aea5-4f4a4f0cfe02\nestdesk`（v0.4, HEAD bf29e5f）
> 方法：阅读源码逐条判断；commit 说明宣称的能力单独核验 UI 通路。
> 计数：**P0 = 1 条 / P1 = 7 条 / P2 = 12 条，共 20 条 findings**。

记号：`path:line` 为 NestDesk 仓库内相对路径（`nestdesk-app/src/...`、`nestdesk-app/src-tauri/src/...`）；vendor 文件全路径在 E:\myzcode 下。

---

## P0 — 直接违背核心承诺，必须先修

### F-01 ｜ S-B URL 下载「同样过 journal，随时一键复原」实际不可复原
- **差距一句话**：URL 拖放进收纳框下载归档后，日志 `old` 字段存的是 URL 字符串而非本地路径，「全部复原/勾选复原」回放时会去把文件移到 `https://…` 这种非法路径，`move_any` 必失败 → 该条目被记成 `missing`，文件永远回不去原位。
- **证据**：
  - `nestdesk-app/src-tauri/src/fs_core.rs:218` —— `entries.push(Entry { batch, old: url.to_string(), new: new.clone(), ts });` `old` 直接是下载 URL。
  - `fs_core.rs:274-304` `undo_entries` 对每条做 `move_any(new_p, old_p)`，当 `old_p = Path::new("https://…")` 时必然失败（`rename` 跨设备/非法路径），落入 `missing.push(e.new)`。
  - 对比正常归档 `ingest` 的 `old` 是真实源路径（`fs_core.rs:250`）。
- **影响**：README.md L28 与 FUSION-PLAN.md L27 都宣传「下载归档……过 journal」、随时可复原。用户拖一张图进格、点「全部复原」想撤掉，结果 toast 报「N 个已被删除找不回」—— 实际是我们的日志写错了 `old`，不是用户删的。这击穿了 v0.4 最招牌的卖点。
- **建议**：S-B 下载时 `old` 不应存 URL，应存一个稳定锚点，例如 `~/.nestdesk/_download-source.jsonl` 里记一条「本次下载自 `<url>`，落点 `<new>`」，复原时反向 = 把 `new` 标记为「无原路可还原（外部下载）」并从收件箱清出，而不是假装能回到 URL。最低限度：在 `undo_entries` 里识别 `e.old` 不是绝对本地路径（不以盘符/UNC 开头）的 Entry，直接判为不可还原并给专门提示文案，别混进「已被删除」误导用户。

---

## P1 — 高频日常交互缺口，应排在 v0.5 前

### F-02 ｜ 文件 widget 完全零交互：无右键打开、无「打开所在文件夹」、无嵌套栈展开、无重排，只读扁平文本
- **差距一句话**：DeskBox 的文件 widget 是真·Windows 文件夹视图（右键菜单含图标、单列 Open-with、空白处 New/View/Sort by 子菜单、QuickLook 空格预览、可手动重排 tile、可展开嵌套栈）；NestDesk 的收纳框只是「分类计数 + 一行顿号拼接的文件名」，没有任何点击/右键/展开动作。
- **证据**：
  - `nestdesk-app/src/App.tsx:256-261` —— `<div className="files">{g.files.slice(0,60).join("、")}</div>`，纯文本，无 onClick、无 onContextMenu。
  - 右键入口完全缺失：全仓 grep `onContextMenu|contextMenu|rightclick` 在 src 下 0 命中。
  - 嵌套不可见：`fs_core.rs:347-389` `snapshot()` 只扫 `cfg.inbox_root` 的一级子目录（=分类格），每个格里再只扫一层，不递归 —— 拖进去的子目录落到「文件夹」格后内部文件永远不显示。
  - 无重排/无视图切换：shelf 无任何排序/网格/列表切换控件。
- **影响**：DeskBox Features §1（L50–52）整段（「Single-column right-click menus with inline icons, Explorer-ordered items, real "Open with" app associations (including UWP), blank-area "New / View / Sort by" submenus, native ZIP compress/extract, and Spacebar QuickLook preview … reorder tiles manually, expand nested folder stacks」）在 NestDesk 一个都没有。用户最自然的「在收纳框里双击打开刚归档的文件 / 右键在资源管理器里定位」做不到，每次都要切出去到 `~/Nest/收件箱/<类别>/` 翻。
- **建议**：最小可用集 ——(a) shelf 内每个文件名加双击 → Rust 命令以默认程序打开；(b) 右键两项：「📂 打开所在文件夹」（`explorer.exe /select,<path>`）；(c) 文件夹格子点开走递归树（snapshot 加 `children` 或后端单独 `tree_list`）。ZIP/QuickLook 可后置。

### F-03 ｜ 透明度滑块标注「实时生效」，但拖动根本不触发任何 apply；保存按钮又不在当前 tab
- **差距一句话**：滑块标题写「面板透明度（实时生效）」，实际 onChange 只更新 React 本地 state，不调任何 Rust 命令；要真正写入得去**归纳 tab** 点「💾 保存归纳设置」——而那个按钮根本不在滑块所在的「更多」tab。
- **证据**：
  - `App.tsx:486` 标题文案「面板透明度（实时生效）」。
  - `App.tsx:490-491` 滑块 onChange 仅 `setAlpha(...)`，无任何 invoke。
  - `App.tsx:511` 自相矛盾地补一句「改后点『💾 保存归纳设置』即写入并 ≤2 秒生效」—— 即承认不是实时。
  - `App.tsx:436` 「💾 保存归纳设置」按钮位于 **归纳 tab**，而滑块位于 **更多 tab**（L484 起）。用户在「更多」调完滑块，看不到保存按钮，只能靠猜去切 tab。
- **影响**：标签与现实相反，违反用户心智；跨 tab 操作断裂，常见「我滑了但没反应」工单。≤2 秒「生效」走的是 lib.rs:485-492 的钉扎线程每 2s 读 settings，所以即便保存了也要等下一个 2s tick 才看见变化，体感又慢半拍。
- **建议**：滑块 onChange 直接 `invoke('settings_save', { groups, alpha })`（或专门的 `apply_alpha` 命令），实时写 SETTINGS 即可（钉扎线程 2s 内会读到并 set_alpha）。或者把保存按钮复制一份到「更多」tab。文案改成「拖动即时写入」。

### F-04 ｜ 中英文案缺陷：播放器路径 placeholder 被 JS 转义吃成乱码，且示例路径根本打不开
- **差距一句话**：placeholder 字面量 `"…如 E:\myzcode\folia\folia.exe"` 在 JS 双引号串里被当成转义序列处理（`\f`→换页符、`\m/\o/\l/\i/\a/\d`→丢弃反斜杠），渲染出来变成 `E:myzcode\x0colia\x0colia.exe`，是条**坏掉的示例路径**。
- **证据**：
  - `App.tsx:497` `placeholder="播放器 exe 完整路径，如 E:\myzcode\olia\olia.exe"`（注意 Read 工具把 `\f` 渲染成了字面换页——源文件里确实是 `\f` 两个字符）。
  - 同仓多处用 `\\` 正确转义（如 lib.rs 中 powershell 参数走 stdin），唯独这处用户可见 placeholder 用了单反斜杠。
- **影响**：用户照抄这条示例会得到一个含控制字符、打不开的路径；`music_open`（lib.rs:406-429）随后走 `Err("播放器路径不存在: …")`，用户一头雾水。这是「混了中英变量名/报错」类问题的具象——不是语言混杂，而是**转义没统一**，典型踩坑。
- **建议**：改用正斜杠 `/` 示例或双反斜杠 `E:\\myzcode\\folia\\folia.exe`，或模板串 `` `E:\\myzcode\\folia\\folia.exe` ``。顺手全局 grep 单反斜杠 JS 字符串里的 Windows 路径。

### F-05 ｜ 「事务」tab 的批次只有时间/计数/类别，没有每批文件清单、不能点定位到某个文件
- **差距一句话**：DeskBox 有 organization history 的**预览 + 撤销**两件事一起做（N 条预览→点哪批看清单→撤销那批）；NestDesk 事务 tab 的批次行只显示 `{time} · {count} 个文件 · {groups}`，看不到任何文件名，也不能点某行跳到该批涉及的文件位置。
- **证据**：
  - `App.tsx:446-462` 事务 tab，批次 map 仅有 `(b.ts).toLocaleString() · b.count 个文件 · b.groups.map(...)`（L453），无 onClick、无文件列表。
  - 后端其实有完整数据：`fs_core.rs:505-518` `list_batches` 只聚合了 `(batch, ts, count, groups)` ——**连文件级 detail 都已经在 journal Entry 里**（`Entry{old,new,...}`），是 UI 层根本没暴露。
- **影响**：多批次累积后，用户想「撤销昨天那批含 X 文件的批次」只能盲勾；选了批次才发现里面有不想动的项，得重开再来一遍。「preview before undo」是 DeskBox Features §1 L53 明确点名的能力，NestDesk 有 undo 没 preview，体验落后一档。
- **建议**：批次行加 `onClick` → 展开/弹层列出该批所有 `(old→new)` 配对，并支持单条勾选粒度还原（已有 `shelf_undo_selected` 支持 batch 级，可扩展 entry 级或在展开列表里给「从该批排除这几条」选项）。

### F-06 ｜ 「浏览器右键另存为」落到桌面后没有「一步归类」通路，拖/点二选一还都不快捷键
- **差距一句话**：浏览器的图片「另存为」选到桌面 = 文件落地 `Desktop`；归类要么手拖到 shelf 窗、要么开控制台到「桌面」tab 勾选 + 点「收取勾选的 N 个」，**没有快捷键、没有自动嗅探新落地的桌面文件、没有托盘**。
- **证据**：
  - 全仓 grep `tray|shortcut|global.?hotkey|register.?shortcut` 在 `src`+`src-tauri/src` 0 命中（见下方「无全局快捷键」一节汇总）。
  - SPEC.md:92 自认待办：「待办：托盘+全局快捷键、开机自启、年龄/大小规则、天气倒计时、剪贴板历史。」
  - `App.tsx:230` ⚡收取桌面是全量吸入（无差别收所有桌面文件）；`:304-310` collectPicked 是控制台里的勾选流，两条都非快捷。
- **影响**：DeskBox 的 Desktop Organization workbench 有「automatic background filing for newly settled items」（Features §1 L53），NestDesk 没有任何定时/触发式收取。用户每次新下载到桌面都得动手，而这是产品的核心场景。实测任务流：浏览器另存为→桌面→拖 shelf=一次拖动（OK 但要瞄准那个小窗）或点收取=全量误伤。
- **建议**：(a) 上托盘菜单 + Ctrl+Alt+N 全局快捷键一键「收取桌面散落」（P1 必做）；(b) 提供一个轻量「监听桌面新增 mtime>now-300s 的文件」选项，按现有分组规则自动吸入并 journal（SPEC 提到「自动收件箱定时吸入」是 M1 规划，但 MVP 一直没落地）。

### F-07 ｜ Alt+D 全局搜索、Alt+V 剪贴板历史、自然语言日期待办、快捷捕获置顶便签、Todo↔捕获互转——全部缺席
- **差距一句话**：DeskBox 把这两大件当 feature 主菜推（Features §2 L57–59），NestDesk 一概没有，连「半吊子」都算不上 —— 完全是空白。
- **证据**：
  - 全局搜索：lib.rs:439-447 invoke_handler 列表里没有任何 search 命令；全仓 grep `everything|alt.?d|search.*popup` 0 命中。FUSION-PLAN.md:45 明确把 Everything 集成列为非目标（「依赖第三方软件，非本机必需」）——但 Alt+D 这个交互范式本身（命令面板式单键唤起）并不强制绑定 Everything，NestDesk 连个壳都没留。
  - 剪贴板历史：全仓 grep `clipboard|paste` 0 命中。SPEC.md:92 列为 M5，未启动。
  - 自然语言日期 Todo：Todo 组件（App.tsx:533-572）只有 text/done/toggle/del，无 created_at 展示、无 date 字段、无自然语言解析（对比 DeskBox 「natural-language date/time/recurrence input」「overdue reschedule bar」「task reminders」「color markers」L59）。
  - 快捷捕获 / pinned note 弹出窗：Note 组件（App.tsx:113-139）是内嵌桌面卡片，无弹出独立窗口、无 multi-note 合并。对比 DeskBox 「popout pinned note window, multi-note merge, live Markdown folder mirroring」。
  - Todo ↔ QuickCapture 互转、Nudge 系统：均未实现。
- **影响**：DeskBox Features §2（L55–60）六条里 NestDesk 命中 0 条。这些是「桌面效率工具」立足的高频场景，缺席等于产品叙事少了一整块。
- **建议**：分两档。MVP 前至少上：(1) 极简命令面板（不绑 Everything，本地文件名模糊匹配即可）作为 Alt+D 雏形；(2) Todo 组件加一个 `date` 字段 + 文本「明天」「下周一」识别。剪贴板、Nudge 可继续 M5/不做。

### F-08 ｜ 音乐入口埋在第 5 个 tab 倒数第二块，S-E 只有「打开外部播放器」一行，没有 SMTC/歌词/拖入即播
- **差距一句话**：S-E（FUSION-PLAN L36-38）在 UI 上就是「更多 → 音乐（S-E）→ 输入 exe 路径 → ▶打开音乐」，所有「深方向」(S-F SMTC 卡、歌词同步、拖入歌曲即播) 都停在文档没落代码；且入口埋在五标签页之末、又夹在透明度滑块和皮肤切换之间，发现性极差。
- **证据**：
  - `App.tsx:484-502` 音乐控件在「更多」tab 内、透明度块之后、皮肤之前。
  - `lib.rs:404-429` `music_open()` 只做一件事：`Command::new(player).creation_flags(DETACHED…).spawn()`。
  - FUSION-PLAN.md:37 自己写明「拖拽歌曲文件到 shelf 同样支持『播放』快捷（右键菜单暂缺，先放置=收藏进格）」—— 即拖歌进 shelf 只是归到「音频」格，**不会播**。
  - 全仓 grep `smtc|media.?session|transportcontrol|now.?playing|spectrum|lyric` 0 命中。
- **影响**：对照 Folia 多平台聚合（local/Navidrome/在线）、华丽歌词动画、多 mod 体系；对照 DeskBox Music「controls Windows media sessions with album-color ambience, audio spectrum visualization, LRCLIB/local .lrc synced lyrics」(Features §2 L60)。NestDesk 这版音乐能力 = 1 行启动按钮 + 一条坏路径示例（见 F-04）。发现性：用户在桌面层的 6 个面板里找不到任何「音乐」入口，得知道有「控制台→更多→第二块」才能找到。
- **建议**：(a) 入口前置：桌面层 sysmon 同族直接加一个常驻「正在播放」迷你卡（即便空着也展示「点击在更多→音乐启动」），发现性 +80%；(b) S-F 降级为桌面小卡：PowerShell `Get-SMTC` 读当前曲目即可（见 FUSION-PLAN L41 现成的降级通道方案，没做）；(c) 拖歌到 shelf 至少给个右键「▶ 用配置播放器播放此文件」。

---

## P2 — 应有但不紧急，或文档承诺与现实的小落差

### F-09 ｜ 农历时钟确实显示（核验通过），但缺「节气」也缺「自定义轮替背景」
- **差距一句话**：commit bf29e5f 宣称的「农历时钟」是真做了、`.NET ChineseLunisolarCalendar` 也确实返回中文串；但对照 DeskBox Glance「date, weekday, lunar calendar, solar terms, and custom rotating backgrounds」还差两块。
- **证据**：
  - 真显示：`App.tsx:51-65` 调用 `lunar_now`，结果渲染在 clock-date 副行；`lib.rs:327-345` PowerShell 走 `[System.Globalization.ChineseLunisolarCalendar]`，`format!("{}{}月{} · {}年", …)` 输出如「十月初十六 · 马年」。核验通过。
  - 缺节气：`lib.rs:319-345` 脚本只取 year/month/day/leap，**没查 `GetSolarTerm`**。
  - 缺轮替背景：Clock 卡片背景由 CSS 决定，无按日切换图机制。
- **影响**：minor。节气一年就二十几个，对自定义用户有彩蛋价值。
- **建议**：在 lunar_now 脚本里追加 `$c.GetSolarTerm($d)`，命中时把节气名拼在农历后面即可，半天的事。

### F-10 ｜ 透明度滑块保存后真的会应用（核验通过），但 α=30 与 254 视觉差异不明显 + 拖到极端值无提示
- **差距一句话**：commit be1bcee 宣称「alpha=60 经 GetLayeredWindowAttributes 回读验证 =60」属实；应用通路 OK。但滑块 30–255 全程单调且无「过透/接近全隐」的提示，且默认值 250 与最高 255 肉眼几乎没差。
- **证据**：
  - 应用通路：`lib.rs:485-492` 钉扎线程每 2s 读 `current_settings().alpha[label]` → `layer::set_alpha(ptr, alpha)`；`layer.rs:73-79` 实写 `SetLayeredWindowAttributes(h, 0, a, LWA_ALPHA)`。闭环成立。
  - 滑块：`App.tsx:489` `Math.round((alpha[k] ?? 250)/255*100)%` 仅显示百分比，无极端值说明。
- **影响**：minor UX。用户调到 100% 以为满值，实际 255≠100% 视觉。
- **建议**：百分比换算改 `(alpha-30)/(255-30)*100` 或干脆用 α 值显示；α<80 时加一句「接近隐形」toast。

### F-11 ｜ URL 下载落点后 shelf 视图可见（核验通过），但分类用的是 URL 文件名而非真实 Content-Type
- **差距一句话**：commit 说「链接拖放下载归档」核验：`shelf_save_url` 把 `.part` rename 到 `inbox/<group>/<name>`，随后 `refresh()` 重新跑 `shelf_snapshot`，文件确会出现在对应分类格下。
- **证据**：
  - `fs_core.rs:173-178` 从 URL 末段裁名字 `clean` → `group_resolve(&PathBuf::from(&name))` → 落点；`:214-216` rename 入位；`:217-220` 写入 journal。
  - `App.tsx:203` `refresh()` 在 handleUrlDrop 末尾必调 → shelf 刷新。
  - 分类 `fs_core.rs:177` 用 `name` 含不含 `.` 判定组别 → 一个 `img?w=800&size=large` 形状的 URL 会因 URL 末段带 `.jpg` 正确进图片格，但若服务器给 `Content-Disposition: attachment; filename="x.png"` 且 URL 是无扩展名短链，就会被错归「其他」或直接 `download-{ts}.bin`。
- **影响**：minor。正确率够用但偶发误分类，且 `save_url` 写死 100MB 上限（fs_core.rs:208），DeskBox 没显式上限口径，行为不一致。
- **建议**：(a) 下载阶段先读 `Content-Type`，无扩展名时用 `application/octet-stream`→「其他」、`image/*`→图片等兜底；(b) 上限改为 config（默认 200MB 与 FUSION-PLAN L28 一致，现状 fs_core.rs:208 写死 100MB，与规划不符）。

### F-12 ｜ 无托盘图标、无全局快捷键、无法一键暂停 shelves（C12 兼容矩阵里承诺了）
- **差距一句话**：SPEC C12 写「全屏游戏/演示 → 托盘一键『暂停 shelves』」；SPEC M1 写「托盘图标 + 全局显隐快捷键」；现实是两样都没有，没有任何系统托盘位置 icon，也没有键盘绑定。
- **证据**：
  - `tauri.conf.json` 的 `app.windows[]` 只有 6 个 label，全是 popup；`security.csp:null`；无 tray config。
  - 全仓 grep `tray|TrayIconBuilder|MenuItem|register_shortcut|Shortcut` 0 命中（main.rs/lib.rs 全文）。
- **影响**：SPEC 兼容矩阵 C12 自检项无法通过；用户全屏玩游戏时没法一键收起桌面层。属 P1/P2 边界，定 P2 因为它不阻断核心归档，但既然 SPEC 里白纸黑字写了 C12，自评应当诚实扣分。
- **建议**：上 `TrayIconBuilder` + 注册 `Ctrl+Alt+H` toggle 全部 surfaces 显隐；至少把「全部复原」加到托盘菜单（高频兜底）。

### F-13 ｜ 天气组件整件缺失（DeskBox 已捆绑四套图标包 + MSN/Open-Meteo 双源）
- **差距一句话**：DeskBox Glance/Weather 是默认 feature（N_features §2 L60）；NestDesk SPEC.md:67 列为 M1.5 v2 联网件，承诺「open-meteo 免 key」「失败自动降级隐藏不报错（A10 验收）」，至今无天气组件代码。
- **证据**：
  - SURFACES 列表 `lib.rs:15` 只有 `["shelf","clock","sysmon","note","todo"]`，无 weather/countdown。
  - `App.tsx:575-583` route 表无 weather label → 启动无窗。
  - SPEC.md:92 待办「天气倒计时」未消项。
- **影响**：常规缺口，但属于 SPEC 已明列的 v2，不违约。P2。
- **建议**：与 Todo/Note 同构扩一个 `weather` surface，open-meteo JSON + 14 天数据装 clock-card 即可，工作量 < 0.5 天。

### F-14 ｜ 多显示器记忆缺「硬件身份绑定」，拔插显示器布局可能错位
- **差距一句话**：DeskBox「Remembers per-display-topology arrangements bound to monitor hardware identities」（Features §4 L72），NestDesk layout.json 只存 `{label:{x,y,w,h}}` 绝对坐标（lib.rs:260-279），主屏变更/拔屏时无优雅折叠与归位逻辑。
- **证据**：
  - `lib.rs:252-279` `Geom{x,y,w,h}` + HashMap，纯坐标无 monitor id。
  - SPEC C3 期望「框跟随所属屏坐标，不跳没」，但实现未区分屏。
- **影响**：外接屏拔掉再插回，位置对不上；分辨率/DPI 变化同理（C2）。
- **建议**：layout 加 `monitor_edid` 字段，启动时 `EnumDisplayDevices` 取匹配；否则按主屏归一化坐标。

### F-15 ｜ 皮肤只有 4 套 + CSS 变量，无 Mica/Acrylic 材质选择（DWM 材质层）
- **差距一句话**：DeskBox「Choose Mica or Acrylic materials, DWM rounded corners…」(Features §4 L71)；NestDesk 只有 RGBA+LWA_ALPHA 分层合成 + DWM 圆角，无 Mica/Acrylic。
- **证据**：
  - `layer.rs:83-89` 仅设 DWMWCP_ROUND；`layer.rs:95-106` 只确保 WS_EX_LAYERED + LWA_ALPHA，注释明确说 ACRYLIC backdrop 与分层互斥（实测教训），所以永久放弃。
- **影响**：观感比 DeskBox 朴素一档，但分层合成路线已够用。属已知取舍，P2 备注而非改进项。
- **建议**：可在 settings 里加「背板材质 = 纯色/Acrylic」，对单面板单独启用 backdrop（避开所有窗口同时设导致踢出合成的问题）。不改也无伤大雅。

### F-16 ｜ 收纳框无法「映射到现有文件夹而不搬入」（DeskBox 核心范式差异）
- **差距一句话**：DeskBox 明确「map existing desktop and disk folders in place without moving them」（Features §1 L50），Migrating 才是可选；NestDesk 永远是「移动文件到 `~/Nest/收件箱/<类别>/`」，无 in-place 映射模式。
- **证据**：
  - SPEC.md §2 决策表 L17：「归档语义 = 真移动文件」；README.md L17：「真移动文件；目标默认 `%USERPROFILE%\Nest\<框名>\`」。
  - `fs_core.rs:224-260` ingest 唯一路径 → move_any，无 symlink/mount-point 分支。
- **影响**：用户已有的桌面分类体系（如 `Desktop/工作/` 目录）无法「挂」进 shelf 当作分类格显示，只能把文件搬进 Nest。属架构承诺 vs DeskBox 范式的根本差异，不是 bug，但要文档明示，避免用户期待落差。
- **建议**：README 加一句边界说明；或 v0.6 加「映射现有文件夹（只读视图）」模式：shelf 勾选 group 后走 `cfg.roots.push(existing_dir)`，snapshot 同时扫 roots 与 inbox。

### F-17 ｜ 「全部复原」二次确认是 3s 内再点同一按钮，但 armed 状态无视觉绑定到按钮位置变化
- **差距一句话**：两步确认设计合理（防误触），但 armed=true 期间按钮文字变「⚠ 确认全部复原?」，无倒计时进度条，且如果用户在 armed 期间点了别的按钮或切 tab 再回来，3s timer 仍在但 UI 不再提示 → 容易以为已经确认过了。
- **证据**：
  - `App.tsx:235-245` armed 二态切换 + setTimeout(3000)；clearTimeout 在切 armed=false 与 act 时。
  - `armTimer.current` 切 tab 后仍 active，但 Settings 组件 mount 时不读 armed/恢复状态。
- **影响**：minor。误触风险存在但「全部复原」本身已被二次确认吃掉大部分误操作。
- **建议**：armed=true 时给按钮加倒计时数字（如「⚠ 确认 3s」每秒更新）或在切 tab 时 clearTimeout 重置。

### F-18 ｜ selftest 通过 ≠ 用户能感知；selftest 里 16 项不覆盖「UI 是否看得到 X」链路
- **差距一句话**：selftest 16/16（README.md L51-52）与 demo 355/355 都是 fs_core 纯后端链路验证，对「某命令是否真被前端调用 / 某 UI 元素是否存在 / 某参数能否到达后端」完全无断言。F-01 这种「journal old 写错类型」在 selftest 里就不会失败，因为 selftest 只用本地路径做 ingest，没测 save_url 的 undo。
- **证据**：
  - `lib.rs:553-723` selftest 全程 `Config{desktop,inbox_root,journal,groups}`，只调 `ingest`/`undo_*`/`suggest_plan`/`find_dups` 等，**从不调 `save_url`**，因此 url-as-old 这个 bug 逃出 selftest。
  - demo_run 同理只走 collect_desktop。
- **影响**：selftest 全绿被当卖点反复写（README/SPEC/FUSION-PLAN），给人一种「链路稳了」错觉，实测输入类型不全。
- **建议**：selftest 增加 `save_url` 用例：用 httpbin/file 服务起一个临时 HTTP 源，下载后立即 undo_all，断言 restored==0 且 missing 包含「URL 不可还原」类标记，而非普通文件缺失。

### F-19 ｜ 控制台「桌面」tab 勾选表 120 上限 + 单行列表头省略，肉眼难定位长文件
- **差距一句话**：`desk.slice(0,120).map` 截断只显示前 120；且行内名字 `whiteSpace:noWrap + ellipsis`，无路径列、无关联定位按钮，超过 120 个散落文件时后续不可见且无法单独勾选。
- **证据**：
  - `App.tsx:407` `desk.slice(0,120).map`。
  - `:410` `<span style={{ flex:1, overflow:hidden, textOverflow:'ellipsis', whiteSpace:'nowrap' }}>{d.name}…`
- **影响**：桌面有几百杂物时，桌面 tab 形同虚设；用户得开资源管理器先筛 —— 违背「桌面 tab 兜底」意图。
- **建议**：(a) 加搜索框过滤 desk；(b) 加路径末段列或 hover 显全路径 tooltip；(c) 上限改 500 + 分页条；或直接接 E-01 的 fuzzy 搜索。

### F-20 ｜ 中英混排零星不统一：「撤销/复原」「上批/上一批」「桌面散落/桌面剩余」三种说法并存
- **差距一句话**：同一个 undo 语义在三处有三种叫法，且 `撤销` / `复原` 各自又有近义词。用户翻 README 见「一键复原」，进 UI 见「复原上批」「全部复原」「勾选批次复原」，控制台 footer 又写「设置随窗口位置自动记忆」这种半截话。
- **证据**：
  - README.md L5-9 「一键复原」；App.tsx:234「↩ 复原上批」、:243「⎌ 全部复原」、:458「↩ 复原勾选批次」；fs_core.rs 返回值 key 名 `restored`/`renamed`/`missing`（英文 serde 字段，但用户看到的 toast 用中文混英文数字）。
  - toast 文案如 `（… ${r.renamed} 挪旁）`（App.tsx:343）——「挪旁」罕见。
- **影响**：低。主要是术语一致性，影响专业感。
- **建议**：定一本小 glossary（复原 / 缺失 / 归类 / 归纳 / 收取），全局替换一致化。

---

## 顶部无对应 interaction 缺口的「已核验通过」项（不计 finding）

| commit 宣称 | 核验结论 | 证据 |
|---|---|---|
| 「农历时钟 .NET 日历实测正确」 | ✅ 真显示，且每小时刷 | App.tsx:51-65, lib.rs:327-345 |
| 「透明度滑块 ≤2 秒生效」 | ✅ 真有通路（钉扎线程每 2s set_alpha）| lib.rs:485-492, layer.rs:73-79 |
| 「α=60 回读 =60」 | ✅ 属实 | lib.rs:78 (lib.rs:491 clamp) + layer.rs:78 |
| 「URL 下载落点后 shelf 能看到」 | ✅ snapshot 会扫到该文件（F-11 仅分类精度问题） | fs_core.rs:214-220 + App.tsx refresh |
| 「354/355 文件全链路 demo」 | ✅ 仅后端链路，前端无断言（见 F-18） | lib.rs:520-551 |
| 「selftest 16/16」 | ✅ 通过但用例不全（F-01 未覆盖） | lib.rs:553-723 |

---

## 最高优先级 Top 3（给决策层）

1. **F-01（P0）**：S-B URL 下载归档后无法复原 —— `fs_core.rs:218` 把 `old` 写成 URL 字符串，undo 必落 missing，直接击穿 v0.4 招牌「随时一键复原」。修法小、危害大、不修则 S-B 等于埋雷。**v0.5 必修第一项。**

2. **F-02（P1）**：文件 widget 零交互（双击/右键打开/定位所在文件夹/嵌套展开/重排全缺）—— 这是与 DeskBox 同品类对决时差距最大、最日常的痛点，也是「桌面收纳工具」立身之本。当前 `~/Nest/收件箱` 在 shelf 里只是一段顿号拼接的纯文本，用户必须切出去到资源管理器操作，等于收纳框只是个分类计数面板。

3. **F-03（P1）**：透明度滑块「实时生效」标签与实现相反、保存按钮又落在另一个 tab —— 误导性文案 + 跨 tab 操作断裂，是典型的「看似能用、实际要绕一圈」体验裂缝。这类缺陷最伤信任，用户会觉得「我以为调了，其实没调」，且根因只需在 onChange 加一行 invoke。

---

## 附：与 DeskBox Features 段的命中率自查（供规划参考）

| DeskBox Features §1–§5 | NestDesk v0.4 命中 |
|---|---|
| §1 真实文件夹 widget / mapping / 多源拖放 / 栈展开 / 工作台 / 自动归档 | 仅「多源拖放（本地+URL）」✓；其余 ✗（F-02, F-16, F-06）|
| §2 Alt+D 搜索 / Alt+V 剪贴板 / NL 待办 / 快捷捕获 / pinned popout / Glance·Weather·Music(SMTC+歌词) | 仅「农历时钟」半命中；其余 ✗（F-07, F-08, F-13）|
| §3 自定义 widget 引擎 | ✗（MVP 非目标，合理）|
| §4 capsule / Mica·Acrylic / 多显示器硬件记忆 | ✗（F-12, F-14, F-15）|
| §5 备份/便携快照/云备份 | 部分（settings.json+.bak ✓；便携导出/云备份/WebDAV ✗）|

**总结**：NestDesk v0.4 在「本地归档+一键复原+journal」这一条主线上完成度尚可且经核验；但对照 DeskBox 已宣布的「日常效率」三层（文件交互效率 / 全局唤起效率 / 媒体体验），NestDesk 目前基本空白或仅占位。若定位仍是 MVP 自用，建议 v0.5 收敛三条 P0/P1；若要对标竞品叙事，F-02/F-06/F-07 三件必须先做。