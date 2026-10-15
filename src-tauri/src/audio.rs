//! Microphone capture. Recording starts when the hotkey goes down and stops
//! when it comes up; the result is mono 16 kHz audio, which is what the speech
//! model expects.
//!
//! The audio stream cannot move between threads on macOS, so one dedicated
//! thread owns it and takes Start/Stop commands over a channel.
//!
//! Opening a microphone takes a moment, and anything said in that moment is
//! lost. So after the first dictation the stream is kept, stopped, between
//! dictations: starting a stopped stream is much quicker than opening one.
//! A stopped stream captures nothing and does not light the microphone
//! indicator. Bluetooth headsets are the exception: holding their microphone
//! open keeps them in low-quality call mode, so theirs is released each time.

use cpal::{
    SampleFormat, Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Instant,
};

pub const TARGET_RATE: u32 = 16_000;
/// Longest single recording. Buffering stops here so a stuck key cannot eat
/// memory, and the pipeline abandons a recording that reaches it.
pub const MAX_SECONDS: usize = 300;

enum Command {
    /// Open the stream without starting it, so the first dictation is quick too.
    Prepare,
    Start(Sender<Result<(), String>>),
    Stop(Sender<Vec<f32>>),
    /// The audio recorded so far from this position on (in the device's own
    /// samples), at 16 kHz, and the position to ask from next time.
    Peek(usize, Sender<(Vec<f32>, usize)>),
}

#[derive(Clone)]
pub struct Recorder {
    commands: Sender<Command>,
    /// The loudest sample since `level` was last read, as the bits of an f32.
    loudest: Arc<AtomicU32>,
}

/// An opened microphone stream, running or stopped.
struct Ready {
    stream: Stream,
    /// The system's id for the device this stream reads from.
    device: u32,
    rate: u32,
    samples: Arc<Mutex<Vec<f32>>>,
    /// The callback keeps audio only while this is set.
    recording: Arc<AtomicBool>,
    /// Microseconds from Start to the first audio arriving; 0 until it has.
    first_audio: Arc<AtomicU64>,
    /// Set when the system reports the stream is no longer usable: its
    /// device went away, or its sample rate was changed under it.
    invalid: Arc<AtomicBool>,
}

impl Recorder {
    pub fn spawn() -> Self {
        let (commands, inbox) = mpsc::channel::<Command>();
        let loudest = Arc::new(AtomicU32::new(0));
        let gauge = loudest.clone();
        thread::Builder::new()
            .name("utter-recorder".into())
            .spawn(move || {
                let mut ready: Option<Ready> = None;
                for command in inbox {
                    match command {
                        Command::Prepare => {
                            let device = system::default_input();
                            if ready.is_none() && device != 0 && !system::is_bluetooth(device) {
                                ready = open_stream(gauge.clone(), device).ok();
                            }
                        }
                        Command::Start(reply) => {
                            let _ = reply.send(start(&mut ready, &gauge));
                        }
                        Command::Stop(reply) => {
                            let _ = reply.send(stop(&mut ready));
                        }
                        Command::Peek(from, reply) => {
                            let _ = reply.send(peek(&ready, from));
                        }
                    }
                }
            })
            .expect("could not start the recorder thread");
        Self { commands, loudest }
    }

    /// How loud the microphone has been since this was last asked, from 0
    /// (silence) to 1 (loud speech). Drives the waveform in the status pill.
    pub fn level(&self) -> f32 {
        meter(f32::from_bits(self.loudest.swap(0, Ordering::Relaxed)))
    }

    /// Get the microphone ready ahead of time. Nothing is captured and the
    /// microphone indicator stays off. Only call this once the user has
    /// allowed microphone access, or macOS will ask for it out of the blue.
    pub fn prepare(&self) {
        let _ = self.commands.send(Command::Prepare);
    }

