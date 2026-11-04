//! Cinematic timeline tracks, editing history, and deterministic frame schedules.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct CameraKey {
    pub position: Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpolation {
    #[serde(alias = "HOLD", alias = "hold")]
    Step,
    #[serde(alias = "LINEAR", alias = "linear")]
    Linear,
    #[serde(alias = "SMOOTH", alias = "smooth")]
    Smooth,
    #[serde(alias = "EASE_IN", alias = "ease_in")]
    EaseIn,
    #[serde(alias = "EASE_OUT", alias = "ease_out")]
    EaseOut,
    #[serde(alias = "EASE_IN_OUT", alias = "ease_in_out")]
    EaseInOut,
    #[serde(alias = "HERMITE", alias = "hermite")]
    Hermite,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Key<T> {
    pub tick: u64,
    pub value: T,
    pub interpolation: Interpolation,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Timeline {
    pub camera: Vec<Key<CameraKey>>,
    pub fov: Vec<Key<f64>>,
    pub speed: Vec<Key<f64>>,
    pub time_of_day: Vec<Key<f64>>,
    pub markers: Vec<String>,
    #[serde(default)]
    pub tracks: Vec<crate::cinematic::Track>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineError {
    NonFinite,
    DuplicateTick,
    InvalidValue,
    InvalidFps,
    InvalidOperation,
    Serialization(String),
}
pub(crate) fn valid<T>(keys: &[Key<T>]) -> Result<(), TimelineError> {
    let mut last = None;
    for k in keys {
        if last.is_some_and(|v| v >= k.tick) {
            return Err(if last == Some(k.tick) {
                TimelineError::DuplicateTick
            } else {
                TimelineError::InvalidOperation
            });
        }
        last = Some(k.tick);
    }
    Ok(())
}
impl Timeline {
    pub fn validate(&self) -> Result<(), TimelineError> {
        let mut ids = std::collections::BTreeSet::new();
        for track in &self.tracks {
            if !ids.insert(track.id) {
                return Err(TimelineError::InvalidOperation);
            }
            track.data.validate()?;
        }
        valid(&self.camera)?;
        valid(&self.fov)?;
        valid(&self.speed)?;
        valid(&self.time_of_day)?;
        for k in &self.camera {
            let v = k.value;
            if ![
                v.position.x,
                v.position.y,
                v.position.z,
                v.yaw,
                v.pitch,
                v.roll,
            ]
            .iter()
            .all(|x| x.is_finite())
            {
                return Err(TimelineError::NonFinite);
            }
        }
        for k in &self.fov {
            if !k.value.is_finite() {
                return Err(TimelineError::NonFinite);
            }
            if !(1.0..=179.0).contains(&k.value) {
                return Err(TimelineError::InvalidValue);
            }
        }
        for k in &self.speed {
            if !k.value.is_finite() {
                return Err(TimelineError::NonFinite);
            }
            if k.value < 0.0 {
                return Err(TimelineError::InvalidValue);
            }
        }
        for k in &self.time_of_day {
            if !k.value.is_finite() {
                return Err(TimelineError::NonFinite);
            }
        }
        Ok(())
    }
    pub fn insert_camera(&mut self, key: Key<CameraKey>) -> Result<(), TimelineError> {
        let mut candidate = self.clone();
        insert(&mut candidate.camera, key)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn insert_fov(&mut self, key: Key<f64>) -> Result<(), TimelineError> {
        let mut candidate = self.clone();
        insert(&mut candidate.fov, key)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn insert_speed(&mut self, key: Key<f64>) -> Result<(), TimelineError> {
        let mut candidate = self.clone();
        insert(&mut candidate.speed, key)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn insert_time(&mut self, key: Key<f64>) -> Result<(), TimelineError> {
        let mut candidate = self.clone();
        insert(&mut candidate.time_of_day, key)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn camera_at(&self, t: f64) -> Option<CameraKey> {
        self.camera_at_with_targets(t, |_| None)
    }
    pub fn camera_at_with_targets(
        &self,
        t: f64,
        mut resolve: impl FnMut(crate::cinematic::EntityId) -> Option<crate::cinematic::TargetPose>,
    ) -> Option<CameraKey> {
        use crate::cinematic::{TrackKind, TrackSample};
        let mut camera = sample_camera(&self.camera, t);
        let mut seen = 0u16;
        for track in &self.tracks {
            let kind = track.data.kind();
            if !matches!(
                kind,
                TrackKind::Camera | TrackKind::Orbit | TrackKind::Tracking
            ) {
                continue;
            }
            let bit = 1u16 << (kind as u8);
            if seen & bit != 0 {
                continue;
            }
            let Some(change) = track.sample(t, false) else {
                continue;
            };
            seen |= bit;
            match change {
                TrackSample::Camera(k) => camera = Some(k),
                TrackSample::Orbit(k) => camera = Some(k.camera()),
                TrackSample::Tracking(k) => {
                    if let Some(pose) = resolve(k.target) {
                        camera = Some(k.camera(pose));
                    }
                }
                _ => {}
            }
        }
        camera
    }
    pub fn changes_at(&self, t: f64) -> impl Iterator<Item = crate::cinematic::TrackSample> + '_ {
        use crate::cinematic::TrackKind;
        let mut seen = 0u16;
        self.tracks.iter().filter_map(move |track| {
            let kind = match track.data.kind() {
                TrackKind::Timelapse => TrackKind::Speed,
                k => k,
            };
            if kind == TrackKind::BlockOverride {
                return None;
            }
            let bit = 1u16 << (kind as u8);
            if seen & bit != 0 && kind != TrackKind::Audio {
                return None;
            }
            let change = track.sample(t, false)?;
            seen |= bit;
            Some(change)
        })
    }
    fn track_scalar(&self, t: f64, kind: crate::cinematic::TrackKind) -> Option<f64> {
        use crate::cinematic::{TrackKind, TrackSample};
        self.tracks
            .iter()
            .filter(|track| {
                track.data.kind() == kind
                    || (kind == TrackKind::Speed && track.data.kind() == TrackKind::Timelapse)
            })
            .find_map(|track| match track.sample(t, false)? {
                TrackSample::Fov(v) | TrackSample::Speed(v) | TrackSample::TimeOfDay(v) => Some(v),
                _ => None,
            })
    }
    pub fn max_tick(&self) -> Option<u64> {
        self.camera
            .last()
            .map(|k| k.tick)
            .into_iter()
            .chain(self.fov.last().map(|k| k.tick))
            .chain(self.speed.last().map(|k| k.tick))
            .chain(self.time_of_day.last().map(|k| k.tick))
            .chain(self.tracks.iter().filter_map(|t| t.data.last_tick()))
            .max()
    }
    pub fn fov_at(&self, t: f64) -> Option<f64> {
        self.track_scalar(t, crate::cinematic::TrackKind::Fov)
            .or_else(|| sample_scalar(&self.fov, t, true))
    }
    pub fn speed_at(&self, t: f64) -> Option<f64> {
        self.track_scalar(t, crate::cinematic::TrackKind::Speed)
            .or_else(|| sample_scalar(&self.speed, t, false))
    }
    pub fn time_of_day_at(&self, t: f64) -> Option<f64> {
        self.track_scalar(t, crate::cinematic::TrackKind::TimeOfDay)
            .or_else(|| sample_scalar(&self.time_of_day, t, false).map(f64::trunc))
    }
    pub fn to_json(&self) -> Result<String, TimelineError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|e| TimelineError::Serialization(e.to_string()))
    }
    pub fn from_json(s: &str) -> Result<Self, TimelineError> {
        let v: Self =
            serde_json::from_str(s).map_err(|e| TimelineError::Serialization(e.to_string()))?;
        v.validate()?;
        Ok(v)
    }
}
fn insert<T>(v: &mut Vec<Key<T>>, k: Key<T>) -> Result<(), TimelineError> {
    match v.binary_search_by_key(&k.tick, |x| x.tick) {
        Ok(_) => Err(TimelineError::DuplicateTick),
        Err(i) => {
            v.insert(i, k);
            Ok(())
        }
    }
}
fn sample_scalar(v: &[Key<f64>], t: f64, focal_length: bool) -> Option<f64> {
    use crate::interpolation::{Boundary, sample};
    let result = sample(
        v,
        t,
        |value| {
            [if focal_length {
                1.0 / (value.to_radians() * 0.5).tan()
            } else {
                *value
            }]
        },
        [0.0],
        &[],
        Boundary::Hold,
    )?[0];
    Some(if focal_length {
        (1.0 / result).atan().to_degrees() * 2.0
    } else {
        result
    })
}
fn sample_camera(v: &[Key<CameraKey>], t: f64) -> Option<CameraKey> {
    use crate::interpolation::{Boundary, sample};
    let values = sample(
        v,
        t,
        |k| {
            [
                k.position.x,
                k.position.y,
                k.position.z,
                k.yaw,
                k.pitch,
                k.roll,
            ]
        },
        [0.0, 0.0, 0.0, 360.0, 360.0, 360.0],
        &[[0, 1, 2]],
        Boundary::Hold,
    )?;
    Some(CameraKey {
        position: Vec3 {
            x: values[0],
            y: values[1],
            z: values[2],
        },
        yaw: values[3],
        pitch: values[4],
        roll: values[5],
    })
}
#[derive(Debug, Clone)]
pub struct Editor {
    pub timeline: Timeline,
    undo: Vec<Timeline>,
    redo: Vec<Timeline>,
}
impl Editor {
    pub fn add_track(&mut self, track: crate::cinematic::Track) -> Result<(), TimelineError> {
        self.edit(move |timeline| {
            timeline.tracks.push(track);
            Ok(())
        })
    }
    pub fn delete_track(&mut self, id: u64) -> Result<(), TimelineError> {
        self.edit(|timeline| {
            let i = timeline
                .tracks
                .iter()
                .position(|t| t.id == id)
                .ok_or(TimelineError::InvalidOperation)?;
            timeline.tracks.remove(i);
            Ok(())
        })
    }
    pub fn move_track(&mut self, id: u64, to: usize) -> Result<(), TimelineError> {
        self.edit(|timeline| {
            if to >= timeline.tracks.len() {
                return Err(TimelineError::InvalidOperation);
            }
            let i = timeline
                .tracks
                .iter()
                .position(|t| t.id == id)
                .ok_or(TimelineError::InvalidOperation)?;
            let track = timeline.tracks.remove(i);
            timeline.tracks.insert(to, track);
            Ok(())
        })
    }
    pub fn update_track(
        &mut self,
        id: u64,
        name: String,
        color: u32,
        enabled: bool,
    ) -> Result<(), TimelineError> {
        self.edit(move |timeline| {
            let track = timeline
                .tracks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or(TimelineError::InvalidOperation)?;
            track.name = name;
            track.color = color;
            track.enabled = enabled;
            Ok(())
        })
    }
    pub fn edit_track_key(
        &mut self,
        id: u64,
        operation: crate::cinematic::KeyEdit,
    ) -> Result<(), TimelineError> {
        self.edit(|timeline| {
            timeline
                .tracks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or(TimelineError::InvalidOperation)?
                .data
                .edit_key(operation)
        })
    }
    pub fn set_track_key(
        &mut self,
        id: u64,
        tick: u64,
        value: crate::cinematic::KeyValue,
        interpolation: Interpolation,
    ) -> Result<(), TimelineError> {
        self.edit(move |timeline| {
            timeline
                .tracks
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or(TimelineError::InvalidOperation)?
                .data
                .set_key(tick, value, interpolation)
        })
    }
    pub fn new(t: Timeline) -> Self {
        Self {
            timeline: t,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
    pub fn edit<F>(&mut self, f: F) -> Result<(), TimelineError>
    where
        F: FnOnce(&mut Timeline) -> Result<(), TimelineError>,
    {
        let mut candidate = self.timeline.clone();
        f(&mut candidate)?;
        candidate.validate()?;
        if candidate == self.timeline {
            return Ok(());
        }
        self.undo.push(self.timeline.clone());
        if self.undo.len() > 256 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.timeline = candidate;
        Ok(())
    }
    pub fn undo(&mut self) -> bool {
        if let Some(v) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.timeline, v));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(v) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.timeline, v));
            true
        } else {
            false
        }
    }
    pub fn move_camera_key(&mut self, from: u64, to: u64) -> Result<(), TimelineError> {
        self.edit(|t| {
            let i = t
                .camera
                .binary_search_by_key(&from, |k| k.tick)
                .map_err(|_| TimelineError::InvalidOperation)?;
            if t.camera.iter().any(|k| k.tick == to) {
                return Err(TimelineError::DuplicateTick);
            }
            let mut k = t.camera.remove(i);
            k.tick = to;
            insert(&mut t.camera, k)
        })
    }
    pub fn delete_camera_key(&mut self, tick: u64) -> Result<(), TimelineError> {
        self.edit(|t| {
            let i = t
                .camera
                .binary_search_by_key(&tick, |k| k.tick)
                .map_err(|_| TimelineError::InvalidOperation)?;
            t.camera.remove(i);
            Ok(())
        })
    }
    pub fn copy_camera_key(&mut self, from: u64, to: u64) -> Result<(), TimelineError> {
        self.edit(|t| {
            let i = t
                .camera
                .binary_search_by_key(&from, |k| k.tick)
                .map_err(|_| TimelineError::InvalidOperation)?;
            let mut k = t.camera[i].clone();
            k.tick = to;
            insert(&mut t.camera, k)
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSchedule {
    pub numerator: u32,
    pub denominator: u32,
}
impl FrameSchedule {
    pub fn new(n: u32, d: u32) -> Result<Self, TimelineError> {
        if n == 0 || d == 0 {
            return Err(TimelineError::InvalidFps);
        }
        let g = gcd(n, d);
        Ok(Self {
            numerator: n / g,
            denominator: d / g,
        })
    }
    pub fn tick_time_nanos(
        &self,
        frame: u64,
        ticks_per_second: u32,
    ) -> Result<u128, TimelineError> {
        if ticks_per_second == 0 {
            return Err(TimelineError::InvalidFps);
        }
        if self.numerator == 0 || self.denominator == 0 {
            return Err(TimelineError::InvalidFps);
        }
        // Timeline position in billionths of a game tick, at the given output frame.
        let numerator = (frame as u128)
            .checked_mul(1_000_000_000)
            .and_then(|v| v.checked_mul(self.denominator as u128))
            .and_then(|v| v.checked_mul(ticks_per_second as u128))
            .ok_or(TimelineError::InvalidFps)?;
        Ok(numerator / self.numerator as u128)
    }
    pub fn frames_until(&self, duration_nanos: u128) -> u128 {
        if self.denominator == 0 {
            return 0;
        }
        let divisor = 1_000_000_000u128 * self.denominator as u128;
        (duration_nanos / divisor)
            .saturating_mul(self.numerator as u128)
            .saturating_add((duration_nanos % divisor) * self.numerator as u128 / divisor)
    }
}
fn gcd(a: u32, b: u32) -> u32 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        let r = a % b;
        a = b;
        b = r
    }
    a
}
#[cfg(test)]
mod tests {
    use super::*;
    fn k(t: u64, x: f64, i: Interpolation) -> Key<CameraKey> {
        Key {
            tick: t,
            value: CameraKey {
                position: Vec3 { x, y: 0.0, z: 0.0 },
                yaw: x,
                pitch: 0.0,
                roll: 0.0,
            },
            interpolation: i,
        }
    }
    #[test]
    fn interpolation_wrap_endpoints_and_smooth_position() {
        let mut t = Timeline::default();
        t.insert_camera(k(0, 350.0, Interpolation::Smooth)).unwrap();
        t.insert_camera(k(10, 10.0, Interpolation::Linear)).unwrap();
        assert_eq!(t.camera_at(-4.0).unwrap().position.x, 350.0);
        let mid = t.camera_at(5.0).unwrap();
        assert!(mid.yaw < 1.0 || mid.yaw > 359.0);
        assert_eq!(t.camera_at(10.0).unwrap().position.x, 10.0);
    }
    #[test]
    fn editor_undo_redo_copy_move_delete() {
        let mut e = Editor::new(Timeline::default());
        e.edit(|t| t.insert_camera(k(1, 2.0, Interpolation::Linear)))
            .unwrap();
        assert!(e.undo());
        assert!(e.timeline.camera.is_empty());
        assert!(e.redo());
        e.copy_camera_key(1, 2).unwrap();
        e.move_camera_key(2, 3).unwrap();
        e.delete_camera_key(3).unwrap();
        assert_eq!(e.timeline.camera.len(), 1);
    }
    #[test]
    fn rational_schedule_and_validation() {
        let s = FrameSchedule::new(60000, 1001).unwrap();
        assert_eq!((s.numerator, s.denominator), (60000, 1001));
        assert_eq!(s.frames_until(1_000_000_000), 59);
        assert!(FrameSchedule::new(0, 1).is_err());
        let mut t = Timeline::default();
        t.fov.push(Key {
            tick: 0,
            value: f64::NAN,
            interpolation: Interpolation::Linear,
        });
        assert_eq!(t.validate(), Err(TimelineError::NonFinite));
    }
}
