//! Waimea's desktop window. The player lives in `waimea::desktop`; this file
//! only connects it to the web view in `ui/`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{ipc::Response, AppHandle, Manager, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use waimea::desktop::{self, Desktop};

#[tauri::command(async)]
fn frame(player: State<'_, Desktop>) -> Response {
    Response::new(player.frame())
}

#[tauri::command(async)]
fn spectrum(player: State<'_, Desktop>) -> Response {
    Response::new(player.spectrum())
}

#[tauri::command(async)]
fn eq(player: State<'_, Desktop>) -> String {
    player.eq()
}

/// `{url, credit, logo}`, or null when no picture was found.
#[tauri::command(async)]
fn artwork(artist: String, station: String, stream: String) -> Option<serde_json::Value> {
    desktop::artwork(&artist, &station, &stream)
        .map(|art| serde_json::json!({"url": art.url, "credit": art.credit, "logo": art.logo}))
}

#[tauri::command(async)]
fn mouse(player: State<'_, Desktop>, kind: String, x: u16, y: u16) -> bool {
    player.mouse(&kind, x, y)
}

#[tauri::command(async)]
fn request(line: String) -> Result<String, String> {
    desktop::request(&line).map_err(|e| e.to_string())
}

/// Names of the installed skins, for the Skins tab.
#[tauri::command(async)]
fn skins() -> Vec<String> {
    waimea::net::list_skins(&waimea::config::skins_dir())
        .iter()
        .filter_map(|p| p.file_stem()?.to_str().map(String::from))
        .collect()
}

/// Native picker; the chosen files or folder join the queue. Returns the count.
#[tauri::command(async)]
fn add_music(app: AppHandle, folder: bool, title: String) -> Result<usize, String> {
    let dialog = app.dialog().file().set_title(title);
    let picked = if folder {
        dialog.blocking_pick_folder().map(|f| vec![f])
    } else {
        dialog.blocking_pick_files()
    };
    let paths: Vec<String> = picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
        .collect();
    if !paths.is_empty() {
        let line = serde_json::json!({"cmd": "enqueue", "arg": paths}).to_string();
        desktop::request(&line).map_err(|e| e.to_string())?;
    }
    Ok(paths.len())
}

fn main() {
    tauri::Builder::default()
        // A second launch brings the open window forward.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            frame, spectrum, eq, artwork, mouse, request, add_music, skins
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            match Desktop::start(move || handle.exit(0)) {
                Ok(player) => {
                    app.manage(player);
                }
                Err(e) => {
                    let handle = app.handle().clone();
                    app.dialog()
                        .message(format!("Waimea could not start.\n\n{e:#}"))
                        .title("Waimea")
                        .show(move |_| handle.exit(1));
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Save the session first; the player exits the app when it is done.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if let Some(player) = window.try_state::<Desktop>() {
                    api.prevent_close();
                    player.quit();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Waimea");
}
