# NestDesk Windows/Win32 桌面层正确性审查

审查范围：`nestdesk-app/src-tauri/src/layer.rs`（166 行）、`nestdesk-app/src-tauri/src/lib.rs` 的 `run()`（436–517 行，启动恢复循环、SURFACES、钉扎线程）、以及 `tauri.conf.json` 窗口配置。
审查方式：只读静态走查 + 仓库既有实测记录（SPEC.md §7）交叉验证。以下按严重度排序，每条带 `path:line` 证据。

## 严重度分布

| 级别 | 数量 | 条目 |
|---|---|---|
| 高 | 1 | W-01 |
| 中高 | 2 | W-02, W-03 |
| 中 | 3 | W-04, W-05, W-06 |
| 低中 | 2 | W-07, W-08 |
| 低 | 2 | W-09, W-10 |

---

## W-01【高】负坐标/屏外矩形原样落盘且原样恢复——面板整体消失且用户无法找回

**位置**
- `nestdesk-app/src-tauri/src/lib.rs:497-503`（采样，无合法性校验）
- `nestdesk-app/src-tauri/src/lib.rs:505-512`、`268-279`（脏检查后写入 layout.json，含 `.bak` 双份）
- `nestdesk-app/src-tauri/src/lib.rs:452-457`（启动恢复，原样 set_position/set_size）

**证据**
采样直接取 `Geom { x: pos.x, y: pos.y }`（lib.rs:498），没有任何 clamp 或显示器边界校验；恢复侧对 `layout.json` 的 x/y 也不做任何过滤（lib.rs:454 `set_position(Position::Physical(PhysicalPosition::new(g.x, g.y)))`）。仓库内已实出现过 `rect=(-21333,-21333)`：这是窗口处于最小化/被系统挪出虚拟桌面时的典型坐标族（最小化窗口 `GetWindowRect` 报 `-32000,-32000`，Tauri `outer_position()` 对 iconic 窗口同样返回该占位值）。
另外采样与钉扎在同一次 tick 内串行执行（先 pin 496、后采 497），跨线程消息泵存在延迟：若 `pin_bottom` 里的 `ShowWindow(SW_RESTORE)`（layer.rs:114）尚未由窗口管理器生效，`outer_position()` 可能仍在读取 iconic 占位坐标，随后被保存为"静止位置"。

**触发**
1. Win+D 期间恰在采样节拍被 Tauri 主线程/webview2 生命周期抢拍；
2. 拔掉副屏后再开 NestDesk（Windows 不保证把该 rect 拉回可见区，负偏移常被保留）；
3. 用户把面板拖过虚拟桌面的负坐标边界（多屏负坐标系拖拽常见）；
4. DPI 变化后旧物理坐标换算到新坐标系落在屏外。

**后果**
五个面板（shelf/clock/sysmon/note/todo 全部在 SURFACES，lib.rs:15）共用同一份 layout.json，任何一次污染会同时"消失"。由于 skipTaskbar + decorations:false + WS_EX_TOOLWINDOW，用户既看不到任务栏入口也不能 Alt+Tab 到它，唯一逃生是手工删 `%APPDATA%\NestDesk\layout.json`。

**修法**
- 恢复前用虚拟屏幕包围盒校验（`GetSystemMetrics(SM_XVIRTUALSCREEN/SM_YVIRTUALSCREEN/SM_CXVIRTUALSCREEN/SM_CYVIRTUALSCREEN)`），越界则 clamp 到最近显示器工作区或回退 tauri.conf.json 默认；
- 采样时跳过 iconic 状态（`IsIconic` 为 0 才落盘），避免 Win+D 竞拍污染；
- 落盘同样做校验，与恢复同一函数。

---

## W-02【中高】多屏/DPI/分辨率切换零处理，物理像素几何跨次启动单位不一致

**位置**
- `nestdesk-app/src-tauri/src/lib.rs:452-457`（恢复用 `PhysicalPosition/PhysicalSize`）
- `nestdesk-app/src-tauri/src/lib.rs:497-503`（采样恒为物理像素）
- `nestdesk-app/src-tauri/tauri.conf.json:19,33,47,61,75`（首启 x/y 是**逻辑**单位）与上两行混用

