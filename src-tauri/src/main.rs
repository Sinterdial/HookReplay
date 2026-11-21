//! HookReplay 桌面端二进制入口（Main）
//!
//! 在非调试（Release）构建下关闭 Windows 控制台黑框，实现双击即开纯桌面窗口、关窗即退零残留。

// Release 构建下隐藏 Windows 控制台黑框
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    hook_replay_lib::run();
}
