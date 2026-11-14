//! Cloud AI video generation and 5-layer `#cinematic` shot specification compiler.
//!
//! Ported from OpenMontage `tools/video/seedance_video.py`, `tools/video/veo_video.py`,
//! `tools/video/_shared.py`, and `.agents/skills/seedance-2-5/SKILL.md`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Errors returned by the video generation subsystem.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VideoGenError {
    /// Required API key environment variable is not configured.
    #[error("missing credentials for {provider}: set {env_vars} in .env or environment")]
    MissingCredentials {
        /// Provider name.
        provider: String,
        /// Required environment variable names.
        env_vars: String,
    },

    /// Invalid combination of parameters for the chosen model/operation.
    #[error("invalid video generation parameter: {0}")]
    InvalidParameter(String),

    /// Local file I/O failure.
    #[error("file I/O error at {path}: {source}")]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },

    /// JSON serialization or deserialization failure.
    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),

    /// Remote API HTTP or transport error.
    #[error("http request failed ({context}): {details}")]
    HttpFailed {
        /// Request step context.
        context: String,
        /// Response or stderr details.
        details: String,
    },

    /// Cloud generation job failed or timed out during polling.
    #[error("{provider} video generation failed: {reason}")]
    GenerationFailed {
        /// Provider identifier.
        provider: String,
        /// Failure reason.
        reason: String,
    },
}

/// Supported video generation operations (`text_to_video`, `image_to_video`, `reference_to_video`, `first_last_frame_to_video`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VideoOperation {
    /// Generate video from a text prompt alone.
    #[default]
    TextToVideo,
    /// Animate from an opening still frame (`image_path` or `image_url`).
    ImageToVideo,
    /// Condition generation on up to 30 reference images (`reference_image_paths`).
    ReferenceToVideo,
    /// Interpolate between explicit first and last keyframe images.
    FirstLastFrameToVideo,
}

impl VideoOperation {
    /// Returns the snake_case identifier used in OpenMontage manifests.
    #[must_use]
    pub const fn as_snake_str(self) -> &'static str {
        match self {
            Self::TextToVideo => "text_to_video",
            Self::ImageToVideo => "image_to_video",
            Self::ReferenceToVideo => "reference_to_video",
            Self::FirstLastFrameToVideo => "first_last_frame_to_video",
        }
    }

    /// Returns the kebab-case path segment used by fal.ai endpoints.
    #[must_use]
    pub const fn as_kebab_str(self) -> &'static str {
        match self {
            Self::TextToVideo => "text-to-video",
            Self::ImageToVideo => "image-to-video",
            Self::ReferenceToVideo => "reference-to-video",
            Self::FirstLastFrameToVideo => "first-last-frame-to-video",
        }
    }
}

impl fmt::Display for VideoOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_snake_str())
    }
}

impl FromStr for VideoOperation {
    type Err = VideoGenError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "text_to_video" | "t2v" => Ok(Self::TextToVideo),
            "image_to_video" | "i2v" => Ok(Self::ImageToVideo),
            "reference_to_video" | "r2v" => Ok(Self::ReferenceToVideo),
            "first_last_frame_to_video" | "flf2v" => Ok(Self::FirstLastFrameToVideo),
            other => Err(VideoGenError::InvalidParameter(format!(
                "unknown operation '{other}'; expected text_to_video, image_to_video, reference_to_video, or first_last_frame_to_video"
            ))),
        }
    }
}

/// Supported cloud video generation providers and model variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum VideoProvider {
    /// ByteDance Seedance 2.5 via fal.ai (`bytedance/seedance-2.5/*`).
    #[default]
    Seedance25,
    /// ByteDance Seedance 2.0 Standard via fal.ai (`bytedance/seedance-2.0/*`).
    Seedance20,
    /// ByteDance Seedance 2.0 Fast via fal.ai (`bytedance/seedance-2.0/fast/*`).
    Seedance20Fast,
    /// Google Veo 3.1 via fal.ai (`fal-ai/veo3.1/*`).
    Veo31Fal,
    /// Google Veo 3.1 Fast via fal.ai (`fal-ai/veo3.1/fast/*`).
    Veo31FastFal,
    /// Google Veo 3.1 via direct Gemini API (`veo-3.1-generate-preview`).
    Veo31Google,
    /// HeyGen Multi-Model Gateway (`veo_3_1`, `kling_pro`, `runway_gen4`, `sora_v2_pro`).
    HeyGenVeo31,
}

impl VideoProvider {
    /// Auto-detects the highest-priority configured provider from environment variables.
    #[must_use]
    pub fn detect_from_env(env: &HashMap<String, String>) -> Option<Self> {
        let has_fal = env.get("FAL_KEY").is_some_and(|v| !v.trim().is_empty())
            || env
                .get("FAL_AI_API_KEY")
                .is_some_and(|v| !v.trim().is_empty());
        if has_fal {
            return Some(Self::Seedance25);
        }
        let has_google = env
            .get("GEMINI_API_KEY")
            .is_some_and(|v| !v.trim().is_empty())
            || env
                .get("GOOGLE_API_KEY")
                .is_some_and(|v| !v.trim().is_empty());
        if has_google {
            return Some(Self::Veo31Google);
        }
        if env
            .get("HEYGEN_API_KEY")
            .is_some_and(|v| !v.trim().is_empty())
        {
            return Some(Self::HeyGenVeo31);
        }
        None
    }

