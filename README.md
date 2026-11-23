# 归巢 NestDesk

Windows 桌面分区收纳 + 改装小组件 + **一键复原**（自用 MVP v0.1）

文件拖进收纳框 → 自动按类型归档到 `~/Nest/收件箱/<类别>/`，每一次移动都记进事务日志；
任何时候点「复原上批 / 全部复原」，文件按原路放回原位（字节级，可验）。
桌面层小组件：时钟 / 系统监视(CPU·内存·电池) / 便利贴，4 套皮肤（◐ 循环切换），
窗口位置自动记忆（layout.json + .bak 双备份）。

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

## 自检（不碰真实桌面，纯 temp 目录）
```bash
cargo run --manifest-path src-tauri/Cargo.toml -- --selftest
```
覆盖 12 项断言：批量归档、7 类扩展名分类（含大写/未知/中文文件名）、同名冲突 (2) 后缀、
已删文件的 missing 报告、全部复原字节级一致、幂等重复复原、undo_last 批次粒度。

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
