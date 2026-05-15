use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};
use ringbuf::{HeapRb, Producer};

pub type AudioProducer = Producer<f32, Arc<HeapRb<f32>>>;

// ─── VisualizerSource ────────────────────────────────────────────────────────

pub struct VisualizerSource<I: Source<Item = f32>> {
    input:    I,
    producer: Arc<Mutex<AudioProducer>>,
}

impl<I: Source<Item = f32>> Iterator for VisualizerSource<I> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let s = self.input.next()?;
        if let Ok(mut p) = self.producer.lock() { p.push(s).ok(); }
        Some(s)
    }
}

impl<I: Source<Item = f32>> Source for VisualizerSource<I> {
    fn current_frame_len(&self) -> Option<usize>           { self.input.current_frame_len() }
    fn channels(&self)          -> u16                     { self.input.channels() }
    fn sample_rate(&self)       -> u32                     { self.input.sample_rate() }
    fn total_duration(&self)    -> Option<std::time::Duration> { self.input.total_duration() }
}

// ─── AudioPlayer ─────────────────────────────────────────────────────────────

pub struct AudioPlayer {
    _stream:        OutputStream,
    stream_handle:  OutputStreamHandle,
    sink:           Sink,
    fade_sink:      Option<Sink>,   // incoming track during crossfade
    is_playing:     bool,
    paused:         bool,
    xfade_active:   bool,
    xfade_progress: f32,            // 0 → 1 over the crossfade window
    xfade_secs:     f32,
    stored_volume:  f32,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let (_stream, stream_handle) =
            OutputStream::try_default().context("No audio output device")?;
        let sink = Sink::try_new(&stream_handle).context("Could not create sink")?;
        Ok(AudioPlayer {
            _stream,
            stream_handle,
            sink,
            fade_sink: None,
            is_playing: false,
            paused: false,
            xfade_active: false,
            xfade_progress: 0.0,
            xfade_secs: 0.0,
            stored_volume: 1.0,
        })
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn append_source(
        sink: &Sink,
        path: &Path,
        producer: Arc<Mutex<AudioProducer>>,
        eq_gains: Arc<Mutex<[f32; crate::eq::BAND_COUNT]>>,
    ) -> Result<std::time::Duration> {
        let file   = File::open(path)?;
        let source = Decoder::new(BufReader::new(file))?;
        let dur    = source.total_duration().unwrap_or_default();
        let vis = VisualizerSource {
            input: crate::eq::EqSource::new(source.convert_samples::<f32>(), eq_gains),
            producer,
        };
        sink.append(vis);
        Ok(dur)
    }

    // ── Public API ────────────────────────────────────────────────────────────

    pub fn play_file(
        &mut self,
        path: impl AsRef<Path>,
        producer: Arc<Mutex<AudioProducer>>,
        eq_gains: Arc<Mutex<[f32; crate::eq::BAND_COUNT]>>,
    ) -> Result<std::time::Duration> {
        // Abort any ongoing crossfade first.
        if let Some(old) = self.fade_sink.take() { old.stop(); }
        self.xfade_active   = false;
        self.xfade_progress = 0.0;

        if !self.sink.empty() {
            self.sink.stop();
            self.sink = Sink::try_new(&self.stream_handle)?;
        }
        self.sink.set_volume(self.stored_volume);
        let dur = Self::append_source(&self.sink, path.as_ref(), producer, eq_gains)?;
        self.sink.play();
        self.is_playing = true;
        self.paused = false;
        Ok(dur)
    }

