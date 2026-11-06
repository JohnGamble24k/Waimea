//! The player core: everything except the two front ends' `main` functions
//! (the terminal binary in `main.rs` and the desktop app in `desktop/`).

pub mod app;
pub mod audio;
pub mod browse;
pub mod cli;
pub mod command;
pub mod config;
pub mod controller;
pub mod desktop;
pub mod eq_presets;
pub mod event;
pub mod extractor;
pub mod gfx;
pub mod install;
pub mod ipc;
pub mod keymap;
pub mod library;
pub mod media;
pub mod mediactl;
pub mod net;
pub mod playback;
pub mod playlist_io;
pub mod providers;
pub mod queue;
pub mod report;
pub mod skin;
pub mod skin_browser;
pub mod skin_cache;
pub mod state;
pub mod tap;
pub mod term_input;
pub mod theme;
pub mod ui;
pub mod view;
pub mod vis;
pub mod winamp;

pub fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// No console window for a helper process (yt-dlp, ffmpeg). The desktop app
/// has no console of its own, so Windows would flash one up for every run.
pub(crate) fn no_console(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(cmd, 0x0800_0000); // CREATE_NO_WINDOW
    cmd
}
