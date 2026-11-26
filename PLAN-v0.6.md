# NestDesk v0.6 规划：桌面监听 + 托盘快捷键 + 资讯雷达

日期：2026-11-25 · 基线 v0.5（77e639e）

## 1. 三块能力与验收

### A. 桌面目录监听自动归档（DeskBox auto-filing 范式）
| 项 | 设计 |
|---|---|
| 机制 | 后台线程轮询 `read_dir(desktop)`，间隔 5s，比对上次快照的 (name,size,mtime) |
| 稳定窗口 | 文件必须连续两次轮询 `size` 与 `mtime` 不变才收（防浏览器未落盘完就搬走） |
| 默认 | **关闭**（`settings.autoWatch=false`）。开启后仍是显式 opt-in |
| 排除 | `.tmp/.crdownload/.part/desktop.ini` 与隐藏文件；本次已收过的路径 60s 内不重收 |
| 日志 | 与手动收取同 batch 语义，可一键复原 |
| 验收 | 造 3 个文件 → 5–15s 内自动进正确格 → `undo_all` 全部字节级归位；写一半的文件不动 |

### B. 托盘 + 全局快捷键（SPEC C12 承诺）
| 项 | 设计 |
|---|---|
| 托盘 | `tray-icon` feature；菜单：收取桌面 / 复原上批 / 暂停面板 / 显示控制台 / 退出 |
| 快捷键 | `Ctrl+Alt+N` 收取桌面，`Ctrl+Alt+H` 显隐全部面板，`Ctrl+Alt+Z` 复原上批 |
| 暂停 | 隐藏 5 个 surface 并停钉扎（全屏游戏/演示场景） |
| 验收 | 三快捷键各触发一次且面板状态正确；托盘菜单五项均可用；暂停后 Win+D 不复活 |

### C. 资讯雷达（自接 API 定时收资讯）
| 项 | 设计 |
|---|---|
| 目标 | 控制台「雷达」tab：配置关键词/源 → 定时抓取 → 列表展示 → 一键存为便利贴/待办 |
| 源 | ① 任意 RSS/Atom URL（零依赖解析）② 通用 JSON API（用户填 URL + JSONPath 风格字段名）③ Hacker News / GitHub Trending 内置源 |
| 定时 | 后台线程按 `settings.radar.intervalMin`（默认 60）抓取；结果落 `%APPDATA%\NestDesk\radar.json`（保留最近 200 条） |
| 去重 | 按 `link`/`id` 去重，只保留新增 |
| 关键词 | 标题/摘要包含任一关键词才进列表（空 = 全收） |
| 卡片 | 桌面新组件 `radar`（可选显示）：最新 5 条标题滚动 |
| 验收 | 配置 1 个 RSS + 关键词 → 首次抓取有数据 → 二次抓取不重复 → 存为待办成功 → 断网时静默失败不崩 |

## 2. 切片
- S1 桌面监听（fs_core `watch_once` + lib 线程 + settings 开关 + 控制台开关 + selftest 断言）
- S2 托盘快捷键（Cargo features + tray 构建 + global-shortcut 注册 + 暂停状态）
- S3 雷达后端（RSS/JSON 解析 + 线程 + radar.json + commands）
- S4 雷达前端（新 tab + 卡片组件 + 设置项）
- S5 回归（selftest 扩到 24+、真机验证、README/SPEC、提交）

## 3. 明确不做
- 不做完整 RSS 阅读器（无已读同步、无 OPML）
- 不做后台常驻网络轮询之外的推送（无 WebSocket）
- 不改 v0.5 已定的 journal/ACL 结构；雷达与监听复用同一 journal 语义

## 4. 实施记录（2026-11-26）

### 事故与修复（必须留在文档里）
初版 `watch_once` 用「上一轮快照」当基线，导致**开启监听的第二次 tick 就把桌面上 354 个历史文件全部搬走**
（首轮快照即基线，第二轮指纹一致 ⇒ 全部判定为「已落盘完成」）。已完成的处置：

1. 用 journal 复原 354/354（missing=0），桌面回到 355 文件；
2. 新增 `--undo-all-offline` 救援入口（不启 GUI，纯 journal 复原），后续同类事故可一条命令救回；
3. 监听改为 `WatchState { baseline, pending }` 双集合语义：
   - **baseline = 开启监听那一刻桌面的全部文件，永不自动搬走**；
   - pending = 之后新出现、等待「第二轮指纹一致」的文件；
   - 关闭监听期间持续刷新基线，重新开启同样不会补搬历史文件；
4. selftest 增加两条不变量断言：`基线文件不被搬走 + 新文件两轮后收 + 临时文件跳过`、`无新文件不产生空批次`。
5. 真机复验：357 个基线文件全部留在桌面，仅新建的 `watcher-e2e-final.pdf` 在 10s 后被归档（1/1），
   随后 undo_last 复原该文件并删除全部测试产物，桌面回到 355。

### 交付清单
| 项 | 状态 | 证据 |
|---|---|---|
| S1 桌面监听 | ✅ | selftest 2 条断言 + 真机 357 基线零移动 / 新文件 10s 内归位 |
| S2 托盘 + 快捷键 | ✅ | 托盘菜单 5 项（收取/复原/暂停/控制台/退出）；Ctrl+Alt+N/H/Z 注册；`surfaces_toggle` 实测隐藏/恢复 |
| S3 雷达后端 | ✅ | 真实抓取 HN 10 条；去重（重复抓取 +0）；关键词过滤（无关项 0）；radar.json + .bak 自愈 |
| S4 雷达前端 | ✅ | 控制台「雷达」tab + 桌面 radar 卡片（7 窗口在线，卡片只读 ACL 生效） |
| S5 回归 | ✅ | selftest **26/26**；雷达单测 4/4；ACL 矩阵含 radar 窗口被拒改配置 |

### 雷达内置源
Hacker News 头条 / GitHub 趋势（日）/ 阮一峰周刊 / 少数派；任意 RSS/Atom 或 JSON API 也可自加。
JSON 源自动识别数组位置与 `title|name|headline`、`link|url|html_url`、`summary|description` 字段名。