**证据**
全 src-tauri 无 `WM_DPICHANGED`/`WM_DISPLAYCHANGE` 处理（grep `dpi|displaychange|wm_` 全仓零命中）。tauri.conf.json 的初始 x/y 按 Tauri 文档以 DIP（逻辑像素）解释，而 layout.json 里存的是 `outer_position()` 返回的物理像素。也就是说：首次启动是逻辑坐标，之后每次启动是物理坐标——同一次 DPI 从 125% 切到 150% 后，同样的物理像素数在新缩放下相对偏移（面板整体右下漂移、易压住任务栏或无工作区避让）。这与 SPEC.md §3 兼容矩阵 C2（分辨率/DPI 切换"不糊不偏移"）、C3（双屏不跳没）的验收项直接冲突，实现里没有任何对应逻辑。

**触发**
改系统缩放、重连投影仪、笔记本拔底座、主屏变更。

**修法**
- 在 layout.json 中随 rect 存当时 DPI/所属显示器实例（`MONITORINFOEX.szDevice`），加载时按 DPI 差异重解释或直接 clamp；
- 窗口创建后注册 `WM_DPICHANGED/WM_DISPLAYCHANGE` 处理（Tauri 侧可挂事件监听或轮询显示器配置变化），切换后对每个 surface 用 `MonitorFromPoint` + rcWork clamp 并重 pin；
- 若决定不做迁移，至少做到"不可见即回落可见"的兜底（见 W-01）。

---

## W-03【中高】Win+D 后每 2 秒 `ShowWindow(SW_RESTORE)` 会激活并抢焦点

**位置**
- `nestdesk-app/src-tauri/src/layer.rs:109-117`（`pin_bottom`：`IsIconic!=0 → ShowWindow(h, SW_RESTORE)`，SW_RESTORE=9 定义于 layer.rs:11）
- `nestdesk-app/src-tauri/src/lib.rs:492-493`（每个 tick 对每个 surface 无条件调用）
- `nestdesk-app/src-tauri/src/lib.rs:15`（SURFACES 顺序 shelf 居首）

**证据**
WinUser.h：`SW_RESTORE` 的语义是 **Activates and displays the window**——激活。同一个函数里另一条路径 `SetWindowPos(... SWP_NOACTIVATE)`（layer.rs:116）明确不激活，唯独恢复路径绕过它。spawn 线程每 2s 扫一遍 SURFACES，顺序第一个是 shelf（lib.rs:15），因此每次 Win+D 后最先被 `SW_RESTORE` 拉到前台的就是收纳框——用户刚按 Win+D 想清屏（且正在浏览器里打字），2 秒内键盘焦点跳进 NestDesk 面板；反复按 Win+D 每次都会重演一次。skipTaskbar 只消掉了任务栏闪烁，消不掉焦点转移。

**触发**
任何"最小化所有"组合键（Win+D / 任务栏「显示桌面」），尤其发生在用户正往别的应用输入时。

**修法**
以不激活方式还原最小化：
```rust
let mut wp = std::mem::MaybeUninit::<WINDOWPLACEMENT>::uninit(); // 或直接初始化
let mut wp = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, showCmd: SW_SHOWNA, ..Default::default() };
GetWindowPlacement(h, &mut wp);
if wp.showCmd == SW_SHOWMINIMIZED { wp.showCmd = SW_SHOWNA; SetWindowPlacement(h, &wp); }
```
（`SetWindowPlacement` 的 showCmd=SW_SHOWNA 是文档化且广泛使用的不抢焦显示路径。）若必须保留 ShowWindow，则只允许在前台应用本来就是 NestDesk 自己时才 restore。

---

## W-04【中】2s 无条件 `SetWindowPos(HWND_BOTTOM)` 与用户 Z 序意图互相打架，且制造周期性重合成

**位置**
- `nestdesk-app/src-tauri/src/layer.rs:109-117`（`SetWindowPos(h, 1 as HWND /*HWND_BOTTOM*/, 0,0,0,0, NOMOVE|NOSIZE|NOACTIVATE)`）
- `nestdesk-app/src-tauri/src/lazy? 无——实际在 lib.rs:492-493` 每 tick × 5 窗无条件调用

**证据**
每一次 `SetWindowPos(HWND_BOTTOM)` 都把窗口沉到所有非 topmost 窗口之下，并产生 WM_WINDOWPOSCHANGING/CHANGED 消息 + 分层窗口重合成。用户点击便利贴输入文字（面板获得键盘焦点，Z 序会被系统抬到栈顶）后 ≤2s 即被再次拍到底部：此时"持有焦点的窗口在视觉上压在应用窗口之下"，用户看着浏览器却往被盖住的便利贴敲字——键盘行为与视觉层级脱节。任何第三方置顶工具、用户手动 Alt+Space 层级操作、拖动归位后的瞬时状态都会在下一个 tick 被抹平。SPEC.md §7 line 102 把这个定时器说成"兜底"，但实现没有任何"当前 Z 序是否已经正确"的判断，纯重发。

