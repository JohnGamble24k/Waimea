//! The engine behind the desktop app (`desktop/`): the same controller as
//! `--daemon`, plus the Winamp windows as one RGBA image and mouse input in
//! window pixels. Lists and search go over the regular IPC protocol.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::mpsc::sync_channel;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use crate::audio::chime;
use crate::command::Command;
use crate::config::{self, Config};
use crate::controller::{self, Controller};
use crate::event::AppEvent;
use crate::gfx::encode::base64;
use crate::gfx::{GfxPainter, Protocol};
use crate::ipc::{self, IpcServer};
use crate::playback::Playback;
use crate::providers::get_bytes;
use crate::providers::radio::{RadioClient, StationQuery};
use crate::skin::WinampSkin;
use crate::state::SessionState;
use crate::view::PlayerView;
use crate::winamp::compose::{self, RowSpan};
use crate::winamp::sprites::{CHAR_W, WINDOW_W};
use crate::winamp::{Region, WinampUiState, Window, WindowGeom};

const MAIN_H: u32 = 116;
const EQ_H: u32 = 116;
const PLAYLIST_SEGMENTS: u32 = 4;
const ROW_H: i32 = 10;
/// Playlist rows get a virtual mouse row each, far from any window pixel.
const TRACKS_ROW: u16 = 10_000;

/// Top of the equalizer and playlist in the stacked image, when shown.
fn stack(ui: &WinampUiState) -> (Option<u32>, Option<u32>) {
    let eq = ui.show_eq.then_some(MAIN_H);
    let below_eq = MAIN_H + if ui.show_eq { EQ_H } else { 0 };
    (eq, ui.show_playlist.then_some(below_eq))
}

fn push_window(ui: &mut WinampUiState, window: Window, y: u32, height: u32) {
    // One "cell" per pixel, so the terminal's hit testing and drags work as is.
    ui.push_region(Region::Window(WindowGeom {
        window,
        cells: Rect::new(0, y as u16, WINDOW_W as u16, height as u16),
        px_per_col: 1.0,
        px_per_row: 1.0,
        height_px: height,
    }));
}

/// Stack the visible windows into one image: `[flags, height lo, height hi]`
/// followed by RGBA pixels. Flag bit 0: animating, the caller should poll fast.
pub(crate) fn render(
    skin: &WinampSkin,
    view: &PlayerView,
    ui: &mut WinampUiState,
    playing: bool,
    out: &mut Vec<u8>,
) {
    let (eq_y, pl_y) = stack(ui);
    ui.begin_layout();
    let t = ui.clock();
    ui.compose_main(skin, view, t);
    push_window(ui, Window::Main, 0, MAIN_H);
    let mut height = MAIN_H;
    if let Some(y) = eq_y {
        ui.compose_eq(skin, view);
        push_window(ui, Window::Equalizer, y, EQ_H);
        height = y + EQ_H;
    }
    if let Some(y) = pl_y {
        let pl_h = compose::playlist_height(PLAYLIST_SEGMENTS);
        render_playlist(skin, view, ui, pl_h);
        push_window(ui, Window::Playlist, y, pl_h);
        height = y + pl_h;
    }
    out.clear();
    out.reserve(3 + (WINDOW_W * height * 4) as usize);
    out.push(u8::from(playing));
    out.extend((height as u16).to_le_bytes());
    let mut append = |w: Window| out.extend_from_slice(ui.canvas(w).pixels().as_flattened());
    append(Window::Main);
    if eq_y.is_some() {
        append(Window::Equalizer);
    }
    if pl_y.is_some() {
        append(Window::Playlist);
    }
}

