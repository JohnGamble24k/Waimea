//! Windows: System Media Transport Controls (the media overlay next to the
//! volume flyout, the lock screen, media keys and headset buttons).
//!
//! A desktop or console app without a packaged identity gets its SMTC from
//! a `MediaPlayer` whose own command handling is switched off; Waimea plays
//! its audio itself and only uses that player's controls. WinRT activates
//! in the multithreaded apartment on first use (windows-rs does that), so
//! this works from the player thread of the TUI, `--daemon` and the desktop
//! app alike.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc::UnboundedSender;
use windows::core::{Result, HSTRING};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    SystemMediaTransportControls, SystemMediaTransportControlsButton,
    SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};

use super::track::{Changes, Clock, State};
use crate::command::Command;
use crate::event::AppEvent;
use crate::view::PlayStatus;

/// WinRT `TimeSpan` counts 100 ns ticks.
fn ticks(d: Duration) -> TimeSpan {
    TimeSpan {
        Duration: i64::try_from(d.as_nanos() / 100).unwrap_or(i64::MAX),
    }
}

fn from_ticks(t: TimeSpan) -> Duration {
    Duration::from_nanos(u64::try_from(t.Duration).unwrap_or(0).saturating_mul(100))
}

fn status_code(status: PlayStatus) -> u8 {
    match status {
        PlayStatus::Stopped => 0,
        PlayStatus::Playing => 1,
        PlayStatus::Paused => 2,
    }
}

/// The command for a pressed button; Play while playing restarts nothing.
fn button_command(button: SystemMediaTransportControlsButton, status: u8) -> Option<Command> {
    match button {
        SystemMediaTransportControlsButton::Play => {
            (status != status_code(PlayStatus::Playing)).then_some(Command::Play)
        }
        SystemMediaTransportControlsButton::Pause => Some(Command::Pause),
        SystemMediaTransportControlsButton::Stop => Some(Command::Stop),
        SystemMediaTransportControlsButton::Next => Some(Command::Next),
        SystemMediaTransportControlsButton::Previous => Some(Command::Prev),
        _ => None,
    }
}

pub struct Smtc {
    // Kept alive: the controls belong to this player.
    _player: MediaPlayer,
    smtc: SystemMediaTransportControls,
    status: Arc<AtomicU8>,
}

impl Smtc {
    pub fn start(tx: UnboundedSender<AppEvent>) -> Option<Self> {
        Self::try_start(tx).ok()
    }

    fn try_start(tx: UnboundedSender<AppEvent>) -> Result<Self> {
        let player = MediaPlayer::new()?;
        player.CommandManager()?.SetIsEnabled(false)?;
        let smtc = player.SystemMediaTransportControls()?;
        smtc.SetIsEnabled(true)?;
        smtc.SetIsPlayEnabled(true)?;
        smtc.SetIsPauseEnabled(true)?;
        smtc.SetIsStopEnabled(true)?;
        smtc.SetPlaybackStatus(MediaPlaybackStatus::Closed)?;

        let status = Arc::new(AtomicU8::new(status_code(PlayStatus::Stopped)));
        let (buttons, pressed_status) = (tx.clone(), Arc::clone(&status));
        smtc.ButtonPressed(
            &TypedEventHandler::new(
                move |_,
                      args: windows::core::Ref<
                    SystemMediaTransportControlsButtonPressedEventArgs,
                >| {
                    if let Some(args) = args.as_ref() {
                        let status = pressed_status.load(Ordering::Acquire);
                        if let Some(cmd) = button_command(args.Button()?, status) {
                            let _ = buttons.send(AppEvent::Command(cmd));
                        }
                    }
                    Ok(())
                },
            ),
        )?;
        smtc.PlaybackPositionChangeRequested(&TypedEventHandler::new(
            move |_, args: windows::core::Ref<PlaybackPositionChangeRequestedEventArgs>| {
                if let Some(args) = args.as_ref() {
                    let pos = from_ticks(args.RequestedPlaybackPosition()?);
                    let _ = tx.send(AppEvent::Command(Command::SeekTo(pos)));
                }
                Ok(())
            },
        ))?;
        Ok(Self {
            _player: player,
            smtc,
            status,
        })
    }

    pub fn publish(&self, state: &State, clock: &Clock, changes: &Changes) {
        self.status
            .store(status_code(state.status), Ordering::Release);
        // The overlay is a convenience: a failed update leaves it as it was.
        let _ = self.update(state, clock, changes);
    }

    fn update(&self, state: &State, clock: &Clock, changes: &Changes) -> Result<()> {
        let smtc = &self.smtc;
        if changes.caps {
            smtc.SetIsNextEnabled(state.can_next)?;
            smtc.SetIsPreviousEnabled(state.can_prev)?;
        }
        if changes.track {
            let display = smtc.DisplayUpdater()?;
            display.SetType(MediaPlaybackType::Music)?;
            let music = display.MusicProperties()?;
            let track = &state.track;
            music.SetTitle(&HSTRING::from(track.title.as_str()))?;
            music.SetArtist(&HSTRING::from(track.artist.as_deref().unwrap_or("")))?;
            music.SetAlbumTitle(&HSTRING::from(track.album.as_deref().unwrap_or("")))?;
            display.Update()?;
        }
        if changes.status {
            smtc.SetPlaybackStatus(match state.status {
                PlayStatus::Playing => MediaPlaybackStatus::Playing,
                PlayStatus::Paused => MediaPlaybackStatus::Paused,
                PlayStatus::Stopped => MediaPlaybackStatus::Stopped,
            })?;
        }
        if changes.track || changes.status || changes.caps || changes.seeked.is_some() {
            let timeline = SystemMediaTransportControlsTimelineProperties::new()?;
            let length = state.track.length.filter(|_| state.can_seek);
            let end = length.unwrap_or_default();
            timeline.SetStartTime(ticks(Duration::ZERO))?;
            timeline.SetMinSeekTime(ticks(Duration::ZERO))?;
            timeline.SetEndTime(ticks(end))?;
            timeline.SetMaxSeekTime(ticks(end))?;
            timeline.SetPosition(ticks(clock.position(Instant::now()).min(end)))?;
            smtc.UpdateTimelineProperties(&timeline)?;
        }
        Ok(())
    }
}

impl Drop for Smtc {
    fn drop(&mut self) {
        let _ = self.smtc.SetPlaybackStatus(MediaPlaybackStatus::Closed);
    }
}

pub fn doctor() -> String {
    "[ok] media controls: Windows media overlay and media keys".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_map_to_commands() {
        use SystemMediaTransportControlsButton as B;
        let playing = status_code(PlayStatus::Playing);
        let paused = status_code(PlayStatus::Paused);
        assert!(matches!(
            button_command(B::Play, paused),
            Some(Command::Play)
        ));
        assert!(button_command(B::Play, playing).is_none());
        assert!(matches!(
            button_command(B::Next, playing),
            Some(Command::Next)
        ));
        assert!(button_command(B::Record, playing).is_none());
        let t = Duration::from_millis(1500);
        assert_eq!(from_ticks(ticks(t)), t);
        assert_eq!(from_ticks(TimeSpan { Duration: -5 }), Duration::ZERO);
    }
}