    /// Human-readable model identifier.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Seedance25 => "ByteDance Seedance 2.5 (fal.ai)",
            Self::Seedance20 => "ByteDance Seedance 2.0 Standard (fal.ai)",
            Self::Seedance20Fast => "ByteDance Seedance 2.0 Fast (fal.ai)",
            Self::Veo31Fal => "Google Veo 3.1 (fal.ai)",
            Self::Veo31FastFal => "Google Veo 3.1 Fast (fal.ai)",
            Self::Veo31Google => "Google Veo 3.1 (Gemini Direct API)",
            Self::HeyGenVeo31 => "Google Veo 3.1 (HeyGen Gateway)",
        }
    }
}

impl fmt::Display for VideoProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

impl FromStr for VideoProvider {
    type Err = VideoGenError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "seedance-2.5" | "seedance25" | "seedance_2_5" | "seedance" => Ok(Self::Seedance25),
            "seedance-2.0" | "seedance20" | "seedance_2_0" => Ok(Self::Seedance20),
            "seedance-2.0-fast" | "seedance-fast" | "seedance20fast" => Ok(Self::Seedance20Fast),
            "veo-fal" | "veo3.1-fal" | "veo31-fal" => Ok(Self::Veo31Fal),
            "veo-fast-fal" | "veo3.1-fast-fal" => Ok(Self::Veo31FastFal),
            "veo" | "veo-google" | "veo3.1" | "veo3.1-google" => Ok(Self::Veo31Google),
            "heygen" | "heygen-veo" => Ok(Self::HeyGenVeo31),
            other => Err(VideoGenError::InvalidParameter(format!(
                "unknown provider '{other}'; expected seedance-2.5, seedance-2.0, seedance-2.0-fast, veo-fal, veo-google, or heygen"
            ))),
        }
    }
}

/// A single timestamped beat in an OpenMontage `#cinematic` shot specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineBeat {
    /// Start timestamp in seconds.
    pub start_sec: f32,
    /// End timestamp in seconds.
    pub end_sec: f32,
    /// Optional shot label (e.g. `"Shot 1"`).
    pub label: Option<String>,
    /// Concrete physical action and camera behavior during this window.
    pub action: String,
}

/// Five-layer `#cinematic` shot specification matching `openmontage.video/capabilities/#cinematic`
/// and `.agents/skills/seedance-2-5/SKILL.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CinematicShotSpec {
    /// Short capability title (e.g. `"Frozen time — Car Crash"`).
    pub title: String,
    /// Natural-language "Ask Studio" prompt (1-2 sentences).
    pub ask_studio_prompt: String,
    /// Target clip duration in seconds (`4..=30`).
    pub duration_seconds: u32,
    /// Layer 1 (`GLOBAL STYLE`): photoreal look, lighting continuity matching frame 1, shutter/color rules.
    pub visual_style: String,
    /// Layer 2 (`SUBJECT & STATE LOCKS`): persistent physical objects and state monotonicity rules.
    pub subject_locks: Vec<String>,
    /// Layer 3 (`TIMESTAMPED TIMELINE`): sub-second beat breakdown.
    pub timeline_beats: Vec<TimelineBeat>,
    /// Layer 4 (`OPTICS / CAMERA`): explicit camera rig movement and banned camera tricks.
    pub camera_rules: String,
    /// Layer 5 (`NEGATIVE / STILLNESS LOCKS`): explicit exclusions (no HUD, no glowing rings, etc.).
    pub negative_constraints: Vec<String>,
    /// Audio direction (`AUDIO` section).
    pub audio_direction: String,
}

