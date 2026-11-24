# NestDesk 融合规划 v0.4（Folia + DeskBox 双参照）

日期：2026-11-24 · 状态：调研完成，部分切片已实施 · 依据：vendor/folia、vendor/deskbox 本地源码调研

## 1. 参照对象一句话画像

| 项目 | 栈 | ★ | 许可 | 与我们的关系 |
|---|---|---|---|---|
| **DeskBox**（桌面管理） | C# WinUI3 + .NET10 NativeAOT + Rust Shell 层 | 9.6k | **GPL-3.0-only**（商业双授权） | 同品类最强竞品：真实文件夹 widget、撤销历史、Alt+D 搜索、剪贴板历史 Alt+V、Todo、快捷捕获、自定义 widget 体系、音乐控件（SMTC 会话控制+歌词） |
| **Folia major**（音乐，辞曲新境） | Electron24 + Vite + TS（React 渲染层），含自研 mods/mod-export 生态 | 13.4k | **AGPL-3.0** | 目标机同款桌面音乐场景：多平台聚合（本地/Navidrome/在线）、**华丽歌词动画引擎**、mods 可扩展体系 |

## 2. 许可证矩阵（决定借什么）

| 方式 | Folia(AGPL) | DeskBox(GPL) | 结论 |
|---|---|---|---|
| 直接 vendoring 代码进 NestDesk 仓库 | ✗ 强制 AGPL | ✗ 强制 GPL | NestDesk 现为无许可证私有项目，**两者都不能抄代码** |
| 架构/交互范式重实现（Rust/TS 自写） | ✓ | ✓ | 采纳 |
| 以独立子项目身份 fork 运行（不改我们源码） | ✓ 可随附使用 | ✓ | 音乐模块走此路：NestDesk 管理 Folia 进程窗口 |

## 3. 融合切片与验收断言

### S-A 安全归档强化（借鉴 DeskBox copy-first-verify-commit）
- **改动**：fs_core::move_any 跨卷拷贝后做 SHA-256 校验 → 通过才删源；失败保留源文件并报告。
- **验收**：selftest 增加「跨卷同内容写入两处→归档→字节一致且源保留直至校验通过」。

### S-B 多来源拖放（借鉴 DeskBox "浏览器 URL 下载入格"）
- **改动**：shelf 面板 drop 时识别 `text/uri-list`/`text/plain` http(s) 链接 → Rust 端下载落对应分类格，同样过 journal。PowerShell `Invoke-WebRequest` 零依赖实现。
- **边界**：仅 http/https；大文件超 200MB 拒绝并提示。

### S-C Glance 时钟增强（借鉴 DeskBox Glance）
- **改动**：时钟组件增农历/节气显示（纯 TS 算法，1900–2100 数据表，自写不抄）。

### S-D Todo 便利贴族（借鉴 DeskBox QuickCapture/Todo）
- **改动**：新增 todo surface（第 6 窗），todo.json 持久化，一键收录今日截止项到 head 行。

### S-E 音乐表面（Folia 进程托管，借鉴其交互而非代码）
- **改动**：新增「更多→音乐」：启动外部播放器的按钮（默认 Folia，路径在 settings 配置，缺失时给出安装指引）；保持 NestDesk 作为宿主——拖拽歌曲文件到 shelf 同样支持「播放」快捷（右键菜单暂缺，先放置=收藏进格）。
- **深入方向（若后续投入）**：Folia 的歌词动画引擎为 Electron WebGL —— 若我们要原生同级别效果，独立 fork 集成成本高；MVP 先以过程化卡片展示当前曲目（Windows SMTC 读取，作为 S-F）。

### S-F Windows 媒体会话读卡（借鉴 DeskBox Music widget 的 SMTC）
- **改动**：sysmon 同族小卡「正在播放」：读 Windows media session（TrackInfo）到桌面层（Get-SMTC via WinRT in PowerShell 降级通道）。无 SMTC 时隐藏不报错。

## 4. 非目标（v0.4 明确不做）
- 不 vendoring Folia/DeskBox 任何源码；
- Everything 集成（Alt+D）——依赖第三方软件，非本机必需；
- 剪贴板全局监听（需低级钩子，单列为 M5）；
- 高对比皮肤在 Folia 侧的同步（独立进程，画风自持）。

## 5. 遗留决策点（给用户的）
1. 是否允许把 NestDesk 以 GPL-3.0 重新发布？不改则永远只能"借鉴再写"——本次即按再写执行；
2. 音乐深度路线选 A（Folia 整进程托管，10 分钟落地）或 B（NestDesk 内重建迷你歌词引擎，数日级）。