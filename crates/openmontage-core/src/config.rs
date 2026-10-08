use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AspectRatio {
    #[serde(rename = "16:9")]
    Landscape16x9,
    #[serde(rename = "9:16")]
    Portrait9x16,
    #[serde(rename = "1:1")]
    Square1x1,
}

impl Default for AspectRatio {
    fn default() -> Self {
        AspectRatio::Landscape16x9
    }
}

impl AspectRatio {
    pub fn dimensions(&self, quality_height: u32) -> (u32, u32) {
        match self {
            AspectRatio::Landscape16x9 => ((quality_height * 16) / 9, quality_height),
            AspectRatio::Portrait9x16 => (quality_height, (quality_height * 16) / 9),
            AspectRatio::Square1x1 => (quality_height, quality_height),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionConfig {
    pub project_name: String,
    pub aspect_ratio: AspectRatio,
    pub fps: f32,
    pub height: u32,
    pub width: u32,
    pub budget_usd: f64,
    pub output_dir: PathBuf,
}

impl Default for ProductionConfig {
    fn default() -> Self {
        let aspect = AspectRatio::Landscape16x9;
        let (width, height) = aspect.dimensions(1080);
        Self {
            project_name: "untitled_production".to_string(),
            aspect_ratio: aspect,
            fps: 30.0,
            height,
            width,
            budget_usd: 10.0,
            output_dir: PathBuf::from("output"),
        }
    }
}
