//! Per-window camera session used by the native adapter.
//!
//! This preview intentionally never applies replay data to a live world.
use crate::replay::{CameraPose, Delta, Replay, SceneState, TickEvent};
use crate::timeline::{CameraKey, Editor, Interpolation, Key, Timeline, Vec3};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

pub const MAX_CAMERA_SAMPLES: usize = 72_000;
const MAX_PROJECT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    version: u32,
    duration: u64,
    timeline: Timeline,
}

#[derive(Debug)]
pub struct Session {
    pub editor: Editor,
    pub replay: Option<Replay>,
    pub recording: bool,
    pub playing: bool,
    pub preview: bool,
    pub playhead: f64,
    pub duration: u64,
    pub speed: f64,
    pub status: String,
    pub saved: bool,
    pending_sample: bool,
    samples: usize,
    capture_tick: u64,
    dimension: String,
    last_camera: Option<CameraPose>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            editor: Editor::new(Timeline::default()),
            replay: None,
            recording: false,
            playing: false,
            preview: false,
            playhead: 0.0,
            duration: 0,
            speed: 1.0,
            status: "Camera preview prototype".into(),
            saved: true,
            pending_sample: false,
            samples: 0,
            capture_tick: 0,
            dimension: String::new(),
            last_camera: None,
        }
    }
}

impl Session {
    /// Starting a new take must be explicit. Returns false if one already exists.
    pub fn start(&mut self, dimension: &str) -> bool {
        if self.recording || self.replay.is_some() || !self.editor.timeline.camera.is_empty() {
            self.status = "Save this take before starting a new session".into();
            return false;
        }
        self.dimension = dimension.into();
        self.recording = true;
        self.saved = false;
        self.preview = false;
        self.playing = false;
        self.capture_tick = 0;
        self.pending_sample = true;
        self.status = "Recording camera; world replay not implemented".into();
        true
    }

    pub fn stop(&mut self) {
        self.recording = false;
        self.pending_sample = false;
        if let Some(replay) = &mut self.replay {
            replay.rebuild_snapshots();
        }
        self.status = format!("{} camera samples captured", self.samples);
    }

    /// Start a fresh take only when current work has been saved.
    pub fn new_take(&mut self) -> bool {
        if self.recording || !self.saved {
            self.status = "Stop and save the current take before creating another".into();
            return false;
        }
        *self = Self::default();
        true
    }

