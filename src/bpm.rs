use anyhow::Result;

// ─── TrackAnalysis ────────────────────────────────────────────────────────────

pub struct TrackAnalysis {
    pub bpm:       Option<f32>,
    pub norm_gain: f32,
    pub waveform:  Vec<f32>,   // 200 normalised peak values (0..1)
    pub mood:       Option<String>,
}

/// Decode a file once and compute BPM, RMS-based norm gain, and waveform.
pub fn analyze_file(path: &std::path::Path) -> Result<TrackAnalysis> {
    use rodio::{Decoder, Source};
    use std::{fs::File, io::BufReader};

    let file = File::open(path)?;
    let decoder = Decoder::new(BufReader::new(file))?;
    let sample_rate = decoder.sample_rate();
    let channels    = decoder.channels() as usize;
    let max_samples = sample_rate as usize * 60 * channels;

    let samples: Vec<f32> = decoder.convert_samples::<f32>().take(max_samples).collect();

    // Mix down to mono.
    let mono: Vec<f32> = if channels > 1 {
        samples.chunks_exact(channels)
            .map(|ch| ch.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    let bpm = detect_bpm(&mono, sample_rate);

    // RMS normalisation — target 0.1 RMS ≈ −20 dBFS, clamp [0.25, 4.0].
    let rms = if mono.is_empty() {
        0.1
    } else {
        (mono.iter().map(|x| x * x).sum::<f32>() / mono.len() as f32).sqrt()
    };
    let norm_gain = if rms > 1e-6 {
        (0.1_f32 / rms).clamp(0.25, 4.0)
    } else {
        1.0
    };

    // Waveform: 200 peak values, normalised 0..1.
    const POINTS: usize = 200;
    let waveform: Vec<f32> = if mono.is_empty() {
        vec![0.0; POINTS]
    } else {
        let chunk = (mono.len() / POINTS).max(1);
        (0..POINTS).map(|i| {
            let s = i * chunk;
            let e = ((i + 1) * chunk).min(mono.len());
            mono[s..e].iter().map(|&x| x.abs()).fold(0.0f32, f32::max)
        }).collect()
    };
    let wf_max = waveform.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
    let waveform = waveform.iter().map(|&x| x / wf_max).collect();

    // bliss-rs placeholder (to avoid compilation errors without ffmpeg/system deps)
    let mood = None;

    Ok(TrackAnalysis { bpm, norm_gain, waveform, mood })
}

/// Autocorrelation-based BPM detector.
pub fn detect_bpm(samples: &[f32], sample_rate: u32) -> Option<f32> {
    let sr = sample_rate as usize;
    if samples.len() < sr * 5 {
        return None;
    }

    let window = (sr / 10).max(1);
    let hop    = (window / 2).max(1);

    let energies: Vec<f32> = (0..samples.len().saturating_sub(window))
        .step_by(hop)
        .map(|i| {
            let s = &samples[i..i + window];
            s.iter().map(|x| x * x).sum::<f32>() / window as f32
        })
        .collect();

    if energies.len() < 30 {
        return None;
    }

    let novelty: Vec<f32> = energies.windows(2)
        .map(|w| (w[1] - w[0]).max(0.0))
        .collect();

    let hops_per_sec = sr as f32 / hop as f32;
    let min_lag = (hops_per_sec * 60.0 / 220.0).ceil() as usize;
    let max_lag = ((hops_per_sec * 60.0 / 40.0) as usize).min(novelty.len() / 2);

    if min_lag >= max_lag {
        return None;
    }

    let n = novelty.len();
    let (best_lag, _best) = (min_lag..=max_lag)
        .map(|lag| {
            let corr: f32 = novelty[..n - lag]
                .iter()
                .zip(&novelty[lag..])
                .map(|(&a, &b)| a * b)
                .sum();
            (lag, corr)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;

    let bpm = hops_per_sec * 60.0 / best_lag as f32;
    let bpm = if bpm < 60.0 { bpm * 2.0 } else if bpm > 200.0 { bpm / 2.0 } else { bpm };

    if !(40.0..=220.0).contains(&bpm) {
        return None;
    }

    Some(bpm.round())
}