impl CinematicShotSpec {
    /// Builds the canonical 10-second single-shot **Frozen Time Car Crash** specification
    /// modeled directly on `https://www.openmontage.video/capabilities/#cinematic` (Example 2: "Frozen time").
    #[must_use]
    pub fn frozen_time_car_crash() -> Self {
        Self {
            title: "Frozen time (one instant, held) — Rainy Intersection Collision".to_string(),
            ask_studio_prompt: "A rainy city intersection freezes one instant into a two-car collision while the camera slowly moves through it. 10 seconds, one shot.".to_string(),
            duration_seconds: 10,
            visual_style: "Photoreal cinematic photography, overcast flat daylight just after heavy rain, wet reflective asphalt, cold neutral documentary colour, 35mm spherical prime lens, exactly matching the first frame. No colour or lighting change at any point.".to_string(),
            subject_locks: vec![
                "City intersection one instant into a collision between a dark charcoal sedan and a silver compact hatchback at the wet crosswalk.".to_string(),
                "One exempt pedestrian in a plain dark wool coat on the curb edge is the only entity in motion after 1.5s.".to_string(),
                "Eight specific objects hold their exact 3D state once the freeze begins at 1.5s:".to_string(),
                "1. Dark charcoal sedan with front hood buckled upward and front suspension fully compressed.".to_string(),
                "2. Silver hatchback angled 35 degrees at impact with rear left tyre lifted 12 cm off the wet tarmac.".to_string(),
                "3. Cloud of shattered tempered windshield glass fragments hanging motionless above the hood seam.".to_string(),
                "4. Fan-shaped spray of rainwater droplets and black tarmac grit suspended rigid in mid-air.".to_string(),
                "5. Detached black side-mirror housing hovering 40 cm above the white crosswalk stripe.".to_string(),
                "6. Upside-down takeaway coffee cup with dark liquid suspended rigid in air near the curb.".to_string(),
                "7. Pigeon with wings fully open 1.5 metres above the hood.".to_string(),
                "8. Cyclist near the crosswalk corner paused mid-swerve with both wheels locked.".to_string(),
            ],
            timeline_beats: vec![
                TimelineBeat {
                    start_sec: 0.0,
                    end_sec: 1.5,
                    label: Some("0 to 1.5s".to_string()),
                    action: "Normal real-time speed: charcoal sedan and silver hatchback crunch into the intersection impact; hoods buckle, suspension compresses, glass and water spray burst upward.".to_string(),
                },
                TimelineBeat {
                    start_sec: 1.5,
                    end_sec: 1.6,
                    label: Some("At 1.5s".to_string()),
                    action: "Single silent pulse crosses the frame with a subtle overcast lighting shift (no shockwave ring, no flash).".to_string(),
                },
                TimelineBeat {
                    start_sec: 1.5,
                    end_sec: 4.0,
                    label: Some("1.5 to 4.0s".to_string()),
                    action: "Everything dead-stops in position — cars, airborne glass shards, water spray, coffee cup, and pigeon hold completely rigid in 3D space — except the exempt figure in the dark coat, who steps off the curb into the street.".to_string(),
                },
                TimelineBeat {
                    start_sec: 4.0,
                    end_sec: 6.5,
                    label: Some("4.0 to 6.5s".to_string()),
                    action: "The camera tracks laterally past the suspended glass cloud and crumpled hoods while the exempt figure walks a straight line between the frozen bumper and the suspended takeaway cup.".to_string(),
                },
                TimelineBeat {
                    start_sec: 6.5,
                    end_sec: 10.0,
                    label: Some("6.5 to 10.0s".to_string()),
                    action: "The exempt figure continues walking calmly toward the far side of the intersection; the collision tableau and every suspended droplet remain 100% frozen until the end of the shot.".to_string(),
                },
            ],
            camera_rules: "One continuous 10-second shot. Slow, constant lateral dolly track from left to right at constant speed across the front of the frozen collision. 47-degree normal field of view (SINGLE-LENS LOCK). No cuts, no zoom, no whip pan, no speed variation.".to_string(),
            negative_constraints: vec![
                "No text, no HUD, no telemetry overlays, no timecode, no watermarks.".to_string(),
                "No glowing rings, no sci-fi energy ripples, no neon lens flares, no fireballs or explosions.".to_string(),
                "No slow-motion drift after 1.5s (frozen objects are 100% stopped in space, not slowly falling).".to_string(),
                "No blood or visible injury.".to_string(),
            ],
            audio_direction: "0 to 1.5s crisp wet-tyre skid and metallic crunch; at 1.5s sudden near-total acoustic vacuum with only faint quiet footsteps on wet pavement as the camera glides past. No music, no discernible dialogue.".to_string(),
        }
    }

    /// Compiles the structured 5-layer specification into the canonical Seedance 2.5 / Veo 3.1 prompt block.
    #[must_use]
    pub fn compile_prompt(&self) -> String {
        let mut out = String::with_capacity(2048);
        out.push_str("GLOBAL STYLE\n  ");
        out.push_str(&self.visual_style);
        out.push_str("\n\nSCENE\n  ");
        out.push_str(&self.ask_studio_prompt);
        out.push_str("\n\nFIRST FRAME AND SUBJECT LOCKS\n");
        for lock in &self.subject_locks {
            out.push_str("  - ");
            out.push_str(lock);
            out.push('\n');
        }
        out.push_str("\nTIMELINE (");
        out.push_str(&self.duration_seconds.to_string());
        out.push_str("s total)\n");
        for beat in &self.timeline_beats {
            out.push_str("  - ");
            if let Some(label) = &beat.label {
                out.push_str(label);
                out.push_str(": ");
            } else {
                out.push_str(&format!("{:.1} to {:.1}s: ", beat.start_sec, beat.end_sec));
            }
            out.push_str(&beat.action);
            out.push('\n');
        }
        out.push_str("\nOPTICS / CAMERA\n  ");
        out.push_str(&self.camera_rules);
        out.push_str("\n\nSTILLNESS & NEGATIVE LOCKS\n");
        for neg in &self.negative_constraints {
            out.push_str("  - ");
            out.push_str(neg);
            out.push('\n');
        }
        out.push_str("\nAUDIO\n  ");
        out.push_str(&self.audio_direction);
        out
    }

