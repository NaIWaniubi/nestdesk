// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--undo-all-offline") {
        // 一次性救援入口：不启 GUI，直接用 journal 复原
        nestdesk_app_lib::rescue_undo_all();
        return;
    }
    #[cfg(any(debug_assertions, feature = "harness"))]
    {
        if args.iter().any(|a| a == "--selftest") {
            nestdesk_app_lib::selftest();
            return;
        }
        if args.iter().any(|a| a == "--demo") {
            nestdesk_app_lib::demo_run();
            return;
        }
    }
    nestdesk_app_lib::run()
}
