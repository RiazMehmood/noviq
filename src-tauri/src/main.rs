//! EchoAI — Tauri desktop application entry point.
//!
//! This file is intentionally minimal. All wiring lives in `lib.rs`.

// Prevent terminal window from appearing on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    echo_ai_lib::run();
}
