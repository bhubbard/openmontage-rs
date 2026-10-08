use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedAsset {
    pub name: String,
    pub path: PathBuf,
    pub asset_type: String,
    pub duration_seconds: Option<f32>,
    pub file_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionManifest {
    pub project_name: String,
    pub pipeline_id: String,
    pub total_duration_seconds: f32,
    pub total_shots: usize,
    pub rendered_assets: Vec<RenderedAsset>,
    pub cost_usd: f64,
}

impl ProductionManifest {
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string_pretty(self)
            .context("Failed serializing production manifest")?;
        std::fs::write(path_ref, json)?;
        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let content = std::fs::read_to_string(path_ref)
            .with_context(|| format!("Failed to read manifest at {:?}", path_ref))?;
        let manifest: Self = serde_json::from_str(&content)?;
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_manifest_serde() {
        let manifest = ProductionManifest {
            project_name: "Demo Reel".to_string(),
            pipeline_id: "cinematic".to_string(),
            total_duration_seconds: 30.0,
            total_shots: 8,
            rendered_assets: vec![
                RenderedAsset {
                    name: "master_video".to_string(),
                    path: PathBuf::from("master.mp4"),
                    asset_type: "video/mp4".to_string(),
                    duration_seconds: Some(30.0),
                    file_size_bytes: 12500000,
                },
            ],
            cost_usd: 3.45,
        };

        let temp_file = NamedTempFile::new().unwrap();
        manifest.save_to_file(temp_file.path()).unwrap();
        let loaded = ProductionManifest::load_from_file(temp_file.path()).unwrap();

        assert_eq!(loaded.project_name, "Demo Reel");
        assert_eq!(loaded.rendered_assets.len(), 1);
    }
}
