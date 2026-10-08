use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStage {
    pub name: String,
    pub description: String,
    pub required_inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub gates: Vec<String>,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub stages: Vec<PipelineStage>,
}

pub struct PipelineRegistry;

impl PipelineRegistry {
    /// Load a pipeline definition from YAML file
    pub fn load_from_yaml<P: AsRef<Path>>(path: P) -> Result<PipelineDefinition> {
        let path_ref = path.as_ref();
        let content = std::fs::read_to_string(path_ref)
            .with_context(|| format!("Failed to read pipeline YAML at {:?}", path_ref))?;
        let def: PipelineDefinition = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse pipeline YAML at {:?}", path_ref))?;
        Ok(def)
    }

    /// List standard built-in production pipelines
    pub fn standard_pipelines() -> HashMap<&'static str, PipelineDefinition> {
        let mut map = HashMap::new();

        map.insert(
            "cinematic",
            PipelineDefinition {
                id: "cinematic".to_string(),
                name: "Cinematic Narrative".to_string(),
                description: "High-end visual storytelling with cinematic color grading, dramatic pacing, and dynamic camera movements".to_string(),
                version: "1.0.0".to_string(),
                stages: vec![
                    PipelineStage {
                        name: "Script & Treatment".to_string(),
                        description: "Develop screenplay, beat sheet, and character arcs".to_string(),
                        required_inputs: vec!["premise".to_string()],
                        outputs: vec!["script.json".to_string()],
                        gates: vec!["plot_hole_check".to_string()],
                        estimated_cost_usd: 0.10,
                    },
                    PipelineStage {
                        name: "Storyboard & Shotlist".to_string(),
                        description: "Generate keyframes, visual prompts, and camera directions".to_string(),
                        required_inputs: vec!["script.json".to_string()],
                        outputs: vec!["shotlist.json".to_string()],
                        gates: vec!["variety_check".to_string()],
                        estimated_cost_usd: 0.50,
                    },
                    PipelineStage {
                        name: "Video Generation".to_string(),
                        description: "Render shots with diffusion transformer video models".to_string(),
                        required_inputs: vec!["shotlist.json".to_string()],
                        outputs: vec!["raw_clips/".to_string()],
                        gates: vec!["motion_stability_check".to_string()],
                        estimated_cost_usd: 2.50,
                    },
                    PipelineStage {
                        name: "Assembly & Audio Mix".to_string(),
                        description: "Assemble timeline, mix dialogue, sound effects, and orchestral score".to_string(),
                        required_inputs: vec!["raw_clips/".to_string(), "voiceover.wav".to_string()],
                        outputs: vec!["final_master.mp4".to_string()],
                        gates: vec!["lufs_broadcast_standard".to_string()],
                        estimated_cost_usd: 0.20,
                    },
                ],
            },
        );

        map.insert(
            "animated-explainer",
            PipelineDefinition {
                id: "animated-explainer".to_string(),
                name: "Animated Explainer".to_string(),
                description: "Educational and corporate explainers with motion graphics, animated charts, and synchronized voiceover".to_string(),
                version: "1.0.0".to_string(),
                stages: vec![
                    PipelineStage {
                        name: "Concept & Script".to_string(),
                        description: "Write clear educational narration and chapter breakdowns".to_string(),
                        required_inputs: vec!["topic".to_string()],
                        outputs: vec!["script.json".to_string()],
                        gates: vec!["pacing_wpm_check".to_string()],
                        estimated_cost_usd: 0.05,
                    },
                    PipelineStage {
                        name: "Voiceover Synthesis".to_string(),
                        description: "Synthesize high-clarity voiceover track".to_string(),
                        required_inputs: vec!["script.json".to_string()],
                        outputs: vec!["voiceover.wav".to_string()],
                        gates: vec!["pronunciation_check".to_string()],
                        estimated_cost_usd: 0.15,
                    },
                    PipelineStage {
                        name: "Motion Graphics Composition".to_string(),
                        description: "Generate Remotion/HyperFrames programmatic animation components".to_string(),
                        required_inputs: vec!["script.json".to_string(), "voiceover.wav".to_string()],
                        outputs: vec!["final_render.mp4".to_string()],
                        gates: vec!["sync_timing_check".to_string()],
                        estimated_cost_usd: 0.30,
                    },
                ],
            },
        );

        map.insert(
            "avatar-spokesperson",
            PipelineDefinition {
                id: "avatar-spokesperson".to_string(),
                name: "Avatar Spokesperson".to_string(),
                description: "Photorealistic AI avatar spokesperson with lip synchronization and branded background".to_string(),
                version: "1.0.0".to_string(),
                stages: vec![
                    PipelineStage {
                        name: "Dialogue Prep".to_string(),
                        description: "Scriptwriting and phoneme alignment".to_string(),
                        required_inputs: vec!["text".to_string()],
                        outputs: vec!["speech.wav".to_string()],
                        gates: vec!["audio_quality_check".to_string()],
                        estimated_cost_usd: 0.10,
                    },
                    PipelineStage {
                        name: "Avatar Synthesis".to_string(),
                        description: "Generate talking-head video with lip sync".to_string(),
                        required_inputs: vec!["speech.wav".to_string(), "avatar_image.png".to_string()],
                        outputs: vec!["avatar_clip.mp4".to_string()],
                        gates: vec!["lip_sync_confidence_check".to_string()],
                        estimated_cost_usd: 1.20,
                    },
                    PipelineStage {
                        name: "Branded Overlay".to_string(),
                        description: "Burn subtitles, lower thirds, and logo watermarks".to_string(),
                        required_inputs: vec!["avatar_clip.mp4".to_string()],
                        outputs: vec!["final_spokesperson.mp4".to_string()],
                        gates: vec!["safe_zone_check".to_string()],
                        estimated_cost_usd: 0.05,
                    },
                ],
            },
        );

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_registry() {
        let registry = PipelineRegistry::standard_pipelines();
        assert!(registry.contains_key("cinematic"));
        assert!(registry.contains_key("animated-explainer"));
        assert!(registry.contains_key("avatar-spokesperson"));

        let cinematic = registry.get("cinematic").unwrap();
        assert_eq!(cinematic.stages.len(), 4);
    }
}
