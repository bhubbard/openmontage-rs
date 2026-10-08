use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackType {
    Video,
    BRoll,
    Voiceover,
    Music,
    SoundEffects,
    Subtitles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineClip {
    pub id: String,
    pub track_type: TrackType,
    pub source_path: String,
    pub in_point_ms: u64,
    pub out_point_ms: u64,
    pub timeline_start_ms: u64,
    pub volume: f32,
    pub opacity: f32,
}

impl TimelineClip {
    pub fn duration_ms(&self) -> u64 {
        self.out_point_ms.saturating_sub(self.in_point_ms)
    }

    pub fn timeline_end_ms(&self) -> u64 {
        self.timeline_start_ms + self.duration_ms()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub name: String,
    pub fps: f32,
    pub clips: Vec<TimelineClip>,
}

impl Timeline {
    pub fn new(name: &str, fps: f32) -> Self {
        Self {
            name: name.to_string(),
            fps,
            clips: Vec::new(),
        }
    }

    pub fn add_clip(&mut self, clip: TimelineClip) {
        self.clips.push(clip);
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.clips.iter().map(|c| c.timeline_end_ms()).max().unwrap_or(0)
    }

    pub fn clips_on_track(&self, track_type: TrackType) -> Vec<&TimelineClip> {
        self.clips.iter().filter(|c| c.track_type == track_type).collect()
    }

    /// Check if clips on the primary video track overlap
    pub fn validate_primary_video_overlaps(&self) -> bool {
        let mut video_clips: Vec<_> = self.clips_on_track(TrackType::Video);
        video_clips.sort_by_key(|c| c.timeline_start_ms);

        for w in video_clips.windows(2) {
            if w[0].timeline_end_ms() > w[1].timeline_start_ms {
                return false; // overlap found
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeline_duration() {
        let mut timeline = Timeline::new("Test Reel", 30.0);
        timeline.add_clip(TimelineClip {
            id: "clip1".to_string(),
            track_type: TrackType::Video,
            source_path: "scene1.mp4".to_string(),
            in_point_ms: 0,
            out_point_ms: 3000,
            timeline_start_ms: 0,
            volume: 1.0,
            opacity: 1.0,
        });
        timeline.add_clip(TimelineClip {
            id: "clip2".to_string(),
            track_type: TrackType::Video,
            source_path: "scene2.mp4".to_string(),
            in_point_ms: 0,
            out_point_ms: 4000,
            timeline_start_ms: 3000,
            volume: 1.0,
            opacity: 1.0,
        });

        assert_eq!(timeline.total_duration_ms(), 7000);
        assert!(timeline.validate_primary_video_overlaps());
    }
}