**触发**
面板交互期间的每个 2s 节拍；第三方窗口管理器；用户临时置顶。

**修法**
只在需要时 re-pin：
- 检查 `GetForegroundWindow()==h` 时跳过（人正用这窗，沉底无意义也无收益）；
- 或先 `GetWindow(h, GW_HWNDPREV)` 链判断是否已在栈底，正确则不发 SetWindowPos；
- 需要观察重合成开销的用户还可将 2s 降级为"状态变化驱动"（收到 WM_WINDOWPOSCHANGED 且有非顶应用窗覆盖再钉）。

---

## W-05【中】`STYLED` 死条目永不清理 + HWND 地址复用误判"已样式化"+ 全部样式改写跑在非属主线程

**位置**
- `nestdesk-app/src-tauri/src/lib.rs:473-484`（`static STYLED: LazyLock<Mutex<HashSet<usize>>>`，key = hwnd 地址）
- `nestdesk-app/src-tauri/src/layer.rs:95-106`（`ensure_layered`）

**证据**
三件事叠加：
1. lib.rs:477-482 以原始 HWND 地址为 key 插入且从不删除。窗口在 hwnd() 取到与 `IsWindow` 检查之间被销毁时，layer.rs:96-98 直接 return，但 lib.rs:482 的 `set.insert(addr)` 照做——死地址入册；进程生命周期内只增不减（量小，语义错才是关键）。
2. Windows 销毁句柄后会**高频复用地址**。任何一次"重建 surface 窗口"（webview2 崩溃自愈、未来动态加面板、Tauri 侧 recreate）拿到旧地址时，`set.contains(&addr)` 短路，`ensure_layered` 被跳过，新窗口保留 tauri `transparent:true` 带来的 `WS_EX_NOREDIRECTIONBITMAP(0x40000)`——这正是 SPEC.md §7 lines 104-108 记录的"合成黑洞"：沉底后完全不进桌面合成，PrintWindow 有内容、屏幕/GDI 无。**样式只打一次的架构以这个 HashSet 钉死在旧窗口实例上**。
3. explorer.exe 重启本身**不会**重建这些 top-level 窗口（SPEC.md §7 line 109 自述"天然无父链依赖"）——所以审查题设的"explorer 重启污染"前提不成立；真正的污染源是进程内销毁-新建 + 地址复用 + 第 1 条的死条目。
4. 另外 layer.rs:103 `SetWindowLongPtrW(GWL_EXSTYLE)` 由 spawn 后台线程发起，而窗口属主线程（Microsoft KB183588 一系指南要求 GWL 改写由创建线程完成）。spike 验证过"能用"，但属脆弱实践：主线程此刻可能正在派发该窗口的 WM_* 消息，改扩展样式存在消息泵竞态面。

**触发**
surface 窗口被销毁后地址被复用；长 uptime + 未来任何动态窗口特性上线即可静默踩雷。

**修法**
- key 改用 `w.label()`（String），并在某个 label 从 `surfaces()` 消失时移除该条（保证 rebuild 后必重样式）；
- `IsWindow==0` 直接 return 的分支不再 insert；
- 所有 GWL/DWM 属性改写迁回主线程：用 Tauri `AppHandle::run_on_main_thread` 或 `tauri-plugin` 的主线程调度；`SetLayeredWindowAttributes`/`SetWindowPos` 可留后台线程（DWM 属主，跨线程安全）。

---

## W-06【中低】控制台窗口几何既不采样也不恢复——与 README "位置自动记忆" 承诺不符

**位置**
- `nestdesk-app/src-tauri/src/lib.rs:15`（SURFACES 五项不含 `"settings"`）
- `nestdesk-app/src-tauri/src/lib.rs:468`（采样循环只遍历 `surfaces()`）
- `nestdesk-app/src-tauri/src/lib.rs:452-457`（恢复循环只覆盖 layout.json 里出现的 label，keys ⊆ SURFACES）
- `nestdesk-app/src-tauri/README.md:19`（承诺 "窗口位置自动记忆（layout.json + .bak 双备份）"）