fn render_playlist(skin: &WinampSkin, view: &PlayerView, ui: &mut WinampUiState, pl_h: u32) {
    let area = compose::playlist_track_area(pl_h);
    let visible = ((area.h as i32 - 3) / ROW_H).max(0) as usize;
    let row_y = |i: usize| area.y + 2 + i as i32 * ROW_H;
    ui.pl_rows.clear();
    ui.pl_rows.extend((0..visible).map(|i| RowSpan {
        y0: row_y(i),
        y1: row_y(i + 1),
        track: None,
    }));
    let first = ui.sync_playlist_scroll(visible, view);
    for (i, row) in ui.pl_rows.iter_mut().enumerate() {
        row.track = (first + i < view.playlist.len()).then_some(first + i);
    }
    ui.push_region(Region::Tracks {
        cells: Rect::new(0, TRACKS_ROW, WINDOW_W as u16, visible as u16),
        first,
    });
    ui.compose_playlist(skin, view, pl_h);

    let canvas = ui.canvas_mut(Window::Playlist);
    for i in 0..visible {
        let idx = first + i;
        let Some(item) = view.playlist.get(idx) else {
            break;
        };
        let (color, _) = compose::row_colors(skin, view, idx);
        let title = match &item.artist {
            Some(artist) => format!("{}. {artist} - {}", idx + 1, item.title),
            None => format!("{}. {}", idx + 1, item.title),
        };
        let y = row_y(i) + 2;
        let mut time = String::new();
        if let Some(secs) = item.duration {
            compose::write_time(&mut time, secs);
        }
        let right = area.x + area.w as i32 - 2;
        let time_x = right - time.chars().count() as i32 * CHAR_W as i32;
        compose::draw_text_colored(
            canvas,
            skin,
            &title,
            area.x + 2,
            y,
            time_x - CHAR_W as i32,
            color,
        );
        compose::draw_text_colored(canvas, skin, &time, time_x, y, right + 1, color);
    }
}