    /// Start fading in `path` over `xfade_secs` while the current track fades out.
    /// Returns the duration of the incoming track.
    pub fn start_crossfade(
        &mut self,
        path: &Path,
        producer: Arc<Mutex<AudioProducer>>,
        eq_gains: Arc<Mutex<[f32; crate::eq::BAND_COUNT]>>,
        xfade_secs: f32,
    ) -> Result<std::time::Duration> {
        if xfade_secs <= 0.0 { return self.play_file(path, producer, eq_gains); }

        let new_sink = Sink::try_new(&self.stream_handle)?;
        new_sink.set_volume(0.0);
        let dur = Self::append_source(&new_sink, path, producer, eq_gains)?;
        new_sink.play();
        if self.paused { new_sink.pause(); }

        self.fade_sink      = Some(new_sink);
        self.xfade_active   = true;
        self.xfade_progress = 0.0;
        self.xfade_secs     = xfade_secs;
        Ok(dur)
    }

    /// Advance the crossfade by `dt` seconds. Returns true when the crossfade
    /// finished and `sink` now points at the new track.
    pub fn update_crossfade(&mut self, dt: f32) -> bool {
        if !self.xfade_active { return false; }

        self.xfade_progress = (self.xfade_progress + dt / self.xfade_secs).min(1.0);
        let v = self.stored_volume;
        self.sink.set_volume(v * (1.0 - self.xfade_progress));
        if let Some(ref fs) = self.fade_sink {
            fs.set_volume(v * self.xfade_progress);
        }

        if self.xfade_progress >= 1.0 {
            self.sink.stop();
            if let Some(new_sink) = self.fade_sink.take() {
                self.sink = new_sink;
                self.sink.set_volume(v);
            }
            self.xfade_active   = false;
            self.xfade_progress = 0.0;
            return true;
        }
        false
    }

    /// Play raw audio bytes (e.g., downloaded SoundCloud stream).
    pub fn play_bytes(
        &mut self,
        data: Vec<u8>,
        producer: Arc<Mutex<AudioProducer>>,
        eq_gains: Arc<Mutex<[f32; crate::eq::BAND_COUNT]>>,
    ) -> Result<std::time::Duration> {
        if let Some(old) = self.fade_sink.take() { old.stop(); }
        self.xfade_active   = false;
        self.xfade_progress = 0.0;

        if !self.sink.empty() {
            self.sink.stop();
            self.sink = Sink::try_new(&self.stream_handle)?;
        }
        self.sink.set_volume(self.stored_volume);

        let cursor = std::io::Cursor::new(data);
        let source = Decoder::new(BufReader::new(cursor))?;
        let dur    = source.total_duration().unwrap_or_default();
        let vis = VisualizerSource {
            input:    crate::eq::EqSource::new(source.convert_samples::<f32>(), eq_gains),
            producer,
        };
        self.sink.append(vis);
        self.sink.play();
        self.is_playing = true;
        self.paused     = false;
        Ok(dur)
    }

    pub fn get_pos(&self) -> std::time::Duration { self.sink.get_pos() }

    pub fn set_volume(&self, v: f32) {
        if self.xfade_active {
            let out = v * (1.0 - self.xfade_progress);
            self.sink.set_volume(out);
            if let Some(ref fs) = self.fade_sink { fs.set_volume(v * self.xfade_progress); }
        } else {
            self.sink.set_volume(v);
        }
    }

    pub fn store_volume(&mut self, v: f32) {
        self.stored_volume = v;
        self.set_volume(v);
    }

    pub fn try_seek(&self, pos: std::time::Duration) { let _ = self.sink.try_seek(pos); }

    pub fn is_finished(&self) -> bool {
        if self.xfade_active { return false; }
        self.is_playing && self.sink.empty()
    }

    pub fn mark_stopped(&mut self) {
        self.is_playing = false;
        self.paused     = false;
    }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            self.sink.play();
            if let Some(ref fs) = self.fade_sink { fs.play(); }
        } else {
            self.sink.pause();
            if let Some(ref fs) = self.fade_sink { fs.pause(); }
        }
        self.paused = !self.paused;
    }

    pub fn is_paused(&self)       -> bool { self.paused }
    pub fn is_crossfading(&self)  -> bool { self.xfade_active }
}