**证据**
settings 窗口是普通装饰窗口、可缩放（tauri.conf.json:85-94），却永远不会进入 layout.json：采样循环写死了 SURFACES。每次启动它都弹回 conf 默认 `x=1080, y=90`，用户上次挪过的位置和 660×780 之外的缩放全部丢失。五个桌面面板都有记忆，唯独最常用的控制台没有，属于产品行为不一致。

**触发**
每次启动应用（或最大化/移动控制台后关闭）。

**修法**
把 `"settings"` 加入单独的白名单列表（参与采样与恢复但不进 STYLED/pin/style_surface 分支）；注意 settings 不是透明窗口，不能走 ensure_layered。

---

## W-07【低中】CPU 双采样器三处数据瑕疵

**位置**
- `nestdesk-app/src-tauri/src/layer.rs:119-143`

**证据**
1. 首轮调用走 `None => 0.0`（layer.rs:137-138）：前端 sysmon 组件每 2s 调 `get_sysinfo`（App.tsx:81-83），每次前端组件重挂载（比如 DevTools 重载、设置切页再回来）都会把后端历史抹掉，第一拍永远显示 0%，即使 CPU 正忙。
2. 睡眠唤醒后 FileTime 计数与墙钟不同步，可能出现 `dt == 0`（两台采样间系统未推进）→ 返回 0.0（layer.rs:135-136 分支），醒来瞬间误报 0%。
3. 反过来，若上一次调用与本次之间夹了系统睡眠，dt 巨大而 di 几乎不动，算出的利用率会被摊薄成一整段平均值，只展示一个数字时无从分辨是"忙"还是"刚醒"。
结论均为仪表数据质量问题，不影响系统。

**修法**
- dt==0 时保持上一个有效比率而不是返回 0.0；
- None 分支返回 `Option<f64>`/`f64::NAN` 让前端显示 "--" 而非 0%；或干脆返回上一拍的持久副本。

---

## W-08【低中】布局落盘节流语义：宣称"3 秒静止自动落盘"，实际 ≥4 秒且可被单次抖动无限推迟

**位置**
- `nestdesk-app/src-tauri/src/lib.rs:505-512`

**证据**
tick 周期 2000ms（lib.rs:465），`dirty_since = Some(now)` 只在"有变化"的节拍更新，保存条件是 `t.elapsed() > 3s`。于是实际行为是：**两个连续无变化 tick**（约 4s）才满足；另外任何一次变都重置计时。用户拖拽过程中的每一帧抖动（哪怕最终静止）都会延后保存，且保存动作发生在下一拍 start 后，无背压。这不算 bug，但与注释"移动后 3 秒静止自动落盘"的语义差一档，也意味着用户强杀进程后最多丢近 4–6 秒的位移。

**修法**
把节流改成基于时间戳比较：`elapsed >= 3s` 判定在拿到 `Ok(pos)` 之后立即用 `Instant::now() - last_change_instant` 判断，而非依赖节拍数；或干脆每拍无脑覆盖写（layout.json 是小文件，原子 rename，成本可忽略）。

---

## W-09【低】并发审查：close 竞态当前不存在但**全部 Err-safe**，真正的问题是两处锁 poison 与每拍无谓重设 alpha

**位置**
- `nestdesk-app/src-tauri/src/lib.rs:461-514`
- `nestdesk-app/src-tauri/src/layer.rs:131`（`CPU_PREV.lock().unwrap()`）
- `nestdesk-app/src-tauri/src/lib.rs:92`（`SETTINGS.read().unwrap()`）

**证据**
正面结论先行：前端**没有任何 close/destroy 窗口代码**（grep `../src/App.tsx` 零命中；capabilities/default.json:7 只有 `core:default`，连动态创建窗口的权限都没给），所以"前端 close 窗口与采样线程竞态"今天不可达。即便如此，逐点核对仍要记录：
1. `w.hwnd()/outer_position()/inner_size()` 全部 Ok-guard（lib.rs:469,497），窗口销毁中途不会被 panic——这部分设计正确。
2. `CPU_PREV.lock().unwrap()` 与 `current_settings()` 里的 `read().unwrap()`：任一处持锁线程 panic 后，后续每次调用都 panic-spiral（pin 线程整个哑掉，面板失去钉扎），且锁毒化后 `save_layout` 一并被断。
3. lib.rs:485-492 每拍 × 每窗 `set_alpha`：alpha 不变也照发 `SetLayeredWindowAttributes`。代码注释（layer.rs:71-72）声明其廉价，但它每 2s 强制 5 次分层重合成，且在某些 WebView2/驱动组合上与 DWM 圆角属性叠加偶发闪屏——属于无谓 churn。

