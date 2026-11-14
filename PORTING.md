# OpenMontage Python-to-Rust Porting Plan (`tools/video/*` -> `openmontage-core::video_gen`)

## 1. Source Inventory
Ported from upstream OpenMontage Python modules (prior to commit `5e3c1cc`):
- `tools/video/_shared.py` (~70 LOC): `.env` file loading, `require_env`, output directory creation, and URL-to-file media download.
- `tools/video/seedance_video.py` (~260 LOC): ByteDance Seedance 2.0 / 2.5 cloud video generation client via fal.ai (`fal-ai/bytedance/seedance/v2.0/*`) and Volcengine Ark (`doubao-seedance-2-0-*`), supporting `text_to_video`, `image_to_video`, `reference_to_video`, and `first_last_frame_to_video`, RFC 2397 data URI encoding for local images, and cost estimation.
- `tools/video/veo_video.py` (~240 LOC): Google Veo 3.1 / Veo 3.1 Fast cloud video generation client via the Gemini REST `predictLongRunning` endpoint, long-running operation polling, and video download.
- `.agents/skills/seedance-2-5/SKILL.md`: 5-layer `#cinematic` constraint prompt compiler (`GLOBAL STYLE`, `FIRST FRAME AND SUBJECT LOCKS`, `TIMELINE`, `OPTICS / CAMERA`, `STILLNESS & NEGATIVE LOCKS`, `AUDIO`).

## 2. Type & Signature Mapping

| Python Construct (`tools/video/*`) | Rust Target ([`crates/openmontage-core/src/video_gen.rs`](file:///Users/bhubbard/PROJECTS/openmontage-rs/crates/openmontage-core/src/video_gen.rs)) |
|---|---|
| `RuntimeError` / `ValueError` exceptions | `#[derive(Debug, Error)] #[non_exhaustive] pub enum VideoGenError` |
| `operation: str` (`"text_to_video"`, `"image_to_video"`, etc.) | `pub enum VideoOperation { TextToVideo, ImageToVideo, ReferenceToVideo, FirstLastFrameToVideo }` |
| `model_variant: str` / tool selection | `pub enum VideoProvider { Seedance25, Seedance20Fast, Veo31, Veo31Fast, HeyGen }` |
| `_file_to_data_uri(path: Path) -> str` | `pub fn file_to_data_uri(path: &Path) -> Result<String, VideoGenError>` + `pub fn encode_base64(input: &[u8]) -> String` |
| `require_env(name: str) -> str` + `load_dotenv()` | `pub fn load_env_with_dotenv(dotenv_path: Option<&Path>) -> HashMap<String, String>` |
| `SeedanceVideo.execute(**kwargs) -> dict` | `VideoGenRequest::execute(&self, env: &HashMap<String, String>) -> Result<VideoGenOutput, VideoGenError>` |
| `VeoVideo.execute(**kwargs) -> dict` | `VideoGenRequest::execute_veo(&self, env: &HashMap<String, String>) -> Result<VideoGenOutput, VideoGenError>` |
| 5-layer `#cinematic` prompt templates | `pub struct CinematicShotSpec` + `CinematicShotSpec::frozen_time_car_crash()` + `CinematicShotSpec::compile_prompt()` |

## 3. Verification
- `cargo clippy --all-targets --all-features -- -D warnings` passes with 0 warnings.
- `cargo test --workspace` passes all 21 unit and integration tests, including:
  - `video_gen::tests::test_frozen_time_car_crash_spec_compilation`
  - `video_gen::tests::test_fal_payload_and_data_uri`
  - `video_gen::tests::test_seedance_and_veo_cost_estimation`
  - `video_gen::tests::test_missing_credentials_returns_clear_error`
