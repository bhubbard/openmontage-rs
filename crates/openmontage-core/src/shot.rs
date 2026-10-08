use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShotType {
    Wide,
    Medium,
    CloseUp,
    ExtremeCloseUp,
    OverTheShoulder,
    AerialDrone,
    Macro,
    Pov,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CameraMotion {
    Static,
    PanLeft,
    PanRight,
    TiltUp,
    TiltDown,
    ZoomIn,
    ZoomOut,
    Tracking,
    Orbit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shot {
    pub id: String,
    pub shot_type: ShotType,
    pub motion: CameraMotion,
    pub description: String,
    pub duration_seconds: f32,
    pub visual_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacingReport {
    pub total_shots: usize,
    pub total_duration_seconds: f32,
    pub average_shot_duration: f32,
    pub short_shots_count: usize, // < 1.5s
    pub long_shots_count: usize,  // > 8.0s
    pub slideshow_risk: bool,     // too many static shots in a row
    pub variety_score: f32,       // 0.0 to 1.0
}

pub struct PacingValidator;

impl PacingValidator {
    pub fn analyze(shots: &[Shot]) -> PacingReport {
        if shots.is_empty() {
            return PacingReport {
                total_shots: 0,
                total_duration_seconds: 0.0,
                average_shot_duration: 0.0,
                short_shots_count: 0,
                long_shots_count: 0,
                slideshow_risk: false,
                variety_score: 1.0,
            };
        }

        let mut total_duration = 0.0f32;
        let mut short_count = 0;
        let mut long_count = 0;
        let mut max_consecutive_static = 0;
        let mut current_consecutive_static = 0;

        let mut unique_shot_types = std::collections::HashSet::new();

        for shot in shots {
            total_duration += shot.duration_seconds;
            unique_shot_types.insert(shot.shot_type);

            if shot.duration_seconds < 1.5 {
                short_count += 1;
            } else if shot.duration_seconds > 8.0 {
                long_count += 1;
            }

            if shot.motion == CameraMotion::Static {
                current_consecutive_static += 1;
                if current_consecutive_static > max_consecutive_static {
                    max_consecutive_static = current_consecutive_static;
                }
            } else {
                current_consecutive_static = 0;
            }
        }

        let avg_duration = total_duration / shots.len() as f32;
        let slideshow_risk = max_consecutive_static >= 3;
        let variety_score = (unique_shot_types.len() as f32 / 4.0).min(1.0);

        PacingReport {
            total_shots: shots.len(),
            total_duration_seconds: total_duration,
            average_shot_duration: avg_duration,
            short_shots_count: short_count,
            long_shots_count: long_count,
            slideshow_risk,
            variety_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pacing_analysis() {
        let shots = vec![
            Shot {
                id: "s1".to_string(),
                shot_type: ShotType::Wide,
                motion: CameraMotion::PanLeft,
                description: "Opening cityscape".to_string(),
                duration_seconds: 3.5,
                visual_prompt: "Cinematic city at dawn".to_string(),
            },
            Shot {
                id: "s2".to_string(),
                shot_type: ShotType::Medium,
                motion: CameraMotion::Tracking,
                description: "Hero walking".to_string(),
                duration_seconds: 4.0,
                visual_prompt: "Hero walking through street".to_string(),
            },
            Shot {
                id: "s3".to_string(),
                shot_type: ShotType::CloseUp,
                motion: CameraMotion::ZoomIn,
                description: "Close-up on eyes".to_string(),
                duration_seconds: 2.0,
                visual_prompt: "Intense gaze close-up".to_string(),
            },
        ];

        let report = PacingValidator::analyze(&shots);
        assert_eq!(report.total_shots, 3);
        assert!(!report.slideshow_risk);
        assert!(report.variety_score > 0.5);
    }
}
