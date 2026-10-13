//! Microphone capture. Recording starts when the hotkey goes down and stops
//! when it comes up; the result is mono 16 kHz audio, which is what the speech
//! model expects.
//!
//! The audio stream cannot move between threads on macOS, so one dedicated
//! thread owns it and takes Start/Stop commands over a channel.

use cpal::{
    SampleFormat, Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
};

pub const TARGET_RATE: u32 = 16_000;
/// Stop buffering after this long so a stuck key cannot eat memory.
const MAX_SECONDS: usize = 300;

enum Command {
    Start(Sender<Result<(), String>>),
    Stop(Sender<Vec<f32>>),
}

#[derive(Clone)]
pub struct Recorder {
    commands: Sender<Command>,
}

/// What the recording thread holds while a recording is in progress.
struct Active {
    /// Dropping the stream stops capture and releases the microphone.
    _stream: Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    rate: u32,
}

impl Recorder {
    pub fn spawn() -> Self {
        let (commands, inbox) = mpsc::channel::<Command>();
        thread::Builder::new()
            .name("utter-recorder".into())
            .spawn(move || {
                let mut active: Option<Active> = None;
                for command in inbox {
                    match command {
                        Command::Start(reply) => {
                            let result = open_stream().map(|a| active = Some(a));
                            let _ = reply.send(result);
                        }
                        Command::Stop(reply) => {
                            let audio = match active.take() {
                                Some(a) => {
                                    let raw = std::mem::take(&mut *a.samples.lock().unwrap());
                                    resample(&raw, a.rate, TARGET_RATE)
                                }
                                None => Vec::new(),
                            };
                            let _ = reply.send(audio);
                        }
                    }
                }
            })
            .expect("could not start the recorder thread");
        Self { commands }
    }

    /// Open the default microphone and begin buffering.
    pub fn start(&self) -> Result<(), String> {
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command::Start(reply))
            .map_err(|_| "Recorder thread is gone.".to_string())?;
        result
            .recv()
            .map_err(|_| "Recorder thread is gone.".to_string())?
    }

    /// Stop and return everything captured, as mono 16 kHz samples.
    pub fn stop(&self) -> Vec<f32> {
        let (reply, result) = mpsc::channel();
        if self.commands.send(Command::Stop(reply)).is_err() {
            return Vec::new();
        }
        result.recv().unwrap_or_default()
    }
}

fn open_stream() -> Result<Active, String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("No microphone found.")?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Could not read the microphone format: {e}"))?;

    let rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config = supported.config();

    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let limit = rate as usize * MAX_SECONDS;
    let on_error = |e| eprintln!("[utter] microphone stream error: {e}");

    // Each callback delivers interleaved frames; average the channels to mono.
    macro_rules! build {
        ($sample:ty, $to_f32:expr) => {{
            let sink = samples.clone();
            device.build_input_stream(
                config,
                move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                    let mut buffer = sink.lock().unwrap();
                    if buffer.len() >= limit {
                        return;
                    }
                    for frame in data.chunks(channels) {
                        let sum: f32 = frame.iter().map(|s| $to_f32(*s)).sum();
                        buffer.push(sum / channels as f32);
                    }
                },
                on_error,
                None,
            )
        }};
    }

    let stream = match format {
        SampleFormat::F32 => build!(f32, |s: f32| s),
        SampleFormat::I16 => build!(i16, |s: i16| s as f32 / 32_768.0),
        SampleFormat::U16 => build!(u16, |s: u16| (s as f32 - 32_768.0) / 32_768.0),
        SampleFormat::I32 => build!(i32, |s: i32| s as f32 / 2_147_483_648.0),
        other => return Err(format!("Unsupported microphone sample format: {other}")),
    }
    .map_err(|e| format!("Could not open the microphone: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("Could not start recording: {e}"))?;

    Ok(Active {
        _stream: stream,
        samples,
        rate,
    })
}

/// Convert between sample rates by averaging the source samples that fall
/// inside each output sample's time window. Averaging acts as a simple low-pass
/// filter, which keeps downsampling (48 kHz to 16 kHz is the common case) from
/// folding high frequencies back into the speech band.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let step = from as f64 / to as f64;
    let out_len = (input.len() as f64 / step).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let start = (i as f64 * step).floor() as usize;
        let end = (((i + 1) as f64 * step).ceil() as usize)
            .max(start + 1)
            .min(input.len());
        let window = &input[start..end];
        out.push(window.iter().sum::<f32>() / window.len() as f32);
    }
    out
}

/// Loudest sample in the recording; used to skip transcription of silence.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |max, s| max.max(s.abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsampling_48k_to_16k_keeps_duration() {
        let one_second = vec![0.5_f32; 48_000];
        let out = resample(&one_second, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn non_integer_ratio_is_handled() {
        let out = resample(&vec![0.25_f32; 44_100], 44_100, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.25).abs() < 1e-6));
    }

    #[test]
    fn same_rate_is_a_copy() {
        let input = vec![0.1, -0.2, 0.3];
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }

    #[test]
    fn upsampling_does_not_panic_or_shrink() {
        let out = resample(&vec![0.5_f32; 8_000], 8_000, 16_000);
        assert_eq!(out.len(), 16_000);
    }

    #[test]
    fn peak_finds_the_loudest_sample() {
        assert_eq!(peak(&[0.1, -0.7, 0.3]), 0.7);
        assert_eq!(peak(&[]), 0.0);
    }
}