/// A mouse event at window pixel (x, y) of the stacked image, in the form the
/// terminal front end produces. `None` for an unknown `kind`.
pub(crate) fn mouse_event(ui: &WinampUiState, kind: &str, x: u16, y: u16) -> Option<MouseEvent> {
    let kind = match kind {
        "down" => MouseEventKind::Down(MouseButton::Left),
        "drag" => MouseEventKind::Drag(MouseButton::Left),
        "up" => MouseEventKind::Up(MouseButton::Left),
        "wheel-up" => MouseEventKind::ScrollUp,
        "wheel-down" => MouseEventKind::ScrollDown,
        _ => return None,
    };
    let mut row = y;
    let (_, pl_y) = stack(ui);
    if let (Some(top), false) = (
        pl_y,
        matches!(kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_)),
    ) {
        let area = compose::playlist_track_area(compose::playlist_height(PLAYLIST_SEGMENTS));
        let (fx, fy) = (i32::from(x), i32::from(y) - top as i32);
        if fx >= area.x
            && fx < area.x + area.w as i32
            && fy >= area.y
            && fy < area.y + area.h as i32
        {
            let visible = ((area.h as i32 - 3) / ROW_H).max(1);
            row = TRACKS_ROW + ((fy - area.y - 2).max(0) / ROW_H).min(visible - 1) as u16;
        }
    }
    Some(MouseEvent {
        kind,
        column: x,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

/// Listen on `endpoint`. A terminal or daemon Waimea holding it is asked to
/// save and quit first: opening the app should just work. (A second desktop
/// app never gets here, the single-instance plugin focuses the first one.)
fn take_over(endpoint: &Path, tx: &UnboundedSender<AppEvent>) -> io::Result<IpcServer> {
    let mut result = IpcServer::start(endpoint.to_path_buf(), tx.clone());
    if matches!(&result, Err(e) if e.kind() == io::ErrorKind::AddrInUse) {
        let _ = request(r#"{"cmd":"quit"}"#);
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(100));
            result = IpcServer::start(endpoint.to_path_buf(), tx.clone());
            if !matches!(&result, Err(e) if e.kind() == io::ErrorKind::AddrInUse) {
                break;
            }
        }
    }
    result
}

/// Handle to the running player. Dropping it does not stop the player: call
/// [`Desktop::quit`].
pub struct Desktop {
    tx: UnboundedSender<AppEvent>,
    last_frame: Mutex<Vec<u8>>,
}

impl Desktop {
    /// Start the player on its own thread. Fails when another instance owns
    /// the remote-control socket. `on_exit` runs after the state was saved.
    pub fn start(on_exit: impl FnOnce() + Send + 'static) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let endpoint = ipc::endpoint();
        let server = take_over(&endpoint, &tx)
            .with_context(|| format!("cannot listen on {}", endpoint.display()))?;
        let thread_tx = tx.clone();
        std::thread::Builder::new()
            .name("waimea-player".into())
            .spawn(move || {
                let tx = thread_tx;
                let (mut config, _) = Config::load();
                let (mut state, _) = SessionState::load(&config::state_path());
                // The window is the Winamp skin, and has no terminal onboarding.
                config.theme = "Winamp".into();
                config.onboarding_done = true;
                state.theme = Some("Winamp".into());
                // The old download-on-first-run default gives way to our own.
                if state.skin.as_deref() == Some("base-2.91.wsz") {
                    state.skin = None;
                }
                let volume = state.volume;
                if let Ok(rt) = crate::runtime() {
                    rt.block_on(async move {
                        let playback = Playback::new(tx.clone());
                        if config.startup_sound {
                            chime::play(volume);
                        }
                        let painter = GfxPainter::with_cell_size(Protocol::HalfBlock, None);
                        let mut ctl = Controller::new(config, state, tx.clone(), playback, painter);
                        ctl.observers.push(server.observer());
                        let media_controls = ctl.app.config.media_controls;
                        // ponytail: macOS media keys need the main thread's run
                        // loop, which Tauri owns; wire mediactl::host in when
                        // the app ships on macOS.
                        ctl.observers
                            .extend(crate::mediactl::start(media_controls, tx));
                        ctl.start(Vec::new());
                        controller::run_headless(&mut ctl, rx, std::future::pending()).await;
                        ctl.shutdown();
                        drop(server);
                    });
                    rt.shutdown_timeout(Duration::from_millis(200));
                }
                on_exit();
            })?;
        Ok(Self {
            tx,
            last_frame: Mutex::new(Vec::new()),
        })
    }

    /// The current image (see [`render`]), or only its flags byte when nothing
    /// changed, so the web view has no repaint to do. Empty while no skin is
    /// loaded.
    pub fn frame(&self) -> Vec<u8> {
        let (reply, answer) = sync_channel(1);
        if self.tx.send(AppEvent::DesktopFrame(reply)).is_err() {
            return Vec::new();
        }
        let image: Vec<u8> = answer
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_default();
        let Ok(mut last) = self.last_frame.lock() else {
            return image;
        };
        if !image.is_empty() && *last == image {
            return vec![image[0]];
        }
        last.clone_from(&image);
        image
    }

    /// Spectrum magnitudes as little-endian f32 (see `AppEvent`).
    pub fn spectrum(&self) -> Vec<u8> {
        let (reply, answer) = sync_channel(1);
        if self.tx.send(AppEvent::DesktopSpectrum(reply)).is_err() {
            return Vec::new();
        }
        answer
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_default()
    }

    /// Equalizer settings and preset names as JSON.
    pub fn eq(&self) -> String {
        let (reply, answer) = sync_channel(1);
        if self.tx.send(AppEvent::DesktopEq(reply)).is_err() {
            return "{}".into();
        }
        answer
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|_| "{}".into())
    }

    /// Mouse input in stacked-image pixels. Returns true when the user asked
    /// to open files (the eject button).
    pub fn mouse(&self, kind: &str, x: u16, y: u16) -> bool {
        let (reply, answer) = sync_channel(1);
        let event = AppEvent::DesktopMouse {
            kind: kind.to_string(),
            x,
            y,
            reply,
        };
        self.tx.send(event).is_ok() && answer.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
    }

    pub fn quit(&self) {
        let _ = self.tx.send(AppEvent::Command(Command::Quit));
    }
}