**修法**
- 锁获取改用 `unwrap_or_else(|e| e.into_inner())`（锁毒化只代表前任 panic，状态仍可用）；
- alpha 应用改为"设置变化时才发"（缓存上次值），或与 W-04 一样只在需要时下发。

---

## W-10【低中】shelf 窗口 `focus:true` 使每次启动都抢前台；叠加 W-03 后 Win+D 的受害者固定是收纳框

**位置**
- `nestdesk-app/src-tauri/tauri.conf.json:26`（shelf `"focus": true`；其余四个面板均 false，见 40/54/68/82 行）

**证据**
工具窗口 + skipTaskbar 没有任务栏入口，却拥有启动即前台特权：用户在桌面右键运行 NestDesk（双击 exe、开机自启未来上线后更明显）时，键盘焦点无条件跳到收纳框；而且它排在 SURFACES 第一位（lib.rs:15），配合 W-03 的 `SW_RESTORE` 激活路径，**每次 Win+D 后 2 秒内被重新激活的窗口恰好就是收纳框**——两套独立缺陷共同放大成"Win+D 后我的输入到了归巢"这一可感行为。

**修法**
shelf 改 `"focus": false`（桌面面板不需要启动抢焦点，需要时鼠标点击即得）；W-03 修复后此项影响自然消解大半。

---

## 附：对审查题设的直接回答

1. **ensure_layered 每窗只打一次 / explorer 重启 / 热插拔**：explorer 重启不重建 top-level 窗口（SPEC.md §7 line 109），set 旧地址在该场景下无害；真正有害的是**进程内销毁后 HWND 地址复用 + 死条目永不清除**（W-05）。新窗口被误判"已样式化"时正是 SPEC 记录的 NOREDIRECTIONBITMAP 合成黑洞复发场景。
2. **pin_bottom 频率**：2s × 5 窗 = 2.5 SetWindowPos/s，量本身不构成风暴；问题不在频率而在**无条件的 Z 序重发与用户焦点/交互打架**（W-04）以及 `SW_RESTORE` 激活（W-03）。
3. **-21333 类负坐标**：会被原样保存、原样恢复，面板整体消失且无 UI 逃生通道（W-01）。
4. **多屏/DPI**：零处理，conf 首启用逻辑坐标、之后用物理坐标，跨次启动单位不一致（W-02）。
5. **transparent+decorations:false+skipTaskbar+Win+D**：`SW_RESTORE` 会激活窗口（WinUser.h 语义），焦点每 2s 可能跳回面板；`SWP_NOACTIVATE` 只护住了 SetWindowPos 一条路，没护住 ShowWindow 这条（W-03）。
6. **sysmon CPU**：首轮恒 0%、唤醒后 dt==0 报 0%、睡眠跨段平均值失真三类仪表瑕疵（W-07）。
7. **layout.json 恢复只在启动跑一次**：`todo` 确在 SURFACES（lib.rs:15），启动恢复覆盖它；但**当前代码根本不存在运行时新建窗口的路径**（前端无 createWindow，capabilities 未授权），所以"新增窗口缺恢复"今天是潜伏缺陷而非现症；真正现症是 settings 窗口完全不记忆（W-06）。
8. **并发**：无 close 路径故竞态不可达；真正的锁隐患是两处 `.unwrap()` 的 poison-spiral 与每拍 alpha 重发（W-09）。

## 已核对、无需修复的点（防止误报）

- `settings_defaults` alpha 表漏了 `todo`（lib.rs:59-60）但 `init_settings` 的 SURFACES 兜底循环（lib.rs:84-86）补全，非 bug。
- `cpu_usage` 公式 `1 - Δidle/Δ(kernel+user)` 本身正确（GetSystemTimes 的 kernel 含 idle），无算术错误。
- `style_surface` 的 DWMWCP_ROUND=2 / DWMWA_WINDOW_CORNER_PREFERENCE=33 常量正确。
- `pin_bottom` 的 `SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE` 三标志齐备，不抢焦点、不改尺寸。
- `save_layout/settings` 的 tmp→rename 原子写 + .bak 保底策略一致，无并发双写者（layout 只有 spawn 线程一个写者）。
- layout.json 与 settings.json 均按 label 键控，恢复循环对未知 label 是安全的（get_webview_window 查不到即跳过）。