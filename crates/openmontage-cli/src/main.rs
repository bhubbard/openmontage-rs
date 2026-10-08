use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use openmontage_core::{
    AudioMixConfig, AudioMixer, CostTracker, PacingValidator,
    PipelineRegistry, ProductionManifest, Shot, Timeline,
    TimelineClip, TrackType,
};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    name = "openmontage",
    version,
    about = "High-performance CLI for OpenMontage: Agentic Video Production Studio",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate a production playbook from a script or prompt
    Plan {
        /// Production script or prompt text
        #[arg(short = 's', long = "script")]
        script: String,

        /// Pipeline template ID (cinematic, animated-explainer, avatar-spokesperson)
        #[arg(short = 'p', long = "pipeline", default_value = "cinematic")]
        pipeline: String,

        /// Output playbook JSON path
        #[arg(short = 'o', long = "output")]
        output: PathBuf,

        /// Production budget in USD
        #[arg(long = "budget", default_value = "10.0")]
        budget: f64,
    },

    /// Render and mix timeline audio tracks (dialogue + music ducking)
    Render {
        /// Voiceover dialogue WAV file
        #[arg(short = 'v', long = "voiceover")]
        voiceover: PathBuf,

        /// Optional background music WAV file
        #[arg(short = 'm', long = "music")]
        music: Option<PathBuf>,

        /// Output mixed audio WAV file
        #[arg(short = 'o', long = "output")]
        output: PathBuf,

        /// Background music ducking in dB (default: -12.0)
        #[arg(long = "ducking", default_value = "-12.0")]
        ducking: f32,

        /// Target loudness in LUFS (default: -16.0)
        #[arg(long = "target-lufs", default_value = "-16.0")]
        target_lufs: f32,
    },

    /// List all available video production pipelines and their stages
    Pipelines,

    /// Validate shotlist pacing, slideshow risk, and visual variety
    Validate {
        /// Path to JSON shotlist file
        #[arg(short = 's', long = "shots")]
        shots: PathBuf,
    },

    /// Estimate production cost and resource usage
    Cost {
        /// Estimated video duration in seconds
        #[arg(short = 'd', long = "duration", default_value = "30")]
        duration: f64,

        /// Estimated number of generated shots
        #[arg(short = 's', long = "shots", default_value = "8")]
        shots: usize,

        /// Total budget cap in USD
        #[arg(long = "budget", default_value = "10.0")]
        budget: f64,
    },

    /// Benchmark timeline assembly and audio mixing throughput
    Bench {
        /// Number of timeline clips to simulate
        #[arg(long = "clips", default_value = "50")]
        clips: usize,

        /// Sample rate for benchmark audio mixing
        #[arg(long = "sample-rate", default_value = "44100")]
        sample_rate: u32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Plan {
            script,
            pipeline,
            output,
            budget,
        } => {
            println!(
                "{} OpenMontage production planning with [{}] pipeline...",
                "Starting".cyan().bold(),
                pipeline.yellow().bold()
            );
            println!("  Script/Prompt: \"{}\"", script);
            println!("  Budget Cap   : ${:.2} USD", budget);
            println!("  Output Path  : {}", output.display().to_string().green());

            let pipelines = PipelineRegistry::standard_pipelines();
            let def = pipelines.get(pipeline.as_str())
                .with_context(|| format!("Unknown pipeline: {}", pipeline))?;

            println!("\n{}", "Pipeline Stages:".bold());
            let mut total_est_cost = 0.0;
            for (idx, stage) in def.stages.iter().enumerate() {
                println!("  [{}] {:<30} Est: ${:.2}", idx + 1, stage.name, stage.estimated_cost_usd);
                total_est_cost += stage.estimated_cost_usd;
            }

            let manifest = ProductionManifest {
                project_name: "OpenMontage Production".to_string(),
                pipeline_id: pipeline,
                total_duration_seconds: 30.0,
                total_shots: 8,
                rendered_assets: vec![],
                cost_usd: total_est_cost,
            };

            manifest.save_to_file(&output)?;

            println!("\n{}", "Playbook Created!".green().bold());
            println!("  Estimated Cost: ${:.2} USD", total_est_cost);
            println!("  Saved To      : {}", output.display().to_string().green());
        }

        Commands::Render {
            voiceover,
            music,
            output,
            ducking,
            target_lufs,
        } => {
            println!(
                "{} OpenMontage audio mixing & ducking...",
                "Rendering".cyan().bold()
            );
            println!("  Voiceover  : {}", voiceover.display().to_string().yellow());
            if let Some(m) = &music {
                println!("  Music Track: {}", m.display().to_string().yellow());
            }
            println!("  Ducking    : {:.1} dB", ducking);
            println!("  Target LUFS: {:.1} LUFS", target_lufs);

            let (vo_samples, sr) = AudioMixer::load_wav(&voiceover)?;
            let music_samples = if let Some(m_path) = music {
                let (samples, _) = AudioMixer::load_wav(&m_path)?;
                samples
            } else {
                Vec::new()
            };

            let config = AudioMixConfig {
                sample_rate: sr,
                ducking_db: ducking,
                target_lufs,
                ..Default::default()
            };

            let mixed = AudioMixer::mix_tracks(&vo_samples, &music_samples, &config);
            AudioMixer::save_wav(&output, &mixed, sr)?;

            println!("\n{}", "Audio Mix Complete!".green().bold());
            println!("  Duration: {:.2}s", mixed.len() as f32 / sr as f32);
            println!("  Saved To: {}", output.display().to_string().green());
        }

        Commands::Pipelines => {
            println!("{}", "Available OpenMontage Production Pipelines:".cyan().bold());
            let registry = PipelineRegistry::standard_pipelines();

            for (id, pipe) in registry {
                println!("\n{} ({})", pipe.name.yellow().bold(), id.cyan());
                println!("  Description : {}", pipe.description);
                println!("  Stages      : {}", pipe.stages.len());
                for (i, stage) in pipe.stages.iter().enumerate() {
                    println!("    {}. {} (${:.2})", i + 1, stage.name, stage.estimated_cost_usd);
                }
            }
        }

        Commands::Validate { shots } => {
            println!(
                "{} shotlist pacing & composition...",
                "Validating".cyan().bold()
            );
            let content = std::fs::read_to_string(&shots)?;
            let shot_list: Vec<Shot> = serde_json::from_str(&content)?;

            let report = PacingValidator::analyze(&shot_list);

            println!("\n{}", "Pacing & Variety Report:".bold());
            println!("  Total Shots     : {}", report.total_shots);
            println!("  Total Duration  : {:.1}s", report.total_duration_seconds);
            println!("  Average Shot    : {:.2}s", report.average_shot_duration);
            println!("  Short (<1.5s)   : {}", report.short_shots_count);
            println!("  Long (>8.0s)    : {}", report.long_shots_count);
            println!(
                "  Slideshow Risk  : {}",
                if report.slideshow_risk { "HIGH (too many consecutive static shots)".red().bold() } else { "LOW (good camera movement)".green() }
            );
            println!("  Variety Score   : {:.0}%", report.variety_score * 100.0);
        }

        Commands::Cost { duration, shots, budget } => {
            println!(
                "{} production costs for {:.0}s video ({} shots)...",
                "Estimating".cyan().bold(),
                duration,
                shots
            );

            let mut tracker = CostTracker::new(budget);
            // Scriptwriting & shotlist planning
            tracker.record("OpenAI", "Script & Shotlist LLM", 3500.0, "tokens", 0.00003);
            // Video diffusion generation
            tracker.record("VideoDiffusion", "Shots Rendering", shots as f64 * 4.0, "seconds", 0.12);
            // Voiceover TTS
            tracker.record("TTS", "Voiceover Synthesis", duration * 15.0, "characters", 0.000015);

            let summary = tracker.summary();

            println!("\n{}", "Cost Breakdown:".bold());
            for r in &summary.records {
                println!("  - {:<22} {:<24} : ${:.3}", r.provider, r.operation, r.cost_usd);
            }
            println!("--------------------------------------------------");
            println!("  Total Estimated : ${:.3} USD", summary.total_cost_usd);
            println!("  Budget Cap      : ${:.2} USD", summary.budget_usd);
            println!("  Remaining       : ${:.3} USD", summary.remaining_usd);
        }

        Commands::Bench { clips, sample_rate } => {
            println!(
                "{} OpenMontage benchmark: {} clips, {} Hz audio mixing...",
                "Running".cyan().bold(),
                clips,
                sample_rate
            );

            let start = Instant::now();
            let mut timeline = Timeline::new("Bench Timeline", 30.0);

            for i in 0..clips {
                timeline.add_clip(TimelineClip {
                    id: format!("clip_{}", i),
                    track_type: if i % 2 == 0 { TrackType::Video } else { TrackType::BRoll },
                    source_path: format!("clip_{}.mp4", i),
                    in_point_ms: 0,
                    out_point_ms: 3000,
                    timeline_start_ms: (i as u64) * 2000,
                    volume: 1.0,
                    opacity: 1.0,
                });
            }

            let valid = timeline.validate_primary_video_overlaps();
            let vo_sample = vec![0.1f32; sample_rate as usize * 10]; // 10s audio
            let music_sample = vec![0.05f32; sample_rate as usize * 10];
            let config = AudioMixConfig::default();
            let _mixed = AudioMixer::mix_tracks(&vo_sample, &music_sample, &config);

            let elapsed = start.elapsed();

            println!("\n{}", "Benchmark Results:".green().bold());
            println!("  Timeline Clips  : {}", timeline.clips.len());
            println!("  Total Duration  : {:.2}s", timeline.total_duration_ms() as f64 / 1000.0);
            println!("  Overlap Valid   : {}", valid);
            println!("  Elapsed Time    : {:.4}s", elapsed.as_secs_f64());
        }
    }

    Ok(())
}
