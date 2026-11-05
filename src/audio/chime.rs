//! Startup sound: three slide-guitar chords, the "Hawaiian vamp" D9, G13, C6.
//! Synthesized on the fly so the binary carries no audio asset. It plays on
//! its own short-lived stream, so the engine stays idle and any failure just
//! means silence.

use std::f32::consts::TAU;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

/// Chord start (seconds) and MIDI notes, low string first.
const CHORDS: [(f32, [f32; 4]); 3] = [
    (0.0, [62.0, 66.0, 72.0, 76.0]),
    (0.55, [55.0, 65.0, 71.0, 76.0]),
    (1.1, [60.0, 67.0, 69.0, 76.0]),
];
const END_SECS: f32 = 3.2;
/// The steel bar lands on each chord from this many semitones below.
const SLIDE_SEMITONES: f32 = 1.0;
const SLIDE_SECS: f32 = 0.14;
const STRUM_SECS: f32 = 0.02;
const DAMP_SECS: f32 = 0.06;
const VIBRATO_HZ: f32 = 5.5;
const VIBRATO_SEMITONES: f32 = 0.18;
/// Strength and decay rate (per second) of each harmonic: bright attack that
/// settles into a near sine, the steel-string sound.
const HARMONICS: [(f32, f32); 6] = [
    (1.0, 1.2),
    (0.6, 2.2),
    (0.35, 3.5),
    (0.25, 5.0),
    (0.15, 7.0),
    (0.1, 9.0),
];
const ECHO_SECS: f32 = 0.11;
const ECHO_GAIN: f32 = 0.22;
const FADE_SECS: f32 = 0.3;
/// Loudness at full volume. The knob to turn if it startles.
const PEAK: f32 = 0.5;

/// Add one string to `out`, from `start` until `stop` (then damped).
fn pluck(out: &mut [f32], rate: f32, midi: f32, start: f32, stop: f32) {
    let first = (start * rate) as usize;
    let mut phase = 0.0f32;
    let mut amp = HARMONICS.map(|(a, _)| a);
    let decay = HARMONICS.map(|(_, d)| (-d / rate).exp());
    for (i, sample) in out.iter_mut().skip(first).enumerate() {
        let t = i as f32 / rate;
        let damp = 1.0 - ((start + t - stop) / DAMP_SECS).max(0.0);
        if damp <= 0.0 {
            break;
        }
        let glide = (t / SLIDE_SECS).min(1.0);
        let vibrato =
            VIBRATO_SEMITONES * ((t - 0.18) / 0.3).clamp(0.0, 1.0) * (TAU * VIBRATO_HZ * t).sin();
        let semis = midi - SLIDE_SEMITONES * (1.0 - glide * (2.0 - glide)) + vibrato;
        phase = (phase + TAU * 440.0 * 2f32.powf((semis - 69.0) / 12.0) / rate) % TAU;
        let mut x = 0.0;
        for (n, a) in amp.iter_mut().enumerate() {
            x += *a * ((n + 1) as f32 * phase).sin();
            *a *= decay[n];
        }
        *sample += x * damp * (t / 0.004).min(1.0);
    }
}

/// The whole sound as mono samples.
fn render(rate: u32) -> Vec<f32> {
    let r = rate as f32;
    let mut out = vec![0.0f32; (END_SECS * r) as usize];
    for (i, (start, notes)) in CHORDS.iter().enumerate() {
        let stop = CHORDS.get(i + 1).map_or(END_SECS, |c| c.0);
        for (s, midi) in notes.iter().enumerate() {
            pluck(&mut out, r, *midi, start + s as f32 * STRUM_SECS, stop);
        }
    }
    let delay = (ECHO_SECS * r) as usize;
    for i in delay..out.len() {
        out[i] += out[i - delay] * ECHO_GAIN;
    }
    let fade = (FADE_SECS * r) as usize;
    let len = out.len();
    for (i, x) in out[len - fade..].iter_mut().enumerate() {
        *x *= 1.0 - i as f32 / fade as f32;
    }
    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 0.0 {
        out.iter_mut().for_each(|x| *x *= PEAK / peak);
    }
    out
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    samples: Vec<f32>,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(config.channels.max(1));
    let mut samples = samples.into_iter();
    device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            for frame in data.chunks_mut(channels) {
                let x = samples.next().unwrap_or(0.0);
                frame.fill(<T as FromSample<f32>>::from_sample_(x));
            }
        },
        |_| {},
        None,
    )
}

fn open(volume: f32) -> Option<Stream> {
    let device = cpal::default_host().default_output_device()?;
    let default = device.default_output_config().ok()?;
    let config = default.config();
    let mut samples = render(config.sample_rate);
    samples.iter_mut().for_each(|x| *x *= volume);
    let stream = match default.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, samples),
        SampleFormat::F64 => build::<f64>(&device, &config, samples),
        SampleFormat::I16 => build::<i16>(&device, &config, samples),
        SampleFormat::I32 => build::<i32>(&device, &config, samples),
        SampleFormat::U16 => build::<u16>(&device, &config, samples),
        SampleFormat::I8 => build::<i8>(&device, &config, samples),
        SampleFormat::U8 => build::<u8>(&device, &config, samples),
        _ => return None,
    }
    .ok()?;
    stream.play().ok()?;
    Some(stream)
}

/// Play the sound in the background. Waits only until the device is open:
/// some audio stacks print diagnostics while probing, which must happen
/// before the TUI owns the screen.
pub fn play(volume: f32) {
    let (ready, opened) = mpsc::channel();
    let spawned = thread::Builder::new()
        .name("waimea-chime".into())
        .spawn(move || {
            let stream = open(volume);
            let _ = ready.send(());
            if stream.is_some() {
                thread::sleep(Duration::from_secs_f32(END_SECS + 0.2));
            }
        });
    if spawned.is_ok() {
        let _ = opened.recv_timeout(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strength of `hz` in `samples` (Goertzel).
    fn power(samples: &[f32], rate: f32, hz: f32) -> f32 {
        let k = 2.0 * (TAU * hz / rate).cos();
        let (mut a, mut b) = (0.0, 0.0);
        for x in samples {
            (a, b) = (x + k * a - b, a);
        }
        a * a + b * b - k * a * b
    }

    #[test]
    fn three_tuned_chords_within_bounds() {
        let rate = 48_000.0;
        let out = render(48_000);
        assert_eq!(out.len(), (END_SECS * rate) as usize);
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((peak - PEAK).abs() < 1e-3, "peak {peak}");
        assert!(out.last().unwrap().abs() < 1e-3, "must end silent");

        // After each chord's slide, its low string rings and a semitone up does not.
        let at = |s: f32| (s * rate) as usize;
        for (i, (start, notes)) in CHORDS.iter().enumerate() {
            let stop = CHORDS.get(i + 1).map_or(start + 1.0, |c| c.0);
            let window = &out[at(start + 0.2)..at(stop)];
            let hz = |m: f32| 440.0 * 2f32.powf((m - 69.0) / 12.0);
            let on = power(window, rate, hz(notes[0]));
            let off = power(window, rate, hz(notes[0] + 1.0));
            assert!(on > off * 2.0, "chord {i}: {on} vs {off}");
        }
    }
}
