//! The picture of a video file, for the desktop app. Its sound plays through
//! the audio engine like any track (EQ, gapless, media keys); this decodes the
//! picture with ffmpeg into JPEG frames and hands out the frame that belongs
//! to the audio position, so the audio clock drives the video.

use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::Duration;

/// Frames per second handed to the web view.
const FPS: u64 = 30;

/// What plays, as the controller sees it.
#[derive(Debug, Clone)]
pub struct Clock {
    pub path: PathBuf,
    pub position: Duration,
}

struct Decoder {
    path: PathBuf,
    child: Child,
    out: BufReader<ChildStdout>,
    /// Position of frame 0.
    start: Duration,
    /// Frames read so far.
    read: u64,
    /// ffmpeg has no more frames.
    ended: bool,
}

impl Decoder {
    fn start(path: PathBuf, at: Duration) -> io::Result<Self> {
        let mut cmd = Command::new("ffmpeg");
        crate::no_console(&mut cmd)
            .args(["-nostdin", "-loglevel", "error", "-ss"])
            .arg(format!("{:.3}", at.as_secs_f64()))
            .arg("-i")
            .arg(&path)
            .args(["-an", "-sn", "-dn", "-vf"])
            .arg(format!("fps={FPS},scale='min(1280,iw)':-2"))
            .args(["-c:v", "mjpeg", "-q:v", "4", "-f", "image2pipe", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn()?;
        let out = BufReader::with_capacity(1 << 18, child.stdout.take().expect("piped"));
        Ok(Self {
            path,
            child,
            out,
            start: at,
            read: 0,
            ended: false,
        })
    }

    /// Position of the next frame to read.
    fn next_at(&self) -> Duration {
        self.start + Duration::from_millis(self.read * 1000 / FPS)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Default)]
pub struct Video {
    decoder: Option<Decoder>,
    /// The frame for the current position (JPEG); empty before the first.
    frame: Vec<u8>,
}

impl Video {
    /// The frame for `clock`: empty when there is no picture, `[0]` when it
    /// did not change since the last call, else a JPEG image.
    pub fn frame(&mut self, clock: Option<Clock>) -> Vec<u8> {
        let Some(clock) = clock else {
            self.decoder = None;
            self.frame.clear();
            return Vec::new();
        };
        let want = clock.position;
        let restart = match &self.decoder {
            None => true,
            Some(d) => {
                d.path != clock.path
                    // seeked back, before the frame on screen
                    || want + Duration::from_millis(100)
                        < d.next_at().saturating_sub(Duration::from_millis(1000 / FPS))
                    // seeked ahead, or decoding fell far behind
                    || (!d.ended && want > d.next_at() + Duration::from_secs(3))
            }
        };
        if restart {
            if self.decoder.as_ref().is_some_and(|d| d.path != clock.path) {
                self.frame.clear();
            }
            self.decoder = Decoder::start(clock.path, want).ok();
        }
        let Some(decoder) = &mut self.decoder else {
            return Vec::new();
        };
        let mut fresh = false;
        while !decoder.ended && decoder.next_at() <= want {
            match read_jpeg(&mut decoder.out) {
                Ok(jpeg) => {
                    self.frame = jpeg;
                    decoder.read += 1;
                    fresh = true;
                }
                Err(_) => decoder.ended = true,
            }
        }
        match (fresh, self.frame.is_empty()) {
            (_, true) => Vec::new(),
            (true, false) => self.frame.clone(),
            (false, false) => vec![0],
        }
    }
}

/// One JPEG image from a stream of them: SOI, marker segments up to SOS,
/// then entropy-coded data until EOI. Byte-stuffed `FF 00` and restart
/// markers belong to the data; segment payloads are skipped by length, so an
/// `FF D9` inside a table can not end the image early.
fn read_jpeg(r: &mut impl BufRead) -> io::Result<Vec<u8>> {
    let byte = |r: &mut dyn BufRead| -> io::Result<u8> {
        let mut b = [0];
        r.read_exact(&mut b)?;
        Ok(b[0])
    };
    let mut jpeg = vec![0; 2];
    r.read_exact(&mut jpeg)?;
    if jpeg != [0xFF, 0xD8] {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a JPEG"));
    }
    // Marker segments.
    loop {
        let (ff, marker) = (byte(r)?, byte(r)?);
        if ff != 0xFF {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "bad marker"));
        }
        let (hi, lo) = (byte(r)?, byte(r)?);
        let len = usize::from(hi) << 8 | usize::from(lo);
        jpeg.extend([ff, marker, hi, lo]);
        let start = jpeg.len();
        jpeg.resize(start + len.saturating_sub(2), 0);
        r.read_exact(&mut jpeg[start..])?;
        if marker == 0xDA {
            break;
        }
    }
    // Entropy-coded data up to EOI.
    loop {
        let n = r.read_until(0xFF, &mut jpeg)?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let next = byte(r)?;
        jpeg.push(next);
        if next == 0xD9 {
            return Ok(jpeg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_stream_of_jpegs() {
        // A table holding FF D9, data with a stuffed FF 00 and a restart marker.
        let one: &[u8] = &[
            0xFF, 0xD8, 0xFF, 0xDB, 0x00, 0x04, 0xFF, 0xD9, 0xFF, 0xDA, 0x00, 0x02, 0x12, 0xFF,
            0x00, 0x34, 0xFF, 0xD0, 0x56, 0xFF, 0xD9,
        ];
        let two: &[u8] = &[0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02, 0x99, 0xFF, 0xD9];
        let mut stream = io::Cursor::new([one, two].concat());
        assert_eq!(read_jpeg(&mut stream).unwrap(), one);
        assert_eq!(read_jpeg(&mut stream).unwrap(), two);
        assert!(read_jpeg(&mut stream).is_err());
    }

    /// Needs ffmpeg: `cargo test video -- --ignored`.
    #[test]
    #[ignore]
    fn follows_the_clock() {
        let dir = std::env::temp_dir().join(format!("waimea-video-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("clip.mkv");
        let made = Command::new("ffmpeg")
            .args([
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=25",
            ])
            .args(["-f", "lavfi", "-i", "sine", "-t", "8", "-shortest"])
            .arg(&clip)
            .status()
            .unwrap();
        assert!(made.success());
        let at = |s: f64| {
            Some(Clock {
                path: clip.clone(),
                position: Duration::from_secs_f64(s),
            })
        };
        let mut video = Video::default();
        let first = video.frame(at(0.0));
        assert_eq!(&first[..2], &[0xFF, 0xD8]);
        assert_eq!(video.frame(at(0.0)), vec![0], "same position, same frame");
        assert!(video.frame(at(0.5)).len() > 1, "playing on");
        assert!(video.frame(at(6.0)).len() > 1, "seek ahead");
        assert!(video.frame(at(1.0)).len() > 1, "seek back");
        assert!(
            video.frame(at(30.0)).len() == 1,
            "past the end keeps the last frame"
        );
        assert!(video.frame(None).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
