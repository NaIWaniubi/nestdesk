# 归巢 NestDesk — 数据安全与事务完整性审查

> 来源：审查集群「数据安全」视角（结论由审查员以回复正文交付，本文件为汇总落盘，2026-11-24）
> 仓库 HEAD：bf29e5f（v0.4）· 计数：**P0×2 / P1×6 / P2×3，共 11 条**

## [P0] move_any 哈希校验被静默跳过——跨卷归档无完整性保障时可静默损坏数据
- 位置：nestdesk-app/src-tauri/src/fs_core.rs:117-126
- 证据：`let src_hash = sha256_of(src);`（可能 None）→ `if copy_tree(...)... if let Some(ref want) = src_hash { ... }` → None 时整个验证块跳过 → `let _ = remove_dir_all/remove_file` 仍删源
- 触发：sha256_of 返回 None（文件占用/权限/I/O 中断）。copy_tree 静默错误时无验证即删源，原始数据永久丢失
- 修法：src_hash 计算失败视为致命错误直接 return Err，禁止无验证走 copy→delete 路径

## [P0] save_url 无超时保护——挂起的下载永久阻塞 ingest
- 位置：nestdesk-app/src-tauri/src/fs_core.rs:201
- 证据：`let ok_download = matches!(child.wait(), Ok(s) if s.success());` 无 wait_timeout
- 触发：黑洞 URL/慢速服务器无限等待，Tauri handler 线程永久占住，多次调用耗尽线程池
- 修法：wait_timeout(120s)，超时 kill 子进程 + 清 .part，返回明确超时错误

## [P1] undo_entries journal 重写无备份——rename 链断裂即丢全部 journal 历史
- 位置：nestdesk-app/src-tauri/src/fs_core.rs:306-314
- 证据：先写 tmp 再 rename(tmp, journal)，两步都 `_ =` 吞错，无 .bak 回退
- 触发：进程在最后 move_any 后、rename 前崩溃 → 文件已物理复原但 journal 记旧条目，下次 undo 报 phantom missing；rename 失败 → 重复回放已成功还原的文件 → 冲突重命名风暴
- 修法：先 rename(journal→bak) 再 rename(tmp→journal)，失败用 bak 恢复；serde unwrap 改为 Result 传播

## [P1] find_dups 全量硬读大文件进内存——OOM 风险
- 位置：nestdesk-app/src-tauri/src/fs_core.rs:563
- 证据：`let h = std::fs::read(p).map_err(...)?;` 全量读入 Vec<u8> 才哈希
- 触发：5 个 4GB 同体积视频副本 → ~20GB RAM，OOM。sha256_of 本身是流式 64KB 缓冲，两者不一致
- 修法：改用流式哈希（复用 sha256_of）

## [P1] shelf_ingest 将桌面文件夹递归归档——与 collect_desktop 行为不一致且可能非预期
- 位置：fs_core.rs:224-259 ingest vs fs_core.rs:262-272 collect_desktop（后者显式 is_file() 过滤）；lib.rs:186-189 desk_items 返回含 is_dir=true
- 触发：前端勾选桌面文件夹 → 整目录搬入收件箱/文件夹/；跨卷 copy+remove_dir_all 部分失败产生脏状态而 journal 已记成功
- 修法：ingest 加目录过滤参数或前端传参排除 is_dir；UI 对文件夹单独确认

## [P1] unique_dest 后缀循环无上限 + 不检查 Windows MAX_PATH
- 位置：fs_core.rs:83-91 `while cand.exists()` 无限循环
- 触发：链接回路/网络盘 exists 异常可永不退出；路径 >260 字符时下游 rename/copy 以晦涩 OS 错误失败
- 修法：i<10000 上限；MAX_PATH 提前检查并截断 stem

## [P1] save_settings_file 非原子 rename 链——第二步失败即丢 settings.json
- 位置：nestdesk-app/src-tauri/src/lib.rs:99-104
- 证据：rename(main→bak) 成功后 rename(tmp→main) 若失败，settings.json 不存在；.bak 从不被读取，形同虚设
- 触发：目录权限变更/磁盘满等中间态窗口
- 修法：直接原子 rename(tmp→main)，成功后再把旧 main 备份为 bak；或加 .bak 自动恢复 fallback

## [P1] find_dups 以 `_` 前缀跳过范围过广——误跳合法文件与目录
- 位置：fs_core.rs:542 `n.starts_with('_')` 整棵子树跳过
- 触发：用户 `_notes.txt`、`_backup.zip`、`_temp_work/` 目录等合法命名被静默跳过，查重结果不完整且无提示
- 修法：精确匹配 `_重复待清理` 目录名；或显式排除集合

## [P2] move_any remove_dir_all 返回值被丢弃——部分删除仍报成功
- 位置：fs_core.rs:127
- 触发：源目录内文件被锁 → 部分删除，函数仍返回 Ok；收件箱有副本但源残留，undo 时"·复原"命名混乱
- 修法：检查返回值，失败返回 Err 让调用方记 Failed

## [P2] init_settings 不读取 .bak 恢复损坏 settings.json——自定义设置静默回退默认
- 位置：nestdesk-app/src-tauri/src/lib.rs:71-78
- 证据：parse 失败 unwrap_or_else(settings_defaults)，从不读 .bak
- 修法：parse 失败先读 .bak，仍失败再 defaults

## [P2] undo_entries 中 serde_json::to_string().unwrap() 可 panic（防御性提醒，实际极低）
- 位置：fs_core.rs:312
- 修法：unwrap_or_else 兜底