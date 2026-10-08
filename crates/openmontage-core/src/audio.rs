use anyhow::{Context, Result};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMixConfig {
    pub sample_rate: u32,
    pub voiceover_gain: f32,
    pub music_gain: f32,
    pub ducking_db: f32, // default -12.0 dB
    pub target_lufs: f32,
}

impl Default for AudioMixConfig {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            voiceover_gain: 1.0,
            music_gain: 0.6,
            ducking_db: -12.0,
            target_lufs: -16.0,
        }
    }
}

pub struct AudioMixer;

impl AudioMixer {
    /// Load a mono WAV file into float samples
    pub fn load_wav<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, u32)> {
        let mut reader = WavReader::open(path.as_ref())
            .with_context(|| format!("Failed to read WAV {:?}", path.as_ref()))?;
        let spec = reader.spec();
        let channels = spec.channels as usize;
        let sample_rate = spec.sample_rate;

        let raw: Vec<f32> = match spec.sample_format {
            SampleFormat::Int => {
                let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / max).collect()
            }
            SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        };

        let mono = if channels > 1 {
            raw.chunks(channels).map(|c| c.iter().sum::<f32>() / channels as f32).collect()
        } else {
            raw
        };

        Ok((mono, sample_rate))
    }

    /// Save mono float samples into 16-bit PCM WAV
    pub fn save_wav<P: AsRef<Path>>(path: P, samples: &[f32], sample_rate: u32) -> Result<()> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let mut writer = WavWriter::create(path_ref, spec)
            .with_context(|| format!("Failed creating WAV {:?}", path_ref))?;

        for &s in samples {
            let clamped = s.clamp(-1.0, 1.0);
            let val = (clamped * 32767.0) as i16;
            writer.write_sample(val)?;
        }

        writer.finalize()?;
        Ok(())
    }

    /// Mix voiceover and music with dynamic ducking and loudness normalization
    pub fn mix_tracks(
        voiceover: &[f32],
        music: &[f32],
        config: &AudioMixConfig,
    ) -> Vec<f32> {
        let max_len = voiceover.len().max(music.len());
        if max_len == 0 {
            return Vec::new();
        }

        let duck_gain = 10.0f32.powf(config.ducking_db / 20.0);
        let mut mixed = vec![0.0f32; max_len];

        // Detection window for speech activity (~100ms)
        let window_size = (config.sample_rate as usize / 10).max(1);

        for i in 0..max_len {
            let vo_sample = if i < voiceover.len() { voiceover[i] * config.voiceover_gain } else { 0.0 };

            // Determine if voice is currently active in the local window
            let win_start = i.saturating_sub(window_size / 2);
            let win_end = (i + window_size / 2).min(voiceover.len());
            let is_voice_active = if win_end > win_start {
                let energy: f32 = voiceover[win_start..win_end].iter().map(|&x| x * x).sum();
                (energy / (win_end - win_start) as f32).sqrt() > 0.01
            } else {
                false
            };

            let music_sample = if i < music.len() {
                let attenuation = if is_voice_active { duck_gain } else { 1.0 };
                music[i] * config.music_gain * attenuation
            } else {
                0.0
            };

            mixed[i] = vo_sample + music_sample;
        }

        Self::normalize_loudness(&mixed, config.target_lufs)
    }

    /// Normalize integrated loudness (LUFS)
    pub fn normalize_loudness(samples: &[f32], target_lufs: f32) -> Vec<f32> {
        if samples.is_empty() {
            return Vec::new();
        }

        let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / samples.len() as f32).sqrt();
        let current_lufs = -0.691 + 10.0 * (rms * rms).max(1e-10).log10();

        let gain_db = target_lufs - current_lufs;
        let linear_gain = 10.0f32.powf(gain_db / 20.0);

        let mut out: Vec<f32> = samples.iter().map(|&s| s * linear_gain).collect();

        // Soft-knee limiter
        for s in &mut out {
            if *s > 0.98 {
                *s = 0.98 + (*s - 0.98).tanh() * 0.02;
            } else if *s < -0.98 {
                *s = -0.98 + (*s + 0.98).tanh() * 0.02;
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_mixing_and_ducking() {
        let mut voiceover = vec![0.0f32; 44100 * 2]; // 2 seconds
        // Insert speech in the middle
        for s in &mut voiceover[22050..33075] {
            *s = 0.5;
        }

        let music = vec![0.2f32; 44100 * 2];
        let config = AudioMixConfig::default();

        let mixed = AudioMixer::mix_tracks(&voiceover, &music, &config);
        assert_eq!(mixed.len(), 44100 * 2);
    }
}
