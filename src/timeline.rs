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
    Step,
    Linear,
    Smooth,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Key<T> {
    pub tick: u64,
    pub value: T,
    pub interpolation: Interpolation,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Timeline {
    pub camera: Vec<Key<CameraKey>>,
    pub fov: Vec<Key<f64>>,
    pub speed: Vec<Key<f64>>,
    pub time_of_day: Vec<Key<f64>>,
    pub markers: Vec<String>,
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
fn valid<T>(keys: &[Key<T>]) -> Result<(), TimelineError> {
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
        sample_camera(&self.camera, t)
    }
    pub fn fov_at(&self, t: f64) -> Option<f64> {
        sample_scalar(&self.fov, t, false)
    }
    pub fn speed_at(&self, t: f64) -> Option<f64> {
        sample_scalar(&self.speed, t, false)
    }
    pub fn time_of_day_at(&self, t: f64) -> Option<f64> {
        sample_scalar(&self.time_of_day, t, true)
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
fn bounds<T>(v: &[Key<T>], t: f64) -> Option<(usize, usize, f64)> {
    if v.is_empty() || !t.is_finite() {
        return None;
    }
    if t <= v[0].tick as f64 {
        return Some((0, 0, 0.0));
    }
    let n = v.len() - 1;
    if t >= v[n].tick as f64 {
        return Some((n, n, 0.0));
    }
    let hi = v.partition_point(|k| (k.tick as f64) <= t);
    let lo = hi - 1;
    Some((
        lo,
        hi,
        (t - v[lo].tick as f64) / (v[hi].tick - v[lo].tick) as f64,
    ))
}
fn amount(i: Interpolation, t: f64) -> f64 {
    match i {
        Interpolation::Step => 0.0,
        Interpolation::Linear => t,
        Interpolation::Smooth => t * t * (3.0 - 2.0 * t),
    }
}
fn sample_scalar(v: &[Key<f64>], t: f64, angle: bool) -> Option<f64> {
    let (l, h, a) = bounds(v, t)?;
    if l == h {
        return Some(v[l].value);
    }
    let f = amount(v[l].interpolation, a);
    let b = if angle {
        let d = (v[h].value - v[l].value + 12000.0).rem_euclid(24000.0) - 12000.0;
        v[l].value + d * f
    } else {
        v[l].value + (v[h].value - v[l].value) * f
    };
    Some(if angle { b.rem_euclid(24000.0) } else { b })
}
fn sample_camera(v: &[Key<CameraKey>], t: f64) -> Option<CameraKey> {
    let (l, h, a) = bounds(v, t)?;
    if l == h {
        return Some(v[l].value);
    }
    let f = amount(v[l].interpolation, a);
    let p0 = v[l].value;
    let p1 = v[h].value;
    let prev = if l > 0 {
        v[l - 1].value.position
    } else {
        p0.position
    };
    let next = if h + 1 < v.len() {
        v[h + 1].value.position
    } else {
        p1.position
    };
    let pos = if v[l].interpolation == Interpolation::Smooth {
        Vec3 {
            x: cat(prev.x, p0.position.x, p1.position.x, next.x, f),
            y: cat(prev.y, p0.position.y, p1.position.y, next.y, f),
            z: cat(prev.z, p0.position.z, p1.position.z, next.z, f),
        }
    } else {
        lerp3(p0.position, p1.position, f)
    };
    Some(CameraKey {
        position: pos,
        yaw: lerp_angle(p0.yaw, p1.yaw, f),
        pitch: lerp_angle(p0.pitch, p1.pitch, f),
        roll: lerp_angle(p0.roll, p1.roll, f),
    })
}
fn cat(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    0.5 * ((2.0 * b)
        + (-a + c) * t
        + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t
        + (-a + 3.0 * b - 3.0 * c + d) * t * t * t)
}
fn lerp3(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    Vec3 {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        z: a.z + (b.z - a.z) * t,
    }
}
fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    a + ((b - a + 180.0).rem_euclid(360.0) - 180.0) * t
}
#[derive(Debug, Clone)]
pub struct Editor {
    pub timeline: Timeline,
    undo: Vec<Timeline>,
    redo: Vec<Timeline>,
}
impl Editor {
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