    pub fn tick(&mut self, elapsed_seconds: f64, dimension: Option<&str>) {
        if self.recording {
            if dimension != Some(self.dimension.as_str()) {
                self.stop();
                self.status = "Recording stopped: world or dimension changed".into();
            } else {
                self.capture_tick = self.capture_tick.saturating_add(1);
                self.pending_sample = true;
            }
        }
        if self.playing && elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            let track_speed = self.editor.timeline.speed_at(self.playhead).unwrap_or(1.0);
            self.playhead = (self.playhead + elapsed_seconds * 20.0 * self.speed * track_speed)
                .clamp(0.0, self.duration as f64);
            if self.playhead >= self.duration as f64 {
                self.playing = false;
            }
        }
    }

    pub fn observe(&mut self, pose: CameraPose) {
        self.last_camera = Some(pose);
        if !self.recording || !self.pending_sample {
            return;
        }
        self.pending_sample = false;
        if self.samples >= MAX_CAMERA_SAMPLES {
            self.stop();
            self.status = "Camera sample limit reached; save the take".into();
            return;
        }
        if ![
            pose.position.x,
            pose.position.y,
            pose.position.z,
            pose.yaw,
            pose.pitch,
            pose.roll,
            pose.fov,
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            self.stop();
            self.status = "Recording stopped: invalid camera sample".into();
            return;
        }
        if self.replay.is_none() {
            let initial = SceneState {
                camera: pose,
                dimension: self.dimension.clone(),
                ..Default::default()
            };
            self.replay = Some(Replay::new(self.capture_tick, 200, initial));
        } else if let Some(replay) = &mut self.replay
            && let Err(error) = replay.push(TickEvent {
                tick: self.capture_tick,
                delta: Delta::Camera(pose),
            })
        {
            self.stop();
            self.status = format!("Recording error: {error:?}");
            return;
        }
        self.duration = self.capture_tick;
        self.samples += 1;
    }

    pub fn set_fov(&mut self, fov: f64) {
        if fov.is_finite()
            && (1.0..=179.0).contains(&fov)
            && let Some(camera) = &mut self.last_camera
        {
            camera.fov = fov;
        }
    }

    pub fn toggle_play(&mut self) {
        if self.recording {
            self.stop();
        }
        if self.duration == 0 {
            self.status = "Record or load a camera take first".into();
            return;
        }
        self.preview = true;
        if self.playhead >= self.duration as f64 {
            self.playhead = 0.0;
        }
        self.playing = !self.playing;
    }

    pub fn seek(&mut self, tick: f64) {
        if tick.is_finite() {
            self.playhead = tick.clamp(0.0, self.duration as f64);
            self.playing = false;
            self.preview = true;
        }
    }

    /// The cinematic track takes priority over the captured camera path.
    pub fn camera(&self) -> Option<CameraPose> {
        if !self.preview {
            return None;
        }
        if let Some(key) = self.editor.timeline.camera_at(self.playhead) {
            return Some(CameraPose {
                position: crate::replay::Vec3 {
                    x: key.position.x,
                    y: key.position.y,
                    z: key.position.z,
                },
                yaw: key.yaw,
                pitch: key.pitch,
                roll: key.roll,
                fov: self.editor.timeline.fov_at(self.playhead).unwrap_or(70.0),
            });
        }
        let replay = self.replay.as_ref()?;
        let tick = self.playhead.floor() as u64;
        let a = replay.state_at(tick).ok()?.camera;
        let b = replay.state_at(tick.saturating_add(1)).ok()?.camera;
        let t = self.playhead.fract();
        let linear = |a: f64, b: f64| a + (b - a) * t;
        let angle = |a: f64, b: f64| a + ((b - a + 180.0).rem_euclid(360.0) - 180.0) * t;
        Some(CameraPose {
            position: crate::replay::Vec3 {
                x: linear(a.position.x, b.position.x),
                y: linear(a.position.y, b.position.y),
                z: linear(a.position.z, b.position.z),
            },
            yaw: angle(a.yaw, b.yaw),
            pitch: linear(a.pitch, b.pitch),
            roll: angle(a.roll, b.roll),
            fov: linear(a.fov, b.fov),
        })
    }

    pub fn add_camera_key(&mut self) {
        let Some(pose) = self.last_camera else {
            return;
        };
        let tick = self.playhead.round() as u64;
        let key = Key {
            tick,
            interpolation: Interpolation::Smooth,
            value: CameraKey {
                position: Vec3 {
                    x: pose.position.x,
                    y: pose.position.y,
                    z: pose.position.z,
                },
                yaw: pose.yaw,
                pitch: pose.pitch,
                roll: pose.roll,
            },
        };
        let result = self.editor.edit(|timeline| {
            timeline.camera.retain(|old| old.tick != tick);
            timeline.insert_camera(key)
        });
        self.status = match result {
            Ok(()) => format!("Camera keyframe at {tick}"),
            Err(e) => format!("Keyframe error: {e:?}"),
        };
        self.saved = false;
    }

    pub fn save_project(&self, mut writer: impl Write) -> Result<(), String> {
        self.editor
            .timeline
            .validate()
            .map_err(|e| format!("{e:?}"))?;
        let data = Project {
            version: 1,
            duration: self.duration,
            timeline: self.editor.timeline.clone(),
        };
        serde_json::to_writer_pretty(&mut writer, &data).map_err(|e| e.to_string())
    }

    pub fn load_project(&mut self, reader: impl Read) -> Result<(), String> {
        if self.recording {
            return Err("Stop recording before loading a project".into());
        }
        let mut bytes = Vec::new();
        reader
            .take(MAX_PROJECT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_PROJECT_BYTES {
            return Err("Project exceeds 16 MiB".into());
        }
        let project: Project = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if project.version != 1 {
            return Err("Unsupported project version".into());
        }
        project.timeline.validate().map_err(|e| format!("{e:?}"))?;
        if project
            .timeline
            .camera
            .iter()
            .any(|key| key.tick > project.duration)
        {
            return Err("Camera keyframe exceeds project duration".into());
        }
        self.editor = Editor::new(project.timeline);
        self.duration = project.duration;
        self.replay = None;
        self.playhead = 0.0;
        self.playing = false;
        self.preview = false;
        self.saved = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(x: f64) -> CameraPose {
        CameraPose {
            position: crate::replay::Vec3 { x, y: 64.0, z: 0.0 },
            fov: 70.0,
            ..Default::default()
        }
    }
    #[test]
    fn camera_capture_preview_and_world_boundary() {
        let mut session = Session::default();
        assert!(session.start("overworld"));
        session.observe(pose(0.0));
        session.tick(0.05, Some("overworld"));
        session.observe(pose(2.0));
        session.stop();
        assert!(!session.start("overworld"));
        session.seek(0.5);
        assert_eq!(session.camera().unwrap().position.x, 1.0);
        session.preview = false;
        assert!(session.camera().is_none());
        let mut second = Session::default();
        assert!(second.start("overworld"));
        second.tick(0.05, Some("nether"));
        assert!(!second.recording);
    }
    #[test]
    fn invalid_project_does_not_replace_current_session() {
        let mut session = Session {
            duration: 30,
            ..Default::default()
        };
        assert!(session.load_project(&b"{\"version\":2}"[..]).is_err());
        assert_eq!(session.duration, 30);
        assert!(
            session
                .load_project(std::io::repeat(b' ').take(MAX_PROJECT_BYTES + 1))
                .is_err()
        );
        assert_eq!(session.duration, 30);
    }
}