    /// Saves the structured 5-layer `#cinematic` specification to a pretty-printed JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`VideoGenError::Io`] or [`VideoGenError::Json`] if writing or serializing fails.
    pub fn save_to_file(&self, path: &Path) -> Result<(), VideoGenError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| VideoGenError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let json_bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, json_bytes).map_err(|source| VideoGenError::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

/// Request configuration for cloud AI video generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoGenRequest {
    /// Cloud provider and model variant.
    pub provider: VideoProvider,
    /// Generation operation (`text_to_video`, `image_to_video`, etc.).
    pub operation: VideoOperation,
    /// Full compiled video prompt.
    pub prompt: String,
    /// Optional negative prompt string.
    pub negative_prompt: Option<String>,
    /// Target clip duration in seconds (`4..=30`).
    pub duration_seconds: u32,
    /// Aspect ratio (`"16:9"`, `"9:16"`, `"21:9"`, `"1:1"`).
    pub aspect_ratio: String,
    /// Output resolution (`"720p"`, `"1080p"`).
    pub resolution: String,
    /// Whether to generate native synchronized audio.
    pub generate_audio: bool,
    /// Optional local start-frame image path for `image_to_video`.
    pub image_path: Option<PathBuf>,
    /// Optional remote start-frame image URL for `image_to_video`.
    pub image_url: Option<String>,
    /// Optional deterministic seed.
    pub seed: Option<u64>,
    /// Output MP4 file path.
    pub output_path: PathBuf,
}

/// Result returned from a completed cloud video generation run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoGenOutput {
    /// Provider used.
    pub provider: VideoProvider,
    /// Remote model endpoint identifier.
    pub model_id: String,
    /// Local path where the generated MP4 was written.
    pub output_path: PathBuf,
    /// File size in bytes.
    pub file_size_bytes: u64,
    /// Estimated cost in USD.
    pub estimated_cost_usd: f64,
    /// Wall-clock generation time in seconds.
    pub elapsed_seconds: f64,
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes arbitrary bytes into standard RFC 4648 Base64.
#[must_use]
pub fn encode_base64(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = if chunk.len() > 1 {
            u32::from(chunk[1])
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            u32::from(chunk[2])
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(char::from(
            BASE64_ALPHABET[((triple >> 18) & 0x3F) as usize],
        ));
        out.push(char::from(
            BASE64_ALPHABET[((triple >> 12) & 0x3F) as usize],
        ));
        if chunk.len() > 1 {
            out.push(char::from(BASE64_ALPHABET[((triple >> 6) & 0x3F) as usize]));
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(char::from(BASE64_ALPHABET[(triple & 0x3F) as usize]));
        } else {
            out.push('=');
        }
    }
    out
}

/// Converts a local image file into a `data:<mime>;base64,...` URI for fal.ai / Veo image-to-video endpoints.
///
/// # Errors
/// Returns [`VideoGenError::Io`] if the file cannot be read.
pub fn file_to_data_uri(path: &Path) -> Result<String, VideoGenError> {
    let bytes = std::fs::read(path).map_err(|source| VideoGenError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    };
    let encoded = encode_base64(&bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

/// Reads key-value pairs from a `.env` file merged with current process environment variables.
#[must_use]
pub fn load_env_with_dotenv(dotenv_path: Option<&Path>) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = std::env::vars().collect();
    let candidate = dotenv_path.unwrap_or_else(|| Path::new(".env"));
    if let Ok(content) = std::fs::read_to_string(candidate) {
        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim();
                let mut val = v.trim();
                if let Some(comment_idx) = val.find(" #") {
                    val = val[..comment_idx].trim();
                }
                val = val.trim_matches('"').trim_matches('\'');
                if !val.is_empty() && !map.get(key).is_some_and(|existing| !existing.is_empty()) {
                    map.insert(key.to_string(), val.to_string());
                }
            }
        }
    }
    map
}

impl VideoGenRequest {
    /// Estimates the USD cost of this generation request, matching `SeedanceVideo.estimate_cost`
    /// and `VeoVideo.estimate_cost`.
    #[must_use]
    pub fn estimate_cost_usd(&self) -> f64 {
        let secs = f64::from(self.duration_seconds.max(4));
        let raw = match self.provider {
            VideoProvider::Seedance25 => 0.30 * secs,
            VideoProvider::Seedance20 => 0.3034 * secs,
            VideoProvider::Seedance20Fast => 0.2419 * secs,
            VideoProvider::Veo31Fal => {
                let rate = if self.resolution == "4k" {
                    if self.generate_audio {
                        0.60
                    } else {
                        0.40
                    }
                } else if self.generate_audio {
                    0.40
                } else {
                    0.20
                };
                rate * secs
            }
            VideoProvider::Veo31FastFal => {
                let rate = if self.generate_audio { 0.20 } else { 0.10 };
                rate * secs
            }
            VideoProvider::Veo31Google | VideoProvider::HeyGenVeo31 => 0.40 * secs,
        };
        (raw * 100.0).round() / 100.0
    }