/// One protocol request over the remote-control socket; the reply line.
pub fn request(line: &str) -> std::io::Result<String> {
    let mut conn = ipc::connect(&ipc::endpoint())?;
    conn.write_all(line.as_bytes())?;
    conn.write_all(b"\n")?;
    conn.flush()?;
    let mut reply = String::new();
    BufReader::new(conn).read_line(&mut reply)?;
    Ok(reply.trim_end().to_string())
}

/// A picture for the front of the player, as a `data:` URL, and who supplied it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artwork {
    pub url: String,
    pub credit: String,
    /// A station logo, shown whole on a light background; else a photo.
    pub logo: bool,
}

const ARTWORK_MAX_BYTES: u64 = 4 << 20;
const JSON_MAX_BYTES: u64 = 2 << 20;
const WIKI_CANDIDATES: usize = 3;
/// Wikidata occupations that make a person a music act: musician, singer,
/// singer-songwriter, composer, rapper, pianist, saxophonist, guitarist,
/// drummer, disc jockey, record producer, jazz musician, instrumentalist.
const MUSIC_OCCUPATIONS: [&str; 13] = [
    "Q639669",
    "Q177220",
    "Q488205",
    "Q36834",
    "Q2252262",
    "Q486748",
    "Q12800682",
    "Q855091",
    "Q386854",
    "Q130857",
    "Q183945",
    "Q15981151",
    "Q1278335",
];

/// The artist's freely licensed photo from Wikimedia Commons, else the
/// station's logo. Photos carry their maker and licence, as the licences ask.
pub fn artwork(artist: &str, station: &str, stream_url: &str) -> Option<Artwork> {
    if !artist.is_empty() {
        if let Some(art) = commons_photo(artist) {
            return Some(art);
        }
    }
    if station.is_empty() {
        return None;
    }
    let agent = crate::net::agent();
    let client = RadioClient::discover(agent);
    let stations = client.search(agent, &StationQuery::by_name(station)).ok()?;
    let favicon = stations
        .iter()
        .find(|s| s.url == stream_url)
        .or_else(|| {
            stations
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(station))
        })?
        .favicon
        .clone()?;
    let bytes = get_bytes(agent, &favicon, "image/*", ARTWORK_MAX_BYTES).ok()?;
    Some(Artwork {
        url: data_url(&bytes)?,
        credit: "Radio Browser".into(),
        logo: true,
    })
}

fn commons_photo(artist: &str) -> Option<Artwork> {
    let agent = crate::net::agent();
    let wikidata = |query: String| {
        let url = format!("https://www.wikidata.org/w/api.php?format=json&{query}");
        get_bytes(agent, &url, "application/json", JSON_MAX_BYTES).ok()
    };
    let claims = |id: &str, property: &str| {
        wikidata(format!(
            "action=wbgetclaims&entity={id}&property={property}"
        ))
        .map(|body| claim_values(&body, property))
        .unwrap_or_default()
    };
    let found = wikidata(format!(
        "action=wbsearchentities&type=item&language=en&limit=5&search={}",
        percent_encode(artist)
    ))?;
    for id in exact_matches(&found, artist).iter().take(WIKI_CANDIDATES) {
        let is_music_act = if claims(id, "P31").iter().any(|c| c == "Q5") {
            // A person: a music id alone is not enough, comedians have one too.
            claims(id, "P106")
                .iter()
                .any(|c| MUSIC_OCCUPATIONS.contains(&c.as_str()))
        } else {
            !claims(id, "P434").is_empty()
        };
        if !is_music_act {
            continue;
        }
        let file = claims(id, "P18").into_iter().next()?;
        let url = format!(
            "https://commons.wikimedia.org/w/api.php?format=json&action=query&prop=imageinfo\
             &iiprop=url%7Cextmetadata&iiurlwidth=500&titles=File:{}",
            percent_encode(&file)
        );
        let body = get_bytes(agent, &url, "application/json", JSON_MAX_BYTES).ok()?;
        let (thumb, credit) = commons_picture(&body)?;
        let bytes = get_bytes(agent, &thumb, "image/*", ARTWORK_MAX_BYTES).ok()?;
        return Some(Artwork {
            url: data_url(&bytes)?,
            credit,
            logo: false,
        });
    }
    None
}

