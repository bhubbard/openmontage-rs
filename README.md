# OpenMontage-rs (`openmontage`)

[![Rust 2021](https://img.shields.io/badge/rust-2021_edition-000000?style=flat-square&logo=rust)](https://github.com/bhubbard/openmontage-rs)
[![Live NLE & Pipeline Studio](https://img.shields.io/badge/live_NLE_studio-code.brandonhubbard.com%2Fopenmontage--rs-0055ff?style=flat-square)](https://code.brandonhubbard.com/openmontage-rs/)
[![Benchmarks](https://img.shields.io/badge/benchmarks-37.9x_faster_vs_Python-16a34a?style=flat-square)](./BENCHMARKS.md)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue?style=flat-square)](./LICENSE)

**OpenMontage-rs** is an open-source, agentic video production engine and non-linear timeline sequencer written in pure Rust (`openmontage-core` and `openmontage-cli`).

It provides **multi-stage production pipeline orchestration** (`cinematic`, `animated-explainer`, `avatar-spokesperson`), **6-track NLE timeline sequencing with primary video overlap validation**, **shotlist pacing & slideshow-risk analysis**, **deterministic multi-provider cost tracking**, and **100 ms sliding-window voiceover-masked audio ducking with `-16.0 LUFS` broadcast loudness mastering**.

- **Interactive NLE Timeline & Pacing Studio**: [https://code.brandonhubbard.com/openmontage-rs/](https://code.brandonhubbard.com/openmontage-rs/)
- **Empirical Performance Benchmarks**: [BENCHMARKS.md](./BENCHMARKS.md)

---

## Workspace Architecture

| Crate / Module | Path | Key Types & Responsibilities |
| :--- | :--- | :--- |
| **`openmontage-core::pipeline`** | [`crates/openmontage-core/src/pipeline.rs`](./crates/openmontage-core/src/pipeline.rs) | `PipelineRegistry`, `PipelineDefinition`, `PipelineStage`: built-in and YAML-configurable production DAGs (`cinematic`, `animated-explainer`, `avatar-spokesperson`) with required inputs, outputs, quality gates, and stage cost estimates. |
| **`openmontage-core::timeline`** | [`crates/openmontage-core/src/timeline.rs`](./crates/openmontage-core/src/timeline.rs) | `Timeline`, `TimelineClip`, `TrackType` (`Video`, `BRoll`, `Voiceover`, `Music`, `SoundEffects`, `Subtitles`): millisecond-accurate clip editing and `validate_primary_video_overlaps()`. |
| **`openmontage-core::shot`** | [`crates/openmontage-core/src/shot.rs`](./crates/openmontage-core/src/shot.rs) | `Shot`, `ShotType` (8 framings), `CameraMotion` (9 motions), `PacingValidator`, `PacingReport`: detects short cuts (`< 1.5s`), lingering cuts (`> 8.0s`), static slideshow risk (`>= 3` static shots), and framing variety score (`0..100%`). |
| **`openmontage-core::audio`** | [`crates/openmontage-core/src/audio.rs`](./crates/openmontage-core/src/audio.rs) | `AudioMixer` & `AudioMixConfig`: WAV I/O, 100 ms sliding-window voiceover-active background music ducking (`-12.0 dB`), K-weighted `-16.0 LUFS` loudness normalization, and soft-knee `tanh` limiting. |
| **`openmontage-core::cost`** | [`crates/openmontage-core/src/cost.rs`](./crates/openmontage-core/src/cost.rs) | `CostTracker`, `CostRecord`, `CostSummary`: multi-provider token, second, and character cost ledger with real-time budget cap enforcement (`is_budget_exceeded`). |
| **`openmontage-core::manifest`** | [`crates/openmontage-core/src/manifest.rs`](./crates/openmontage-core/src/manifest.rs) | `ProductionManifest` & `RenderedAsset`: deterministic JSON serialization of project playbooks and rendered asset metadata. |
| **`openmontage-core::config`** | [`crates/openmontage-core/src/config.rs`](./crates/openmontage-core/src/config.rs) | `ProductionConfig` & `AspectRatio` (`16:9` Landscape, `9:16` Portrait, `1:1` Square) resolution calculators. |
| **`openmontage-cli`** | [`crates/openmontage-cli/src/main.rs`](./crates/openmontage-cli/src/main.rs) | The `openmontage` CLI binary exposing `plan`, `render`, `pipelines`, `validate`, `cost`, and `bench`. |

---

## Quickstart & CLI Reference

### 1. Build and Test

```bash
git clone https://github.com/bhubbard/openmontage-rs.git
cd openmontage-rs
cargo build --workspace --release
cargo test --workspace
```

### 2. Inspect Production Pipelines & Generate a Playbook (`openmontage pipelines` / `plan`)

```bash
# List all built-in production pipelines and stage gates
cargo run --release -p openmontage-cli -- pipelines

# Generate a production manifest for a cinematic narrative reel
cargo run --release -p openmontage-cli -- plan \
  --script "Autonomous AI engineering in Rust" \
  --pipeline cinematic \
  --budget 10.0 \
  --output playbook.json
```

### 3. Validate Shotlist Pacing & Camera Variety (`openmontage validate`)

```bash
cargo run --release -p openmontage-cli -- validate --shots shotlist.json
```

### 4. Mix Voiceover & Music with Dynamic Ducking (`openmontage render`)

```bash
cargo run --release -p openmontage-cli -- render \
  --voiceover voiceover.wav \
  --music soundtrack.wav \
  --ducking -12.0 \
  --target-lufs -16.0 \
  --output master_mix.wav
```

### 5. Estimate Multi-Provider Production Costs & Run Benchmarks (`cost` / `bench`)

```bash
cargo run --release -p openmontage-cli -- cost --duration 45 --shots 12 --budget 10.0
cargo run --release -p openmontage-cli -- bench --clips 200 --sample-rate 44100
```

---

## Empirical Performance Summary

See [BENCHMARKS.md](./BENCHMARKS.md) for full methodology and per-stage telemetry.

| Subsystem / Kernel | `openmontage-rs` (Rust) | Reference Python Pipeline | Speedup |
| :--- | ---: | ---: | :--- |
| **NLE Timeline Overlap Validation** (1,000 clips) | **48.0 µs** | 1.82 ms (`moviepy`) | **37.9x faster** |
| **YAML Pipeline DAG Load** (4-stage spec) | **62.0 µs** | 2.15 ms (`PyYAML`) | **34.7x faster** |
| **Shotlist Pacing & Variety Analysis** (500 shots) | **14.2 µs** | 490.0 µs (Python) | **34.5x faster** |
| **Manifest & Cost Ledger Export** | **19.0 µs** | 610.0 µs (Python) | **32.1x faster** |
| **44.1 kHz Voiceover Ducking + LUFS** (60s audio) | **1.84 ms** | 58.40 ms (`pydub` + `numpy`) | **31.7x faster** |

---

## License

Licensed under the [Apache License, Version 2.0](./LICENSE).
