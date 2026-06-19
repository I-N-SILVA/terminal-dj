use anyhow::{Context, Result};
use ringbuf::{Consumer, HeapRb, Producer, SharedRb};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

pub struct FftSource<I> {
    inner: I,
    producer: std::sync::Arc<
        std::sync::Mutex<Producer<f32, Arc<SharedRb<f32, Vec<std::mem::MaybeUninit<f32>>>>>>,
    >,
}

impl<I> FftSource<I> {
    pub fn new(
        inner: I,
        producer: std::sync::Arc<
            std::sync::Mutex<Producer<f32, Arc<SharedRb<f32, Vec<std::mem::MaybeUninit<f32>>>>>>,
        >,
    ) -> Self {
        Self { inner, producer }
    }
}

impl<I: Source<Item = f32>> Iterator for FftSource<I> {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next();
        if let Some(sample) = next {
            if let Ok(mut p) = self.producer.try_lock() {
                let _ = p.push(sample);
            }
        }
        next
    }
}

impl<I> Source for FftSource<I>
where
    I: Source<Item = f32>,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }
    fn channels(&self) -> u16 {
        self.inner.channels()
    }
    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        self.inner.total_duration()
    }
}

pub struct AudioPlayer {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Sink,
    is_playing: bool,
    paused: bool,
    stored_volume: f32,
    pub fft_consumer: Option<Consumer<f32, Arc<SharedRb<f32, Vec<std::mem::MaybeUninit<f32>>>>>>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let (_stream, _stream_handle) =
            OutputStream::try_default().context("No audio output device")?;
        let sink = Sink::try_new(&_stream_handle).context("Could not create sink")?;
        Ok(AudioPlayer {
            _stream,
            _stream_handle,
            sink,
            is_playing: false,
            paused: false,
            stored_volume: 1.0,
            fft_consumer: None,
        })
    }

    pub fn play_file(&mut self, path: impl AsRef<Path>) -> Result<std::time::Duration> {
        if !self.sink.empty() {
            self.sink.stop();
            self.sink = Sink::try_new(&self._stream_handle)?;
        }

        self.sink.set_volume(self.stored_volume);

        let file = File::open(path)?;
        let source = Decoder::new(BufReader::new(file))?;
        let dur = source.total_duration().unwrap_or_default();

        let rb = HeapRb::<f32>::new(4096 * 4);
        let (producer, consumer) = rb.split();
        self.fft_consumer = Some(consumer);
        let producer_arc = std::sync::Arc::new(std::sync::Mutex::new(producer));

        let source_f32 = source.convert_samples::<f32>();
        let fft_source = FftSource::new(source_f32, producer_arc);

        self.sink.append(fft_source);
        self.sink.play();

        self.is_playing = true;
        self.paused = false;

        Ok(dur)
    }

    pub fn get_pos(&self) -> std::time::Duration {
        self.sink.get_pos()
    }

    pub fn set_volume(&mut self, v: f32) {
        self.stored_volume = v;
        self.sink.set_volume(v);
    }

    pub fn try_seek(&self, pos: std::time::Duration) {
        let _ = self.sink.try_seek(pos);
    }

    pub fn is_finished(&self) -> bool {
        self.is_playing && self.sink.empty()
    }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            self.sink.play();
        } else {
            self.sink.pause();
        }
        self.paused = !self.paused;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }
}