    /// Builds the `(model_endpoint_path, json_payload)` tuple for fal.ai queue submission.
    ///
    /// # Errors
    /// Returns [`VideoGenError::InvalidParameter`] if required image inputs are missing or
    /// [`VideoGenError::Io`] if a local image path cannot be read.
    pub fn build_fal_payload(&self) -> Result<(String, Value), VideoGenError> {
        let op_kebab = self.operation.as_kebab_str();
        let model_path = match self.provider {
            VideoProvider::Seedance25 => format!("bytedance/seedance-2.5/{op_kebab}"),
            VideoProvider::Seedance20 => format!("bytedance/seedance-2.0/{op_kebab}"),
            VideoProvider::Seedance20Fast => format!("bytedance/seedance-2.0/fast/{op_kebab}"),
            VideoProvider::Veo31Fal => match self.operation {
                VideoOperation::TextToVideo => "fal-ai/veo3.1".to_string(),
                _ => format!("fal-ai/veo3.1/{op_kebab}"),
            },
            VideoProvider::Veo31FastFal => match self.operation {
                VideoOperation::TextToVideo => "fal-ai/veo3.1/fast".to_string(),
                _ => format!("fal-ai/veo3.1/fast/{op_kebab}"),
            },
            other => {
                return Err(VideoGenError::InvalidParameter(format!(
                    "provider {other} is not a fal.ai route"
                )))
            }
        };

        let is_seedance = matches!(
            self.provider,
            VideoProvider::Seedance25 | VideoProvider::Seedance20 | VideoProvider::Seedance20Fast
        );

        let duration_str = if is_seedance {
            self.duration_seconds.clamp(4, 30).to_string()
        } else {
            format!("{}s", self.duration_seconds)
        };

        let aspect = if self.provider == VideoProvider::Seedance25
            && self.operation == VideoOperation::ImageToVideo
        {
            "auto"
        } else {
            self.aspect_ratio.as_str()
        };

        let mut payload = json!({
            "prompt": self.prompt,
            "duration": duration_str,
            "aspect_ratio": aspect,
            "resolution": self.resolution,
            "generate_audio": self.generate_audio,
        });

        if let Some(neg) = &self.negative_prompt {
            payload["negative_prompt"] = Value::String(neg.clone());
        }
        if let Some(seed) = self.seed {
            payload["seed"] = Value::from(seed);
        }

        if self.operation == VideoOperation::ImageToVideo {
            let image_uri = if let Some(url) = &self.image_url {
                url.clone()
            } else if let Some(path) = &self.image_path {
                file_to_data_uri(path)?
            } else {
                return Err(VideoGenError::InvalidParameter(
                    "image_to_video requires --image <path> or --image-url <url>".to_string(),
                ));
            };
            payload["image_url"] = Value::String(image_uri);
        }

        Ok((model_path, payload))
    }

    /// Executes the cloud video generation job via the configured gateway and downloads the MP4.
    ///
    /// # Errors
    /// Returns [`VideoGenError`] if credentials are missing, HTTP requests fail, or generation errors.
    pub fn execute(&self, env: &HashMap<String, String>) -> Result<VideoGenOutput, VideoGenError> {
        match self.provider {
            VideoProvider::Seedance25
            | VideoProvider::Seedance20
            | VideoProvider::Seedance20Fast
            | VideoProvider::Veo31Fal
            | VideoProvider::Veo31FastFal => self.execute_fal(env),
            VideoProvider::Veo31Google => self.execute_google_veo(env),
            VideoProvider::HeyGenVeo31 => self.execute_heygen(env),
        }
    }

    fn execute_fal(&self, env: &HashMap<String, String>) -> Result<VideoGenOutput, VideoGenError> {
        let api_key = env
            .get("FAL_KEY")
            .or_else(|| env.get("FAL_AI_API_KEY"))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| VideoGenError::MissingCredentials {
                provider: self.provider.display_name().to_string(),
                env_vars: "FAL_KEY (get one at https://fal.ai/dashboard/keys)".to_string(),
            })?;

        let start = Instant::now();
        let (model_path, payload) = self.build_fal_payload()?;
        let submit_url = if model_path.starts_with("fal-ai/") {
            format!("https://queue.fal.run/{model_path}")
        } else {
            format!("https://queue.fal.run/fal-ai/{model_path}")
        };

        let auth_header = format!("Authorization: Key {api_key}");
        let body_str = serde_json::to_string(&payload)?;

