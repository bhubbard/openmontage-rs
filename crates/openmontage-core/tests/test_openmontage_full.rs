use openmontage_core::{
    audio::{AudioMixConfig, AudioMixer},
    config::{AspectRatio, ProductionConfig},
    cost::CostTracker,
    manifest::{ProductionManifest, RenderedAsset},
    pipeline::PipelineRegistry,
    shot::{CameraMotion, PacingValidator, Shot, ShotType},
    timeline::{Timeline, TimelineClip, TrackType},
};
use std::path::PathBuf;
use tempfile::NamedTempFile;

#[test]
fn test_aspect_ratio_and_dimensions() {
    let landscape = AspectRatio::Landscape16x9;
    let (w1, h1) = landscape.dimensions(1080);
    assert_eq!((w1, h1), (1920, 1080));

    let portrait = AspectRatio::Portrait9x16;
    let (w2, h2) = portrait.dimensions(1080);
    assert_eq!((w2, h2), (1080, 1920));

    let square = AspectRatio::Square1x1;
    let (w3, h3) = square.dimensions(720);
    assert_eq!((w3, h3), (720, 720));
}

#[test]
fn test_production_config_defaults() {
    let cfg = ProductionConfig::default();
    assert_eq!(cfg.project_name, "untitled_production");
    assert_eq!(cfg.aspect_ratio, AspectRatio::Landscape16x9);
    assert_eq!(cfg.fps, 30.0);
    assert_eq!(cfg.width, 1920);
    assert_eq!(cfg.height, 1080);
    assert_eq!(cfg.budget_usd, 10.0);
}

#[test]
fn test_shot_pacing_validator() {
    let shots = vec![
        Shot {
            id: "shot1".to_string(),
            shot_type: ShotType::Wide,
            motion: CameraMotion::PanLeft,
            description: "Opening establishing shot of skyline".to_string(),
            duration_seconds: 4.0,
            visual_prompt: "Cinematic drone shot of skyline at golden hour".to_string(),
        },
        Shot {
            id: "shot2".to_string(),
            shot_type: ShotType::Medium,
            motion: CameraMotion::Tracking,
            description: "Protagonist walking down corridor".to_string(),
            duration_seconds: 3.5,
            visual_prompt: "Medium shot of character walking with neon rim lighting".to_string(),
        },
        Shot {
            id: "shot3".to_string(),
            shot_type: ShotType::CloseUp,
            motion: CameraMotion::ZoomIn,
            description: "Close-up of device activation".to_string(),
            duration_seconds: 2.0,
            visual_prompt: "Macro close-up of futuristic cybernetic interface".to_string(),
        },
    ];

    let report = PacingValidator::analyze(&shots);
    assert_eq!(report.total_shots, 3);
    assert_eq!(report.total_duration_seconds, 9.5);
    assert!((report.average_shot_duration - 3.1666).abs() < 0.01);
    assert_eq!(report.short_shots_count, 0);
    assert_eq!(report.long_shots_count, 0);
    assert!(!report.slideshow_risk);
    assert!(report.variety_score > 0.5);

    // Test empty shots
    let empty_report = PacingValidator::analyze(&[]);
    assert_eq!(empty_report.total_shots, 0);
    assert_eq!(empty_report.total_duration_seconds, 0.0);
}

#[test]
fn test_pacing_slideshow_risk() {
    let static_shots = vec![
        Shot {
            id: "s1".to_string(),
            shot_type: ShotType::Wide,
            motion: CameraMotion::Static,
            description: "Stat 1".to_string(),
            duration_seconds: 3.0,
            visual_prompt: "Static".to_string(),
        },
        Shot {
            id: "s2".to_string(),
            shot_type: ShotType::Wide,
            motion: CameraMotion::Static,
            description: "Stat 2".to_string(),
            duration_seconds: 3.0,
            visual_prompt: "Static".to_string(),
        },
        Shot {
            id: "s3".to_string(),
            shot_type: ShotType::Wide,
            motion: CameraMotion::Static,
            description: "Stat 3".to_string(),
            duration_seconds: 3.0,
            visual_prompt: "Static".to_string(),
        },
    ];

    let report = PacingValidator::analyze(&static_shots);
    assert!(report.slideshow_risk);
}

#[test]
fn test_timeline_sequencer() {
    let mut timeline = Timeline::new("Test Feature", 24.0);

    let v1 = TimelineClip {
        id: "clip1".to_string(),
        track_type: TrackType::Video,
        source_path: "assets/shot_001.mp4".to_string(),
        in_point_ms: 0,
        out_point_ms: 4000,
        timeline_start_ms: 0,
        volume: 1.0,
        opacity: 1.0,
    };
    let v2 = TimelineClip {
        id: "clip2".to_string(),
        track_type: TrackType::Video,
        source_path: "assets/shot_002.mp4".to_string(),
        in_point_ms: 500,
        out_point_ms: 3500,
        timeline_start_ms: 4000,
        volume: 1.0,
        opacity: 1.0,
    };
    let audio = TimelineClip {
        id: "music1".to_string(),
        track_type: TrackType::Music,
        source_path: "assets/bgm.wav".to_string(),
        in_point_ms: 0,
        out_point_ms: 7000,
        timeline_start_ms: 0,
        volume: 0.7,
        opacity: 1.0,
    };

    timeline.add_clip(v1);
    timeline.add_clip(v2);
    timeline.add_clip(audio);

    assert_eq!(timeline.total_duration_ms(), 7000);
    assert_eq!(timeline.clips_on_track(TrackType::Video).len(), 2);
    assert_eq!(timeline.clips_on_track(TrackType::Music).len(), 1);
    assert!(timeline.validate_primary_video_overlaps());
}

