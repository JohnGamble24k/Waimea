//! Waimea's desktop window. The player lives in `waimea::desktop`; this file
//! only connects it to the web view in `ui/`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{ipc::Response, AppHandle, Manager, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use waimea::desktop::{self, Desktop};

#[tauri::command(async)]
fn frame(player: State<'_, Desktop>) -> Response {
    Response::new(player.frame())
}

#[tauri::command(async)]
fn spectrum(player: State<'_, Desktop>) -> Response {
    Response::new(player.spectrum())
}

/// The video picture: empty for no video, one byte when unchanged, else JPEG.
#[tauri::command(async)]
fn video(player: State<'_, Desktop>) -> Response {
    Response::new(player.video())
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

/// The YouTube video id of the song a station plays, or null.
#[tauri::command(async)]
fn youtube(title: String, station: String) -> Option<String> {
    desktop::youtube_video(&title, &station)
}

/// Opens a YouTube video in the browser. Only video ids, so the page can not
/// make the app open anything else.
#[tauri::command]
fn open_youtube(app: AppHandle, id: String) -> Result<(), String> {
    if id.len() != 11
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("not a YouTube video id".into());
    }
    app.opener()
        .open_url(
            format!("https://www.youtube.com/watch?v={id}"),
            None::<&str>,
        )
        .map_err(|e| e.to_string())
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

/// Files and URLs given on the command line (an "Open with" from the file
/// manager), made absolute against `cwd`.
fn inputs(args: impl IntoIterator<Item = String>, cwd: &std::path::Path) -> Vec<String> {
    args.into_iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(|a| {
            if a.contains("://") {
                a
            } else {
                cwd.join(a).display().to_string()
            }
        })
        .collect()
}

fn main() {
    // Wayland lets no window keep itself above the others, so the pin button
    // would do nothing there. Run through XWayland, where it works; without
    // XWayland GTK falls back to Wayland itself. WAIMEA_WAYLAND=1 opts out.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAIMEA_WAYLAND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11,wayland");
    }
    tauri::Builder::default()
        // A second launch brings the open window forward and plays its files.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let files = inputs(args, std::path::Path::new(&cwd));
            if !files.is_empty() {
                let line = serde_json::json!({"cmd": "open", "arg": files}).to_string();
                let _ = desktop::request(&line);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            frame,
            video,
            spectrum,
            eq,
            artwork,
            youtube,
            open_youtube,
            mouse,
            request,
            add_music,
            skins
        ])
        .setup(|app| {
            // Windows draws a 1px white border around an undecorated window
            // with a shadow; the page draws its own rounded shape.
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_shadow(false);
            }
            let handle = app.handle().clone();
            let cwd = std::env::current_dir().unwrap_or_default();
            let files = inputs(std::env::args(), &cwd);
            match Desktop::start(files, move || handle.exit(0)) {
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