        let submit_raw = curl_json_post(&submit_url, &[&auth_header], &body_str, "fal.ai submit")?;
        let queue_data: Value = serde_json::from_str(&submit_raw)?;
        let status_url =
            queue_data["status_url"]
                .as_str()
                .ok_or_else(|| VideoGenError::GenerationFailed {
                    provider: model_path.clone(),
                    reason: format!("missing status_url in fal.ai response: {submit_raw}"),
                })?;
        let response_url =
            queue_data["response_url"]
                .as_str()
                .ok_or_else(|| VideoGenError::GenerationFailed {
                    provider: model_path.clone(),
                    reason: format!("missing response_url in fal.ai response: {submit_raw}"),
                })?;

        let deadline = Instant::now() + Duration::from_secs(600);
        loop {
            if Instant::now() >= deadline {
                return Err(VideoGenError::GenerationFailed {
                    provider: model_path,
                    reason: "timed out after 600s waiting for queue completion".to_string(),
                });
            }
            thread::sleep(Duration::from_secs(5));
            let status_raw = curl_get(status_url, &[&auth_header], "fal.ai poll")?;
            let status_json: Value = serde_json::from_str(&status_raw)?;
            let status = status_json["status"].as_str().unwrap_or("UNKNOWN");
            match status {
                "COMPLETED" => break,
                "FAILED" | "CANCELLED" => {
                    return Err(VideoGenError::GenerationFailed {
                        provider: model_path,
                        reason: format!("job status {status}: {status_raw}"),
                    });
                }
                _ => {}
            }
        }

        let result_raw = curl_get(response_url, &[&auth_header], "fal.ai result")?;
        let result_json: Value = serde_json::from_str(&result_raw)?;
        let video_url = result_json["video"]["url"]
            .as_str()
            .or_else(|| result_json["url"].as_str())
            .ok_or_else(|| VideoGenError::GenerationFailed {
                provider: model_path.clone(),
                reason: format!("missing video.url in response: {result_raw}"),
            })?;

        curl_download(video_url, &[], &self.output_path)?;
        let meta = std::fs::metadata(&self.output_path).map_err(|source| VideoGenError::Io {
            path: self.output_path.clone(),
            source,
        })?;