/// Wikidata items whose label is exactly `artist`: a wrong face is worse than none.
fn exact_matches(body: &[u8], artist: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Vec::new();
    };
    json["search"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|hit| {
            hit["label"]
                .as_str()
                .is_some_and(|label| label.eq_ignore_ascii_case(artist))
        })
        .filter_map(|hit| hit["id"].as_str().map(String::from))
        .collect()
}

/// Values of one property from `wbgetclaims`: item ids or plain strings.
fn claim_values(body: &[u8], property: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Vec::new();
    };
    json["claims"][property]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|claim| {
            let value = &claim["mainsnak"]["datavalue"]["value"];
            value["id"].as_str().or(value.as_str()).map(String::from)
        })
        .collect()
}

/// Only licences that allow reuse in any app.
fn license_allowed(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if name.contains("nc") && name.starts_with("cc by")
        || name.contains("-nd")
        || name.contains(" nd")
    {
        return false;
    }
    name.starts_with("cc by") || name.starts_with("cc0") || name.starts_with("public domain")
}

/// The thumbnail URL and credit line of a Commons file, if its licence allows it.
fn commons_picture(body: &[u8]) -> Option<(String, String)> {
    let json: serde_json::Value = serde_json::from_slice(body).ok()?;
    let page = json["query"]["pages"].as_object()?.values().next()?;
    let info = &page["imageinfo"][0];
    let meta = &info["extmetadata"];
    if meta["NonFree"]["value"].as_str() == Some("true") {
        return None;
    }
    let license = meta["LicenseShortName"]["value"].as_str()?;
    if !license_allowed(license) {
        return None;
    }
    let maker = strip_html(meta["Artist"]["value"].as_str().unwrap_or(""));
    // Names only, so the line reads the same in every interface language.
    let credit = if maker.is_empty() {
        format!("{license}, Wikimedia Commons")
    } else {
        format!("{maker}, {license}, Wikimedia Commons")
    };
    Some((info["thumburl"].as_str()?.to_string(), credit))
}