#[test]
fn test_timeline_overlap_detection() {
    let mut timeline = Timeline::new("Overlap Test", 30.0);
    timeline.add_clip(TimelineClip {
        id: "c1".to_string(),
        track_type: TrackType::Video,
        source_path: "a.mp4".to_string(),
        in_point_ms: 0,
        out_point_ms: 3000,
        timeline_start_ms: 0,
        volume: 1.0,
        opacity: 1.0,
    });
    // Starts before c1 ends!
    timeline.add_clip(TimelineClip {
        id: "c2".to_string(),
        track_type: TrackType::Video,
        source_path: "b.mp4".to_string(),
        in_point_ms: 0,
        out_point_ms: 3000,
        timeline_start_ms: 2000,
        volume: 1.0,
        opacity: 1.0,
    });

    assert!(!timeline.validate_primary_video_overlaps());
}

#[test]
fn test_audio_mixer_and_ducking() {
    let sample_rate = 16000;
    let duration_samples = sample_rate * 2; // 2 seconds

    let mut voiceover = vec![0.0f32; duration_samples];
    // Fill 1 second of voiceover activity
    for i in (sample_rate / 2)..(sample_rate * 3 / 2) {
        voiceover[i] = 0.5 * (i as f32 * 0.1).sin();
    }

    let music = vec![0.4f32; duration_samples];
    let config = AudioMixConfig {
        sample_rate: sample_rate as u32,
        voiceover_gain: 1.0,
        music_gain: 0.5,
        ducking_db: -12.0,
        target_lufs: -14.0,
    };

    let mixed = AudioMixer::mix_tracks(&voiceover, &music, &config);
    assert_eq!(mixed.len(), duration_samples);

    // Write to a temp wav file and read it back
    let temp_wav = NamedTempFile::new().unwrap();
    AudioMixer::save_wav(temp_wav.path(), &mixed, sample_rate as u32).unwrap();

    let (loaded_samples, loaded_rate) = AudioMixer::load_wav(temp_wav.path()).unwrap();
    assert_eq!(loaded_rate, sample_rate as u32);
    assert_eq!(loaded_samples.len(), duration_samples);
}

#[test]
fn test_pipeline_registry_standards() {
    let pipelines = PipelineRegistry::standard_pipelines();
    assert!(pipelines.contains_key("cinematic"));
    assert!(pipelines.contains_key("animated-explainer"));
    assert!(pipelines.contains_key("avatar-spokesperson"));

    let cinematic = pipelines.get("cinematic").unwrap();
    assert_eq!(cinematic.id, "cinematic");
    assert_eq!(cinematic.version, "1.0.0");
    assert_eq!(cinematic.stages.len(), 4);

    let stage_names: Vec<&str> = cinematic.stages.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        stage_names,
        vec![
            "Script & Treatment",
            "Storyboard & Shotlist",
            "Video Generation",
            "Assembly & Audio Mix"
        ]
    );

    let explainer = pipelines.get("animated-explainer").unwrap();
    assert_eq!(explainer.stages.len(), 3);
}

#[test]
fn test_cost_tracker() {
    let mut tracker = CostTracker::new(20.0);
    assert_eq!(tracker.remaining_budget(), 20.0);
    assert!(!tracker.is_budget_exceeded());

    tracker.record("claude-3-opus", "screenplay", 5000.0, "tokens", 0.000075);
    tracker.record("runway-gen3", "video_diffusion", 15.0, "seconds", 0.25);
    tracker.record("elevenlabs", "voiceover", 1200.0, "characters", 0.0003);

    let summary = tracker.summary();
    assert!(summary.total_cost_usd > 4.0);
    assert!(summary.remaining_usd < 16.0);
    assert_eq!(summary.records.len(), 3);
    assert!(!tracker.is_budget_exceeded());

    // Record an expensive step that blows the budget
    tracker.record("human-vfx", "manual_paint", 1.0, "shots", 25.0);
    assert!(tracker.is_budget_exceeded());
}

#[test]
fn test_production_manifest_persistence() {
    let manifest = ProductionManifest {
        project_name: "Cyberpunk Short Film".to_string(),
        pipeline_id: "cinematic".to_string(),
        total_duration_seconds: 45.0,
        total_shots: 12,
        rendered_assets: vec![
            RenderedAsset {
                name: "shot_001".to_string(),
                path: PathBuf::from("renders/shot_001.mp4"),
                asset_type: "video/mp4".to_string(),
                duration_seconds: Some(4.0),
                file_size_bytes: 15_420_000,
            },
            RenderedAsset {
                name: "voiceover".to_string(),
                path: PathBuf::from("audio/narration.wav"),
                asset_type: "audio/wav".to_string(),
                duration_seconds: Some(45.0),
                file_size_bytes: 8_820_000,
            },
        ],
        cost_usd: 8.75,
    };

    let temp = NamedTempFile::new().unwrap();
    manifest.save_to_file(temp.path()).unwrap();

    let loaded = ProductionManifest::load_from_file(temp.path()).unwrap();
    assert_eq!(loaded.project_name, "Cyberpunk Short Film");
    assert_eq!(loaded.pipeline_id, "cinematic");
    assert_eq!(loaded.total_shots, 12);
    assert_eq!(loaded.rendered_assets.len(), 2);
    assert_eq!(loaded.cost_usd, 8.75);
}