        Ok(VideoGenOutput {
            provider: self.provider,
            model_id: model_path,
            output_path: self.output_path.clone(),
            file_size_bytes: meta.len(),
            estimated_cost_usd: self.estimate_cost_usd(),
            elapsed_seconds: start.elapsed().as_secs_f64(),
        })
    }

    fn execute_google_veo(
        &self,
        env: &HashMap<String, String>,
    ) -> Result<VideoGenOutput, VideoGenError> {
        let api_key = env
            .get("GEMINI_API_KEY")
            .or_else(|| env.get("GOOGLE_API_KEY"))
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| VideoGenError::MissingCredentials {
                provider: self.provider.display_name().to_string(),
                env_vars: "GEMINI_API_KEY or GOOGLE_API_KEY (https://aistudio.google.com/apikey)"
                    .to_string(),
            })?;

        let start = Instant::now();
        let model_name = "veo-3.1-generate-preview";
        let submit_url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{model_name}:predictLongRunning"
        );
        let key_header = format!("x-goog-api-key: {api_key}");

        let mut instance = json!({ "prompt": self.prompt });
        if self.operation == VideoOperation::ImageToVideo {
            if let Some(path) = &self.image_path {
                let bytes = std::fs::read(path).map_err(|source| VideoGenError::Io {
                    path: path.clone(),
                    source,
                })?;
                let mime = if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                {
                    "image/png"
                } else {
                    "image/jpeg"
                };
                instance["image"] = json!({
                    "bytesBase64Encoded": encode_base64(&bytes),
                    "mimeType": mime,
                });
            }
        }

        let payload = json!({
            "instances": [instance],
            "parameters": {
                "aspectRatio": self.aspect_ratio,
                "durationSeconds": 8,
                "sampleCount": 1,
            }
        });

        let body_str = serde_json::to_string(&payload)?;
        let submit_raw =
            curl_json_post(&submit_url, &[&key_header], &body_str, "Google Veo submit")?;
        let op_json: Value = serde_json::from_str(&submit_raw)?;
        let op_name = op_json["name"]
            .as_str()
            .ok_or_else(|| VideoGenError::GenerationFailed {
                provider: model_name.to_string(),
                reason: format!("missing operation name in response: {submit_raw}"),
            })?;

        let poll_url = format!("https://generativelanguage.googleapis.com/v1beta/{op_name}");
        let deadline = Instant::now() + Duration::from_secs(600);
        let final_json = loop {
            if Instant::now() >= deadline {
                return Err(VideoGenError::GenerationFailed {
                    provider: model_name.to_string(),
                    reason: "timed out after 600s waiting for Veo operation".to_string(),
                });
            }
            thread::sleep(Duration::from_secs(5));
            let poll_raw = curl_get(&poll_url, &[&key_header], "Google Veo poll")?;
            let poll_val: Value = serde_json::from_str(&poll_raw)?;
            if poll_val["done"].as_bool() == Some(true) {
                break poll_val;
            }
        };

        if let Some(err) = final_json.get("error") {
            return Err(VideoGenError::GenerationFailed {
                provider: model_name.to_string(),
                reason: err.to_string(),
            });
        }

        let video_uri = final_json["response"]["generateVideoResponse"]["generatedSamples"][0]
            ["video"]["uri"]
            .as_str()
            .ok_or_else(|| VideoGenError::GenerationFailed {
                provider: model_name.to_string(),
                reason: format!("missing video uri in Veo response: {final_json}"),
            })?;

        curl_download(video_uri, &[&key_header], &self.output_path)?;
        let meta = std::fs::metadata(&self.output_path).map_err(|source| VideoGenError::Io {
            path: self.output_path.clone(),
            source,
        })?;

        Ok(VideoGenOutput {
            provider: self.provider,
            model_id: model_name.to_string(),
            output_path: self.output_path.clone(),
            file_size_bytes: meta.len(),
            estimated_cost_usd: self.estimate_cost_usd(),
            elapsed_seconds: start.elapsed().as_secs_f64(),
        })
    }

    fn execute_heygen(
        &self,
        env: &HashMap<String, String>,
    ) -> Result<VideoGenOutput, VideoGenError> {
        let api_key = env
            .get("HEYGEN_API_KEY")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| VideoGenError::MissingCredentials {
                provider: self.provider.display_name().to_string(),
                env_vars: "HEYGEN_API_KEY".to_string(),
            })?;

        let start = Instant::now();
        let auth_header = format!("X-Api-Key: {api_key}");
        let mut input = json!({
            "prompt": self.prompt,
            "provider": "veo_3_1",
            "aspect_ratio": self.aspect_ratio,
        });
        if let Some(url) = &self.image_url {
            input["reference_image_url"] = Value::String(url.clone());
        }
        let payload = json!({
            "workflow_type": "GenerateVideoNode",
            "input": input,
        });

        let submit_raw = curl_json_post(
            "https://api.heygen.com/v1/workflows/executions",
            &[&auth_header],
            &serde_json::to_string(&payload)?,
            "HeyGen submit",
        )?;
        let submit_json: Value = serde_json::from_str(&submit_raw)?;
        let exec_id = submit_json["data"]["execution_id"]
            .as_str()
            .ok_or_else(|| VideoGenError::GenerationFailed {
                provider: "heygen/veo_3_1".to_string(),
                reason: format!("missing execution_id: {submit_raw}"),
            })?;

        let poll_url = format!("https://api.heygen.com/v1/workflows/executions/{exec_id}");
        let deadline = Instant::now() + Duration::from_secs(600);
        let video_url = loop {
            if Instant::now() >= deadline {
                return Err(VideoGenError::GenerationFailed {
                    provider: "heygen/veo_3_1".to_string(),
                    reason: "timed out after 600s".to_string(),
                });
            }
            thread::sleep(Duration::from_secs(6));
            let poll_raw = curl_get(&poll_url, &[&auth_header], "HeyGen poll")?;
            let poll_json: Value = serde_json::from_str(&poll_raw)?;
            let status = poll_json["data"]["status"].as_str().unwrap_or("");
            if status == "completed" {
                let url = poll_json["data"]["output"]["video"]["video_url"]
                    .as_str()
                    .or_else(|| poll_json["data"]["output"]["video_url"].as_str())
                    .ok_or_else(|| VideoGenError::GenerationFailed {
                        provider: "heygen/veo_3_1".to_string(),
                        reason: format!("completed without video_url: {poll_raw}"),
                    })?;
                break url.to_string();
            }
            if status == "failed" || status == "error" {
                return Err(VideoGenError::GenerationFailed {
                    provider: "heygen/veo_3_1".to_string(),
                    reason: format!("HeyGen failed: {poll_raw}"),
                });
            }
        };

        curl_download(&video_url, &[], &self.output_path)?;
        let meta = std::fs::metadata(&self.output_path).map_err(|source| VideoGenError::Io {
            path: self.output_path.clone(),
            source,
        })?;

        Ok(VideoGenOutput {
            provider: self.provider,
            model_id: "heygen/veo_3_1".to_string(),
            output_path: self.output_path.clone(),
            file_size_bytes: meta.len(),
            estimated_cost_usd: self.estimate_cost_usd(),
            elapsed_seconds: start.elapsed().as_secs_f64(),
        })
    }
}