/// Commons returns the maker as HTML; the page shows it as plain text.
fn strip_html(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Only real images become a data URL, whatever a server claims to send.
fn data_url(bytes: &[u8]) -> Option<String> {
    let mime = match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        [0, 0, 1, 0, ..] => "image/x-icon",
        _ => return None,
    };
    let mut out = format!("data:{mime};base64,").into_bytes();
    base64(bytes, &mut out);
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{MediaItem, SourceKind};
    use crate::view::{EqSettings, PlayStatus, RepeatMode, StreamFormat};
    use std::time::Duration;

    fn view(playlist: &[MediaItem]) -> PlayerView<'_> {
        PlayerView {
            title: "t",
            status: PlayStatus::Stopped,
            elapsed: Duration::ZERO,
            duration: None,
            volume: 1.0,
            balance: 0.0,
            shuffle: false,
            repeat: RepeatMode::Off,
            eq: EqSettings::default(),
            format: StreamFormat::default(),
            spectrum: &[],
            scope: &[],
            playlist,
            current: None,
            selected: 0,
        }
    }

    #[test]
    fn photos_need_an_exact_name_and_a_free_licence() {
        let search = br#"{"search":[{"id":"Q1","label":"Brian Simpsons"},{"id":"Q2","label":"brian simpson"}]}"#;
        assert_eq!(exact_matches(search, "Brian Simpson"), ["Q2"]);
        let occupation =
            br#"{"claims":{"P106":[{"mainsnak":{"datavalue":{"value":{"id":"Q245068"}}}}]}}"#;
        assert_eq!(claim_values(occupation, "P106"), ["Q245068"]);
        let file = br#"{"claims":{"P18":[{"mainsnak":{"datavalue":{"value":"Band.jpg"}}}]}}"#;
        assert_eq!(claim_values(file, "P18"), ["Band.jpg"]);

        for free in ["CC BY 2.0", "CC BY-SA 4.0", "CC0", "Public domain"] {
            assert!(license_allowed(free), "{free}");
        }
        for not_free in [
            "CC BY-NC 2.0",
            "CC BY-NC-SA 3.0",
            "CC BY-ND 4.0",
            "Fair use",
            "Copyrighted",
        ] {
            assert!(!license_allowed(not_free), "{not_free}");
        }

        let page = |license: &str, extra: &str| {
            format!(
                r#"{{"query":{{"pages":{{"1":{{"imageinfo":[{{"thumburl":"https://x/t.jpg","extmetadata":{{
                "LicenseShortName":{{"value":"{license}"}},
                "Artist":{{"value":"<a href='//x'>Ann  <b>Lee</b></a>"}}{extra}}}}}]}}}}}}}}"#
            )
        };
        let (thumb, credit) = commons_picture(page("CC BY 3.0", "").as_bytes()).unwrap();
        assert_eq!(thumb, "https://x/t.jpg");
        assert_eq!(credit, "Ann Lee, CC BY 3.0, Wikimedia Commons");
        assert_eq!(commons_picture(page("CC BY-NC 2.0", "").as_bytes()), None);
        let non_free = r#","NonFree":{"value":"true"}"#;
        assert_eq!(
            commons_picture(page("CC BY 3.0", non_free).as_bytes()),
            None
        );

        assert_eq!(percent_encode("AC/DC & co"), "AC%2FDC%20%26%20co");
        assert!(data_url(&[0x89, b'P', b'N', b'G', 1])
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert_eq!(data_url(b"<script>alert(1)</script>"), None);
    }

    #[test]
    fn stacks_windows_and_routes_clicks() {
        let skin = WinampSkin::default_skin();
        let items: Vec<MediaItem> = (0..3)
            .map(|i| MediaItem {
                id: format!("/m/{i}.mp3"),
                title: format!("T{i}"),
                artist: None,
                album: None,
                subtitle: None,
                duration: Some(60),
                source: SourceKind::Local,
            })
            .collect();
        let v = view(&items);
        let mut ui = WinampUiState::new();
        ui.show_eq = true;
        ui.show_playlist = true;
        let mut out = Vec::new();
        render(&skin, &v, &mut ui, true, &mut out);
        let pl_h = compose::playlist_height(PLAYLIST_SEGMENTS);
        let height = MAIN_H + EQ_H + pl_h;
        assert_eq!(out[0], 1);
        assert_eq!(u16::from_le_bytes([out[1], out[2]]) as u32, height);
        assert_eq!(out.len(), 3 + (WINDOW_W * height * 4) as usize);

        // A press on the main window's play button.
        let play = mouse_event(&ui, "down", 50, 100).unwrap();
        assert_eq!(play.row, 100);
        assert_eq!(ui.on_mouse(play, &v), None);
        let up = mouse_event(&ui, "up", 50, 100).unwrap();
        assert_eq!(ui.on_mouse(up, &v), Some(Command::Play));

        // The second playlist row selects track 2, a double click plays it.
        let area = compose::playlist_track_area(pl_h);
        let y = (MAIN_H + EQ_H) as i32 + area.y + 2 + ROW_H + 1;
        let row = mouse_event(&ui, "down", 50, y as u16).unwrap();
        assert_eq!(row.row, TRACKS_ROW + 1);
        assert_eq!(ui.on_mouse(row, &v), Some(Command::Select(1)));
        assert_eq!(ui.on_mouse(row, &v), Some(Command::PlayIndex(1)));
        assert!(mouse_event(&ui, "bogus", 0, 0).is_none());
    }
}
