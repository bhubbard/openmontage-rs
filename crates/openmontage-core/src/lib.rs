//! openmontage-core: Core video production engine, timeline sequencer, pipeline orchestration, and audio mixing for OpenMontage.

pub mod audio;
pub mod config;
pub mod cost;
pub mod imf_color;
pub mod manifest;
pub mod pipeline;
pub mod shot;
pub mod timeline;
pub mod video_gen;

pub use audio::{AudioMixConfig, AudioMixer};
pub use config::{AspectRatio, ProductionConfig};
pub use cost::{CostRecord, CostSummary, CostTracker};
pub use imf_color::{
    ImfApplicationProfile, ImfPackageValidator, OpenVpCalibrator, TetrahedralLut3D,
};
pub use manifest::{ProductionManifest, RenderedAsset};
pub use pipeline::{PipelineDefinition, PipelineRegistry, PipelineStage};
pub use shot::{CameraMotion, PacingReport, PacingValidator, Shot, ShotType};
pub use timeline::{Timeline, TimelineClip, TrackType};
pub use video_gen::{
    encode_base64, file_to_data_uri, load_env_with_dotenv, CinematicShotSpec, TimelineBeat,
    VideoGenError, VideoGenOutput, VideoGenRequest, VideoOperation, VideoProvider,
};