fn curl_json_post(
    url: &str,
    headers: &[&str],
    json_body: &str,
    context: &str,
) -> Result<String, VideoGenError> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS",
        "--fail-with-body",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json",
    ]);
    for h in headers {
        cmd.args(["-H", h]);
    }
    cmd.args(["-d", json_body, url]);
    let out = cmd.output().map_err(|e| VideoGenError::HttpFailed {
        context: context.to_string(),
        details: e.to_string(),
    })?;
    if !out.status.success() {
        let body = String::from_utf8_lossy(&out.stdout);
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(VideoGenError::HttpFailed {
            context: context.to_string(),
            details: format!("{err} {body}").trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn curl_get(url: &str, headers: &[&str], context: &str) -> Result<String, VideoGenError> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "--fail-with-body"]);
    for h in headers {
        cmd.args(["-H", h]);
    }
    cmd.arg(url);
    let out = cmd.output().map_err(|e| VideoGenError::HttpFailed {
        context: context.to_string(),
        details: e.to_string(),
    })?;
    if !out.status.success() {
        let body = String::from_utf8_lossy(&out.stdout);
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(VideoGenError::HttpFailed {
            context: context.to_string(),
            details: format!("{err} {body}").trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn curl_download(url: &str, headers: &[&str], dest: &Path) -> Result<(), VideoGenError> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|source| VideoGenError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-L", "--fail"]);
    for h in headers {
        cmd.args(["-H", h]);
    }
    cmd.arg("-o").arg(dest).arg(url);
    let out = cmd.output().map_err(|e| VideoGenError::HttpFailed {
        context: "download video".to_string(),
        details: e.to_string(),
    })?;
    if !out.status.success() {
        return Err(VideoGenError::HttpFailed {
            context: "download video".to_string(),
            details: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_frozen_time_car_crash_spec_compilation() {
        let spec = CinematicShotSpec::frozen_time_car_crash();
        assert_eq!(spec.duration_seconds, 10);
        let prompt = spec.compile_prompt();
        assert!(prompt.contains("GLOBAL STYLE"));
        assert!(prompt.contains("FIRST FRAME AND SUBJECT LOCKS"));
        assert!(prompt.contains("TIMELINE (10s total)"));
        assert!(prompt.contains("OPTICS / CAMERA"));
        assert!(prompt.contains("STILLNESS & NEGATIVE LOCKS"));
        assert!(prompt.contains("No text, no HUD"));
        assert!(prompt.contains("Everything dead-stops in position"));
    }

    #[test]
    fn test_seedance_and_veo_cost_estimation() {
        let mut req = VideoGenRequest {
            provider: VideoProvider::Seedance25,
            operation: VideoOperation::ImageToVideo,
            prompt: "Test".to_string(),
            negative_prompt: None,
            duration_seconds: 10,
            aspect_ratio: "16:9".to_string(),
            resolution: "720p".to_string(),
            generate_audio: true,
            image_path: None,
            image_url: Some("https://example.com/frame0.jpg".to_string()),
            seed: Some(42),
            output_path: PathBuf::from("out.mp4"),
        };
        assert!((req.estimate_cost_usd() - 3.00).abs() < 1e-6);

        req.provider = VideoProvider::Seedance20Fast;
        assert!((req.estimate_cost_usd() - 2.42).abs() < 1e-6);

        req.provider = VideoProvider::Veo31Google;
        assert!((req.estimate_cost_usd() - 4.00).abs() < 1e-6);
    }

    #[test]
    fn test_fal_payload_and_data_uri() {
        let tmp = NamedTempFile::new().expect("temp file created");
        std::fs::write(tmp.path(), b"fake-jpeg-bytes").expect("wrote bytes");

        let req = VideoGenRequest {
            provider: VideoProvider::Seedance25,
            operation: VideoOperation::ImageToVideo,
            prompt: "Frozen time crash".to_string(),
            negative_prompt: None,
            duration_seconds: 10,
            aspect_ratio: "16:9".to_string(),
            resolution: "720p".to_string(),
            generate_audio: true,
            image_path: Some(tmp.path().to_path_buf()),
            image_url: None,
            seed: Some(7),
            output_path: PathBuf::from("out.mp4"),
        };

        let (model_path, payload) = req.build_fal_payload().expect("payload built");
        assert_eq!(model_path, "bytedance/seedance-2.5/image-to-video");
        // Seedance 2.5 image_to_video forces aspect_ratio = "auto" per seedance_video.py line 151
        assert_eq!(payload["aspect_ratio"], "auto");
        assert_eq!(payload["duration"], "10");
        assert!(payload["image_url"]
            .as_str()
            .expect("image_url string")
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_missing_credentials_returns_clear_error() {
        let empty_env = HashMap::new();
        let req = VideoGenRequest {
            provider: VideoProvider::Seedance25,
            operation: VideoOperation::TextToVideo,
            prompt: "Test".to_string(),
            negative_prompt: None,
            duration_seconds: 5,
            aspect_ratio: "16:9".to_string(),
            resolution: "720p".to_string(),
            generate_audio: true,
            image_path: None,
            image_url: None,
            seed: None,
            output_path: PathBuf::from("out.mp4"),
        };
        let err = req
            .execute(&empty_env)
            .expect_err("should fail without key");
        assert!(err.to_string().contains("FAL_KEY"));
    }
}