    /// Start the default microphone and begin buffering.
    pub fn start(&self) -> Result<(), String> {
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command::Start(reply))
            .map_err(|_| "Recorder thread is gone.".to_string())?;
        result
            .recv()
            .map_err(|_| "Recorder thread is gone.".to_string())?
    }

    /// While recording: the audio captured since `from` (a position this
    /// returned last time, or 0), as 16 kHz samples, and where to ask from
    /// next. Lets speech be transcribed while the user is still talking.
    pub fn peek(&self, from: usize) -> (Vec<f32>, usize) {
        let (reply, result) = mpsc::channel();
        if self.commands.send(Command::Peek(from, reply)).is_err() {
            return (Vec::new(), from);
        }
        result.recv().unwrap_or((Vec::new(), from))
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

/// Begin a recording, reusing the stopped stream from last time when it still
/// reads from the system's current default microphone.
fn start(ready: &mut Option<Ready>, gauge: &Arc<AtomicU32>) -> Result<(), String> {
    let device = system::default_input();
    if ready.as_ref().is_some_and(|r| r.device != device || r.invalid.load(Ordering::SeqCst)) {
        // The user switched microphones since last time, or the system
        // said the stream is no longer good.
        *ready = None;
    }
    let reused = ready.is_some();
    if ready.is_none() {
        *ready = Some(open_stream(gauge.clone(), device)?);
    }

    let began = Instant::now();
    let outcome = begin(ready.as_ref().expect("stream was just ensured"), began);
    if outcome.is_err() && reused {
        // A kept stream can go stale (its device was unplugged and returned,
        // say). Open a fresh one once before giving up.
        *ready = Some(open_stream(gauge.clone(), device)?);
        return begin(ready.as_ref().expect("stream was just opened"), Instant::now());
    }
    if outcome.is_err() {
        *ready = None;
    }
    outcome
}

fn begin(ready: &Ready, began: Instant) -> Result<(), String> {
    ready.samples.lock().unwrap().clear();
    ready.first_audio.store(0, Ordering::Relaxed);
    STARTED.lock().unwrap().replace(began);
    ready.recording.store(true, Ordering::SeqCst);
    ready
        .stream
        .play()
        .map_err(|e| format!("Could not start recording: {e}"))
}

/// When the current recording was asked to start, for timing the first audio.
static STARTED: Mutex<Option<Instant>> = Mutex::new(None);

/// End the recording and hand back its audio. The stream is stopped and kept
/// for next time, unless it belongs to a Bluetooth microphone.
fn stop(ready: &mut Option<Ready>) -> Vec<f32> {
    let Some(current) = ready.as_ref() else {
        return Vec::new();
    };
    current.recording.store(false, Ordering::SeqCst);
    let raw = std::mem::take(&mut *current.samples.lock().unwrap());
    let rate = current.rate;
    #[cfg(debug_assertions)]
    eprintln!(
        "[audio] first audio arrived {:.0} ms after the key went down",
        current.first_audio.load(Ordering::Relaxed) as f64 / 1000.0
    );

    // A stream that ran for a while and delivered nothing is not worth
    // keeping either; the next dictation gets a new one.
    let ran_for = STARTED.lock().unwrap().map(|at| at.elapsed()).unwrap_or_default();
    let deaf = current.first_audio.load(Ordering::Relaxed) == 0 && ran_for > std::time::Duration::from_millis(700);
    let keep = !system::is_bluetooth(current.device)
        && !current.invalid.load(Ordering::SeqCst)
        && !deaf
        && current.stream.pause().is_ok();
    if !keep {
        *ready = None;
    }
    resample(&raw, rate, TARGET_RATE)
}

fn peek(ready: &Option<Ready>, from: usize) -> (Vec<f32>, usize) {
    let Some(current) = ready.as_ref() else {
        return (Vec::new(), from);
    };
    let (copied, end) = {
        let samples = current.samples.lock().unwrap();
        let from = from.min(samples.len());
        (samples[from..].to_vec(), samples.len())
    };
    (resample(&copied, current.rate, TARGET_RATE), end)
}

fn open_stream(gauge: Arc<AtomicU32>, device_id: u32) -> Result<Ready, String> {
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
    let recording = Arc::new(AtomicBool::new(false));
    let first_audio = Arc::new(AtomicU64::new(0));
    let limit = rate as usize * MAX_SECONDS;
    let invalid = Arc::new(AtomicBool::new(false));
    let broken = invalid.clone();
    let on_error = move |e| {
        eprintln!("[utter] microphone stream error: {e}");
        // Never reuse this stream: the next dictation opens a new one.
        broken.store(true, Ordering::SeqCst);
    };

    // Each callback delivers interleaved frames. Take the loudest channel
    // rather than averaging them: on an interface with several inputs the
    // microphone is on one of them, and averaging it with the silent ones
    // would make it that many times quieter.
    macro_rules! build {
        ($sample:ty, $to_f32:expr) => {{
            let sink = samples.clone();
            let gauge = gauge.clone();
            let recording = recording.clone();
            let first_audio = first_audio.clone();
            device.build_input_stream(
                config,
                move |data: &[$sample], _: &cpal::InputCallbackInfo| {
                    if !recording.load(Ordering::SeqCst) {
                        return;
                    }
                    if first_audio.load(Ordering::Relaxed) == 0 {
                        if let Some(began) = *STARTED.lock().unwrap() {
                            first_audio.store(began.elapsed().as_micros().max(1) as u64, Ordering::Relaxed);
                        }
                    }
                    let mut buffer = sink.lock().unwrap();
                    if buffer.len() >= limit {
                        return;
                    }
                    let mut energy = [0.0_f32; MAX_CHANNELS];
                    for frame in data.chunks_exact(channels) {
                        for (total, s) in energy.iter_mut().zip(frame) {
                            let s = $to_f32(*s);
                            *total += s * s;
                        }
                    }
                    let loudest = loudest_channel(&energy[..channels.min(MAX_CHANNELS)]);
                    let mut top = 0.0_f32;
                    buffer.extend(data.chunks_exact(channels).map(|frame| {
                        let s = $to_f32(frame[loudest]);
                        top = top.max(s.abs());
                        s
                    }));
                    // For floats that are not negative, a larger number has
                    // larger bits, so the atomic can keep the maximum.
                    gauge.fetch_max(top.to_bits(), Ordering::Relaxed);
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

    Ok(Ready {
        stream,
        device: device_id,
        rate,
        samples,
        recording,
        first_audio,
        invalid,
    })
}

/// The two facts about the system's microphones that the recorder needs,
/// read straight from Core Audio.
pub mod system {
    use std::ffi::c_void;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyData(
            object: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    const fn code(tag: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*tag)
    }

    const SYSTEM: u32 = 1;
    const GLOBAL: u32 = code(b"glob");
    const DEFAULT_INPUT: u32 = code(b"dIn ");
    const TRANSPORT: u32 = code(b"tran");
    const RUNNING_SOMEWHERE: u32 = code(b"gone");

    fn read(object: u32, selector: u32) -> Option<u32> {
        let address = Address { selector, scope: GLOBAL, element: 0 };
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &address,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut u32).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    /// The id of the default microphone; 0 if there is none.
    pub fn default_input() -> u32 {
        read(SYSTEM, DEFAULT_INPUT).unwrap_or(0)
    }

    /// True for Bluetooth and Bluetooth LE microphones.
    pub fn is_bluetooth(device: u32) -> bool {
        matches!(read(device, TRANSPORT), Some(t) if t == code(b"blue") || t == code(b"blea"))
    }

    /// True while any app is capturing from, or playing to, this device.
    pub fn in_use(device: u32) -> bool {
        read(device, RUNNING_SOMEWHERE).is_some_and(|running| running != 0)
    }
}

/// More input channels than this are not examined when picking the loudest.
const MAX_CHANNELS: usize = 32;

/// Index of the channel with the most energy; the first one on a tie.
fn loudest_channel(energy: &[f32]) -> usize {
    let mut best = 0;
    for (i, e) in energy.iter().enumerate() {
        if *e > energy[best] {
            best = i;
        }
    }
    best
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

/// Turn a peak sample into a level for display: -60 dB and below is 0, -12 dB
/// and above is 1, and the decibels in between are spread evenly. Speech sits
/// in that range, so quiet and loud talkers both move the waveform.
pub fn meter(peak: f32) -> f32 {
    let decibels = 20.0 * peak.max(1e-6).log10();
    ((decibels + 60.0) / 48.0).clamp(0.0, 1.0)
}

/// Loudest sample in the recording.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |max, s| max.max(s.abs()))
}

/// Below this peak level a recording is silence (a muted or missing mic).
const SILENCE_PEAK: f32 = 0.003;
/// Speech must be at least this many times louder than the background.
const SPEECH_TO_BACKGROUND: f32 = 2.5;

/// Does this recording look like someone talking, rather than silence or a
/// steady background noise such as a fan?
///
/// Speech models invent words when given non-speech, so this check runs
/// before transcription. It compares loud moments with quiet ones: speech
/// swings between syllables and pauses, steady noise does not.
pub fn has_speech(samples: &[f32]) -> bool {
    const FRAME: usize = TARGET_RATE as usize * 30 / 1000; // 30 ms

    if peak(samples) < SILENCE_PEAK {
        return false;
    }
    let mut loudness: Vec<f32> = samples
        .chunks_exact(FRAME)
        .map(|frame| (frame.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt())
        // A wireless microphone delivers exact zeros while it connects.
        // Those are not the room's background and must not be taken for it,
        // or any steady noise after them would look like speech.
        .filter(|level| *level > 1e-6)
        .collect();
    // Too short to judge; let the speech model decide.
    if loudness.len() < 8 {
        return true;
    }
    loudness.sort_by(f32::total_cmp);
    let background = loudness[loudness.len() / 10].max(1e-5);
    let loud = loudness[loudness.len() * 95 / 100];
    loud > background * SPEECH_TO_BACKGROUND
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

    /// A cheap repeatable noise source for tests, in the range -1..1.
    fn noise(len: usize, level: f32) -> Vec<f32> {
        let mut state: u32 = 0x2545_F491;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state as f32 / u32::MAX as f32 * 2.0 - 1.0) * level
            })
            .collect()
    }

    #[test]
    fn silence_and_steady_noise_are_not_speech() {
        assert!(!has_speech(&vec![0.0; 32_000]));
        assert!(!has_speech(&noise(32_000, 0.05)));
    }

    #[test]
    fn bursts_over_a_quiet_background_are_speech() {
        // Half a second of loud sound, half a second of faint room noise, twice.
        let mut clip = Vec::new();
        for _ in 0..2 {
            clip.extend(noise(8_000, 0.4));
            clip.extend(noise(8_000, 0.01));
        }
        assert!(has_speech(&clip));
    }

    #[test]
    fn dead_silence_at_the_start_does_not_make_noise_look_like_speech() {
        // Half a second of exact zeros (a headset connecting), then steady noise.
        let mut clip = vec![0.0_f32; 8_000];
        clip.extend(noise(32_000, 0.05));
        assert!(!has_speech(&clip));
    }

    #[test]
    fn the_loudest_channel_is_chosen() {
        assert_eq!(loudest_channel(&[0.0, 0.0, 4.0, 0.1]), 2);
        assert_eq!(loudest_channel(&[1.0, 1.0]), 0);
        assert_eq!(loudest_channel(&[0.0]), 0);
    }

    #[test]
    fn meter_spreads_speech_levels_between_zero_and_one() {
        assert_eq!(meter(0.0), 0.0);
        assert_eq!(meter(0.001), 0.0); // -60 dB
        assert_eq!(meter(1.0), 1.0);
        assert_eq!(meter(0.26), 1.0); // just above -12 dB
        // -36 dB is halfway.
        assert!((meter(10f32.powf(-36.0 / 20.0)) - 0.5).abs() < 0.001);
        assert!(meter(0.05) > meter(0.01));
    }

    /// Run with `cargo test -- --ignored`. Opens the real default microphone
    /// (no permission is needed to start it; without permission it only
    /// delivers silence) and checks that a kept stream is truly stopped.
    #[test]
    #[ignore = "uses the real microphone"]
    fn a_kept_stream_is_stopped_and_restarts_faster() {
        use std::time::Duration;
        let recorder = Recorder::spawn();
        let device = system::default_input();
        assert_ne!(device, 0, "no default microphone");
        let bluetooth = system::is_bluetooth(device);
        let busy_before = system::in_use(device);

        let cold = Instant::now();
        recorder.start().unwrap();
        let cold_ms = cold.elapsed().as_millis();
        std::thread::sleep(Duration::from_millis(600));
        assert!(system::in_use(device));
        let first = recorder.stop();
        std::thread::sleep(Duration::from_millis(400));
        if !busy_before {
            assert!(!system::in_use(device), "the microphone is still running after stop");
        }

        let warm = Instant::now();
        recorder.start().unwrap();
        let warm_ms = warm.elapsed().as_millis();
        std::thread::sleep(Duration::from_millis(600));
        let second = recorder.stop();
        eprintln!(
            "bluetooth={bluetooth} cold start {cold_ms} ms, warm start {warm_ms} ms, captured {} and {} samples",
            first.len(),
            second.len()
        );
        // About 0.6 s of 16 kHz audio each time, give or take start-up.
        assert!(first.len() > 4_000 && second.len() > 4_000);
    }

    #[test]
    fn peak_finds_the_loudest_sample() {
        assert_eq!(peak(&[0.1, -0.7, 0.3]), 0.7);
        assert_eq!(peak(&[]), 0.0);
    }
}
