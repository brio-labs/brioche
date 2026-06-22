//! Desktop application entry point.
//!
//! Initializes Tauri with the `DesktopState` and registers IPC commands.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use brioche_desktop_lib::DesktopState;
#[cfg(not(clippy))]
use brioche_desktop_lib::commands;

#[cfg(not(clippy))]
fn run_app(state: DesktopState) {
    if let Err(e) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::get_messages,
            commands::clear_messages,
            commands::list_sessions,
            commands::switch_session,
            commands::delete_session,
            commands::new_session,
            commands::get_settings,
            commands::set_settings,
            commands::read_directory,
            commands::read_file,
            commands::write_file,
            commands::delete_file,
            commands::create_file,
            commands::create_directory,
            // Memory commands
            commands::list_memories,
            commands::set_memory,
            commands::delete_memory,
            commands::search_memories,
            // Profile commands
            commands::list_profiles,
            commands::get_profile,
            commands::create_profile,
            commands::switch_profile,
            commands::delete_profile,
            commands::update_profile,
            // Skills commands
            commands::list_skills,
            commands::get_skill_content,
            commands::get_skill_file,
            commands::set_skill_enabled,
            commands::create_skill,
            commands::delete_skill,
            // Model fetching
            commands::fetch_models,
            // Extension commands
            commands::list_extensions,
            commands::list_settings_sections,
            commands::get_footer_metrics,
            // Tool commands
            commands::list_tools,
            commands::set_tool_enabled,
            commands::add_user_tool,
            commands::remove_user_tool,
            // Attachment commands
            commands::attach_reference,
            commands::send_image,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("[brioche-desktop] ERROR: Tauri application failed: {}", e);
        std::process::exit(1);
    }
}

#[cfg(clippy)]
fn run_app(_state: DesktopState) {}

fn main() {
    // Verify frontend assets are accessible before starting Tauri.
    let crate_dir = std::env!("CARGO_MANIFEST_DIR");
    let dist_path = std::path::Path::new(crate_dir).join("frontend/dist/index.html");
    if !dist_path.exists() {
        eprintln!(
            "[brioche-desktop] ERROR: Frontend assets not found at {}\n\
             Run: cd crates/apps/brioche-desktop/frontend && npm run build",
            dist_path.display()
        );
        std::process::exit(1);
    }

    eprintln!(
        "[brioche-desktop] Starting with CARGO_MANIFEST_DIR={}",
        crate_dir
    );
    eprintln!(
        "[brioche-desktop] Frontend assets found at {}",
        dist_path.display()
    );

    let state = match DesktopState::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[brioche-desktop] ERROR: Failed to initialize state: {}", e);
            std::process::exit(1);
        }
    };

    run_app(state);
}
