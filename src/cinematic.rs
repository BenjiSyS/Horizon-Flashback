//! Typed cinematic tracks matching Flashback's registered keyframe families.
//! Behavioral reference: Moulberry's Flashback. Original copyright retained.
use crate::interpolation::{Boundary, discrete_index, sample};
use crate::replay::BlockState;
use crate::timeline::{CameraKey, Interpolation, Key, TimelineError, Vec3, valid};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EntityId([u8; 16]);
impl EntityId {
    pub fn from_u128(value: u128) -> Self {
        Self(value.to_be_bytes())
    }
    pub fn as_u128(self) -> u128 {
        u128::from_be_bytes(self.0)
    }
}
impl TryFrom<String> for EntityId {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() != 32 && bytes.len() != 36 {
            return Err("Invalid entity UUID");
        }
        if bytes.len() == 36 && [8, 13, 18, 23].iter().any(|&i| bytes[i] != b'-') {
            return Err("Invalid entity UUID");
        }
        let compact: String = value.chars().filter(|&c| c != '-').collect();
        if compact.len() != 32 || !compact.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("Invalid entity UUID");
        }
        u128::from_str_radix(&compact, 16)
            .map(Self::from_u128)
            .map_err(|_| "Invalid entity UUID")
    }
}
impl From<EntityId> for String {
    fn from(value: EntityId) -> Self {
        let text = format!("{:032x}", value.as_u128());
        format!(
            "{}-{}-{}-{}-{}",
            &text[..8],
            &text[8..12],
            &text[12..16],
            &text[16..20],
            &text[20..]
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrbitKey {
    pub center: Vec3,
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
}
impl OrbitKey {
    /// Output is an eye-space render camera; no viewer eye-height subtraction.
    pub fn camera(self) -> CameraKey {
        let (sy, cy) = (-self.yaw).to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        CameraKey {
            position: Vec3 {
                x: self.center.x - sy * cp * self.distance,
                y: self.center.y + sp * self.distance,
                z: self.center.z - cy * cp * self.distance,
            },
            yaw: self.yaw,
            pitch: self.pitch,
            roll: 0.0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyPart {
    #[serde(alias = "ROOT")]
    Root,
    #[serde(alias = "BODY")]
    Body,
    #[serde(alias = "HEAD")]
    Head,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackingKey {
    pub target: EntityId,
    pub body_part: BodyPart,
    pub yaw_offset: f64,
    pub pitch_offset: f64,
    pub position_offset: Vec3,
    pub view_offset: Vec3,
    pub roll: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TargetPose {
    pub position: Vec3,
    pub height: f64,
    pub eye_height: f64,
    pub yaw: f64,
    pub body_yaw: f64,
    pub head_yaw: f64,
    pub pitch: f64,
}
impl TrackingKey {
    pub fn camera(self, target: TargetPose) -> CameraKey {
        let (height, yaw, pitch) = match self.body_part {
            BodyPart::Root => (0.0, 0.0, 0.0),
            BodyPart::Body => (target.height * 0.5, target.body_yaw, 0.0),
            BodyPart::Head => (target.eye_height, target.head_yaw, target.pitch),
        };
        let yaw = yaw + self.yaw_offset;
        let pitch = pitch * self.yaw_offset.to_radians().cos() + self.pitch_offset;
        let (sp, cp) = (-pitch).to_radians().sin_cos();
        let (sy, cy) = (180.0 - yaw).to_radians().sin_cos();
        let y = self.view_offset.y * cp - self.view_offset.z * sp;
        let z = self.view_offset.y * sp + self.view_offset.z * cp;
        let x = self.view_offset.x * cy + z * sy;
        let z = -self.view_offset.x * sy + z * cy;
        CameraKey {
            position: Vec3 {
                x: target.position.x + self.position_offset.x + x,
                y: target.position.y + height + self.position_offset.y + y,
                z: target.position.z + self.position_offset.z + z,
            },
            yaw,
            pitch,
            roll: self.roll,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShakeKey {
    pub frequency_x: f64,
    pub amplitude_x: f64,
    pub frequency_y: f64,
    pub amplitude_y: f64,
    pub split_params: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreezeKey {
    pub frozen: bool,
    pub delay: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelapseKey {
    pub ticks: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioKey {
    pub path: PathBuf,
    #[serde(default)]
    pub duration_ticks: Option<f64>,
    #[serde(default = "unit_gain")]
    pub gain: f64,
}
fn unit_gain() -> f64 {
    1.0
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockOverrideKey {
    pub dimension: String,
    pub blocks: Vec<BlockState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    Camera,
    Orbit,
    Tracking,
    Shake,
    Fov,
    Speed,
    Timelapse,
    TimeOfDay,
    Freeze,
    Audio,
    BlockOverride,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "keys", rename_all = "snake_case")]
pub enum TrackData {
    Camera(Vec<Key<CameraKey>>),
    Orbit(Vec<Key<OrbitKey>>),
    Tracking(Vec<Key<TrackingKey>>),
    Shake(Vec<Key<ShakeKey>>),
    Fov(Vec<Key<f64>>),
    Speed(Vec<Key<f64>>),
    Timelapse(Vec<Key<TimelapseKey>>),
    TimeOfDay(Vec<Key<f64>>),
    Freeze(Vec<Key<FreezeKey>>),
    Audio(Vec<Key<AudioKey>>),
    BlockOverride(Vec<Key<BlockOverrideKey>>),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: u64,
    pub name: String,
    pub color: u32,
    pub enabled: bool,
    pub data: TrackData,
}
impl Track {
    pub fn new(id: u64, data: TrackData) -> Self {
        Self {
            id,
            name: format!("{:?}", data.kind()),
            color: 0,
            enabled: true,
            data,
        }
    }
    pub fn sample(&self, tick: f64, hold_last: bool) -> Option<TrackSample> {
        self.enabled
            .then(|| self.data.sample(tick, hold_last))
            .flatten()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum TrackSample {
    Camera(CameraKey),
    Orbit(OrbitKey),
    Tracking(TrackingKey),
    Shake(ShakeKey),
    Fov(f64),
    Speed(f64),
    TimeOfDay(f64),
    Freeze(FreezeKey),
    Audio {
        key: AudioKey,
        start: u64,
        offset_seconds: f64,
    },
    Blocks(BlockOverrideKey),
}
#[derive(Clone, Debug, PartialEq)]
pub enum KeyValue {
    Camera(CameraKey),
    Orbit(OrbitKey),
    Tracking(TrackingKey),
    Shake(ShakeKey),
    Fov(f64),
    Speed(f64),
    Timelapse(TimelapseKey),
    TimeOfDay(f64),
    Freeze(FreezeKey),
    Audio(AudioKey),
    BlockOverride(BlockOverrideKey),
}
#[derive(Clone, Copy, Debug)]
pub enum KeyEdit {
    Move { from: u64, to: u64 },
    Copy { from: u64, to: u64 },
    Delete { tick: u64 },
}

fn edit_keys<T: Clone>(keys: &mut Vec<Key<T>>, edit: KeyEdit) -> Result<(), TimelineError> {
    let (source, destination, copy) = match edit {
        KeyEdit::Move { from, to } => (from, Some(to), false),
        KeyEdit::Copy { from, to } => (from, Some(to), true),
        KeyEdit::Delete { tick } => (tick, None, false),
    };
    let index = keys
        .binary_search_by_key(&source, |k| k.tick)
        .map_err(|_| TimelineError::InvalidOperation)?;
    if destination.is_some_and(|to| keys.binary_search_by_key(&to, |k| k.tick).is_ok()) {
        return Err(TimelineError::DuplicateTick);
    }
    let mut key = keys[index].clone();
    if !copy {
        keys.remove(index);
    }
    if let Some(to) = destination {
        key.tick = to;
        let index = keys.partition_point(|k| k.tick < to);
        keys.insert(index, key);
    }
    Ok(())
}
fn find_value<T: Clone>(keys: &[Key<T>], tick: u64) -> Option<T> {
    keys.get(keys.binary_search_by_key(&tick, |k| k.tick).ok()?)
        .map(|k| k.value.clone())
}
impl TrackSample {
    /// Timelapse and speed both produce the same tickrate change family.
    pub fn kind(&self) -> TrackKind {
        match self {
            Self::Camera(_) => TrackKind::Camera,
            Self::Orbit(_) => TrackKind::Orbit,
            Self::Tracking(_) => TrackKind::Tracking,
            Self::Shake(_) => TrackKind::Shake,
            Self::Fov(_) => TrackKind::Fov,
            Self::Speed(_) => TrackKind::Speed,
            Self::TimeOfDay(_) => TrackKind::TimeOfDay,
            Self::Freeze(_) => TrackKind::Freeze,
            Self::Audio { .. } => TrackKind::Audio,
            Self::Blocks(_) => TrackKind::BlockOverride,
        }
    }
}
fn finite(values: &[f64]) -> Result<(), TimelineError> {
    if values.iter().all(|x| x.is_finite()) {
        Ok(())
    } else {
        Err(TimelineError::NonFinite)
    }
}
fn vector(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}
fn camera_values(k: &CameraKey) -> [f64; 6] {
    [
        k.position.x,
        k.position.y,
        k.position.z,
        k.yaw,
        k.pitch,
        k.roll,
    ]
}
fn keys_valid<T>(
    keys: &[Key<T>],
    check: impl Fn(&T) -> Result<(), TimelineError>,
) -> Result<(), TimelineError> {
    valid(keys)?;
    for key in keys {
        check(&key.value)?;
    }
    Ok(())
}
fn scalar_valid(keys: &[Key<f64>], check: impl Fn(f64) -> bool) -> Result<(), TimelineError> {
    keys_valid(keys, |v| {
        finite(&[*v])?;
        if check(*v) {
            Ok(())
        } else {
            Err(TimelineError::InvalidValue)
        }
    })
}
fn curve<T, const N: usize>(
    keys: &[Key<T>],
    tick: f64,
    values: impl Fn(&T) -> [f64; N],
    periods: [f64; N],
    groups: &[[usize; 3]],
    hold_last: bool,
) -> Option<[f64; N]> {
    let tick = if hold_last {
        tick.min(keys.last()?.tick as f64)
    } else {
        tick
    };
    sample(keys, tick, values, periods, groups, Boundary::Original)
}
fn active<T>(keys: &[Key<T>], tick: f64, hold_last: bool) -> Option<usize> {
    if !tick.is_finite() || keys.is_empty() || tick < keys[0].tick as f64 {
        return None;
    }
    let i = keys
        .partition_point(|key| key.tick as f64 <= tick)
        .saturating_sub(1);
    if i + 1 == keys.len()
        && tick.floor() > keys[i].tick as f64
        && !hold_last
        && keys[i].interpolation != Interpolation::Step
    {
        return None;
    }
    Some(i)
}
impl TrackData {
    pub fn empty(kind: TrackKind) -> Self {
        match kind {
            TrackKind::Camera => Self::Camera(Vec::new()),
            TrackKind::Orbit => Self::Orbit(Vec::new()),
            TrackKind::Tracking => Self::Tracking(Vec::new()),
            TrackKind::Shake => Self::Shake(Vec::new()),
            TrackKind::Fov => Self::Fov(Vec::new()),
            TrackKind::Speed => Self::Speed(Vec::new()),
            TrackKind::Timelapse => Self::Timelapse(Vec::new()),
            TrackKind::TimeOfDay => Self::TimeOfDay(Vec::new()),
            TrackKind::Freeze => Self::Freeze(Vec::new()),
            TrackKind::Audio => Self::Audio(Vec::new()),
            TrackKind::BlockOverride => Self::BlockOverride(Vec::new()),
        }
    }
    pub fn key_times(&self) -> Vec<(u64, Interpolation)> {
        macro_rules! times {
            ($keys:expr) => {
                $keys.iter().map(|k| (k.tick, k.interpolation)).collect()
            };
        }
        match self {
            Self::Camera(k) => times!(k),
            Self::Orbit(k) => times!(k),
            Self::Tracking(k) => times!(k),
            Self::Shake(k) => times!(k),
            Self::Fov(k) | Self::Speed(k) | Self::TimeOfDay(k) => times!(k),
            Self::Timelapse(k) => times!(k),
            Self::Freeze(k) => times!(k),
            Self::Audio(k) => times!(k),
            Self::BlockOverride(k) => times!(k),
        }
    }
    pub fn key_value(&self, tick: u64) -> Option<KeyValue> {
        macro_rules! value {
            ($keys:expr,$variant:ident) => {
                KeyValue::$variant(find_value($keys, tick)?)
            };
        }
        Some(match self {
            Self::Camera(k) => value!(k, Camera),
            Self::Orbit(k) => value!(k, Orbit),
            Self::Tracking(k) => value!(k, Tracking),
            Self::Shake(k) => value!(k, Shake),
            Self::Fov(k) => value!(k, Fov),
            Self::Speed(k) => value!(k, Speed),
            Self::TimeOfDay(k) => value!(k, TimeOfDay),
            Self::Timelapse(k) => value!(k, Timelapse),
            Self::Freeze(k) => value!(k, Freeze),
            Self::Audio(k) => value!(k, Audio),
            Self::BlockOverride(k) => value!(k, BlockOverride),
        })
    }
    /// Key editing is transactional even when called outside Editor history.
    pub fn edit_key(&mut self, edit: KeyEdit) -> Result<(), TimelineError> {
        let mut candidate = self.clone();
        match &mut candidate {
            Self::Camera(k) => edit_keys(k, edit)?,
            Self::Orbit(k) => edit_keys(k, edit)?,
            Self::Tracking(k) => edit_keys(k, edit)?,
            Self::Shake(k) => edit_keys(k, edit)?,
            Self::Fov(k) | Self::Speed(k) | Self::TimeOfDay(k) => edit_keys(k, edit)?,
            Self::Timelapse(k) => edit_keys(k, edit)?,
            Self::Freeze(k) => edit_keys(k, edit)?,
            Self::Audio(k) => edit_keys(k, edit)?,
            Self::BlockOverride(k) => edit_keys(k, edit)?,
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    /// Explicitly replace an existing key at this tick, or insert a new one.
    pub fn set_key(
        &mut self,
        tick: u64,
        value: KeyValue,
        interpolation: Interpolation,
    ) -> Result<(), TimelineError> {
        let mut candidate = self.clone();
        macro_rules! set {
            ($keys:expr,$value:expr) => {{
                let key = Key {
                    tick,
                    value: $value,
                    interpolation,
                };
                match $keys.binary_search_by_key(&tick, |k| k.tick) {
                    Ok(i) => $keys[i] = key,
                    Err(i) => $keys.insert(i, key),
                }
            }};
        }
        match (&mut candidate, value) {
            (Self::Camera(k), KeyValue::Camera(v)) => set!(k, v),
            (Self::Orbit(k), KeyValue::Orbit(v)) => set!(k, v),
            (Self::Tracking(k), KeyValue::Tracking(v)) => set!(k, v),
            (Self::Shake(k), KeyValue::Shake(v)) => set!(k, v),
            (Self::Fov(k), KeyValue::Fov(v))
            | (Self::Speed(k), KeyValue::Speed(v))
            | (Self::TimeOfDay(k), KeyValue::TimeOfDay(v)) => set!(k, v),
            (Self::Timelapse(k), KeyValue::Timelapse(v)) => set!(k, v),
            (Self::Freeze(k), KeyValue::Freeze(v)) => set!(k, v),
            (Self::Audio(k), KeyValue::Audio(v)) => set!(k, v),
            (Self::BlockOverride(k), KeyValue::BlockOverride(v)) => set!(k, v),
            _ => return Err(TimelineError::InvalidOperation),
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
    pub fn kind(&self) -> TrackKind {
        match self {
            Self::Camera(_) => TrackKind::Camera,
            Self::Orbit(_) => TrackKind::Orbit,
            Self::Tracking(_) => TrackKind::Tracking,
            Self::Shake(_) => TrackKind::Shake,
            Self::Fov(_) => TrackKind::Fov,
            Self::Speed(_) => TrackKind::Speed,
            Self::Timelapse(_) => TrackKind::Timelapse,
            Self::TimeOfDay(_) => TrackKind::TimeOfDay,
            Self::Freeze(_) => TrackKind::Freeze,
            Self::Audio(_) => TrackKind::Audio,
            Self::BlockOverride(_) => TrackKind::BlockOverride,
        }
    }
    pub fn last_tick(&self) -> Option<u64> {
        macro_rules! last {
            ($keys:expr) => {
                $keys.last().map(|k| k.tick)
            };
        }
        match self {
            Self::Camera(k) => last!(k),
            Self::Orbit(k) => last!(k),
            Self::Tracking(k) => last!(k),
            Self::Shake(k) => last!(k),
            Self::Fov(k) | Self::Speed(k) | Self::TimeOfDay(k) => last!(k),
            Self::Timelapse(k) => last!(k),
            Self::Freeze(k) => last!(k),
            Self::Audio(k) => last!(k),
            Self::BlockOverride(k) => last!(k),
        }
    }
    pub fn validate(&self) -> Result<(), TimelineError> {
        match self {
            Self::Camera(k) => keys_valid(k, |v| finite(&camera_values(v))),
            Self::Orbit(k) => keys_valid(k, |v| {
                finite(&[
                    v.center.x, v.center.y, v.center.z, v.distance, v.yaw, v.pitch,
                ])?;
                if v.distance < 0.0 {
                    Err(TimelineError::InvalidValue)
                } else {
                    Ok(())
                }
            }),
            Self::Tracking(k) => keys_valid(k, |v| {
                finite(&[v.yaw_offset, v.pitch_offset, v.roll])?;
                finite(&vector(v.position_offset))?;
                finite(&vector(v.view_offset))
            }),
            Self::Shake(k) => keys_valid(k, |v| {
                finite(&[v.frequency_x, v.amplitude_x, v.frequency_y, v.amplitude_y])?;
                if v.frequency_x < 0.0
                    || v.frequency_y < 0.0
                    || v.amplitude_x < 0.0
                    || v.amplitude_y < 0.0
                {
                    Err(TimelineError::InvalidValue)
                } else {
                    Ok(())
                }
            }),
            Self::Fov(k) => scalar_valid(k, |v| (1.0..=179.0).contains(&v)),
            Self::Speed(k) => scalar_valid(k, |v| v >= 0.0),
            Self::TimeOfDay(k) => scalar_valid(k, |_| true),
            Self::Freeze(k) => keys_valid(k, |v| {
                if v.delay <= 10 {
                    Ok(())
                } else {
                    Err(TimelineError::InvalidValue)
                }
            }),
            Self::Timelapse(k) => {
                valid(k)?;
                if k.windows(2).any(|p| p[1].value.ticks <= p[0].value.ticks) {
                    Err(TimelineError::InvalidValue)
                } else {
                    Ok(())
                }
            }
            Self::Audio(k) => keys_valid(k, |v| {
                finite(&[v.gain])?;
                if v.gain < 0.0
                    || v.path.as_os_str().is_empty()
                    || v.duration_ticks.is_some_and(|d| !d.is_finite() || d < 0.0)
                {
                    Err(TimelineError::InvalidValue)
                } else {
                    Ok(())
                }
            }),
            Self::BlockOverride(k) => keys_valid(k, |v| {
                if v.dimension.is_empty() || v.blocks.len() > 1_000_000 {
                    return Err(TimelineError::InvalidValue);
                }
                let mut positions = BTreeSet::new();
                for b in &v.blocks {
                    if b.state.is_empty() || b.state.len() > 1024 || !positions.insert(b.position) {
                        return Err(TimelineError::InvalidValue);
                    }
                }
                Ok(())
            }),
        }
    }
    pub fn sample(&self, tick: f64, hold_last: bool) -> Option<TrackSample> {
        if !tick.is_finite() {
            return None;
        }
        match self {
            Self::Camera(k) => {
                let p = curve(
                    k,
                    tick,
                    camera_values,
                    [0.0, 0.0, 0.0, 360.0, 360.0, 360.0],
                    &[[0, 1, 2]],
                    hold_last,
                )?;
                Some(TrackSample::Camera(CameraKey {
                    position: Vec3 {
                        x: p[0],
                        y: p[1],
                        z: p[2],
                    },
                    yaw: p[3],
                    pitch: p[4],
                    roll: p[5],
                }))
            }
            Self::Orbit(k) => {
                let p = curve(
                    k,
                    tick,
                    |v| {
                        [
                            v.center.x, v.center.y, v.center.z, v.distance, v.yaw, v.pitch,
                        ]
                    },
                    [0.0; 6],
                    &[[0, 1, 2]],
                    hold_last,
                )?;
                Some(TrackSample::Orbit(OrbitKey {
                    center: Vec3 {
                        x: p[0],
                        y: p[1],
                        z: p[2],
                    },
                    distance: p[3],
                    yaw: p[4],
                    pitch: p[5],
                }))
            }
            Self::Tracking(k) => {
                let p = curve(
                    k,
                    tick,
                    |v| {
                        [
                            v.yaw_offset,
                            v.pitch_offset,
                            v.position_offset.x,
                            v.position_offset.y,
                            v.position_offset.z,
                            v.view_offset.x,
                            v.view_offset.y,
                            v.view_offset.z,
                            v.roll,
                        ]
                    },
                    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 360.0],
                    &[[2, 3, 4], [5, 6, 7]],
                    hold_last,
                )?;
                let discrete_tick = if hold_last {
                    tick.min(k.last()?.tick as f64)
                } else {
                    tick
                };
                let discrete = discrete_index(k, discrete_tick)?;
                let base = k[discrete].value;
                Some(TrackSample::Tracking(TrackingKey {
                    target: base.target,
                    body_part: base.body_part,
                    yaw_offset: p[0],
                    pitch_offset: p[1],
                    position_offset: Vec3 {
                        x: p[2],
                        y: p[3],
                        z: p[4],
                    },
                    view_offset: Vec3 {
                        x: p[5],
                        y: p[6],
                        z: p[7],
                    },
                    roll: p[8],
                }))
            }
            Self::Shake(k) => {
                let p = curve(
                    k,
                    tick,
                    |v| [v.frequency_x, v.amplitude_x, v.frequency_y, v.amplitude_y],
                    [0.0; 4],
                    &[],
                    hold_last,
                )?;
                Some(TrackSample::Shake(ShakeKey {
                    frequency_x: p[0],
                    amplitude_x: p[1],
                    frequency_y: p[2],
                    amplitude_y: p[3],
                    split_params: p[0] != p[2] || p[1] != p[3],
                }))
            }
            Self::Fov(k) => {
                let p = curve(
                    k,
                    tick,
                    |v| [1.0 / (v.to_radians() * 0.5).tan()],
                    [0.0],
                    &[],
                    hold_last,
                )?[0];
                Some(TrackSample::Fov((1.0 / p).atan().to_degrees() * 2.0))
            }
            Self::Speed(k) => Some(TrackSample::Speed(
                curve(k, tick, |v| [*v], [0.0], &[], hold_last)?[0],
            )),
            Self::TimeOfDay(k) => Some(TrackSample::TimeOfDay(
                curve(k, tick, |v| [*v], [0.0], &[], hold_last)?[0].trunc(),
            )),
            Self::Timelapse(k) => {
                let lo = active(k, tick, false)?;
                let hi = k.get(lo + 1)?;
                let duration = hi.value.ticks.checked_sub(k[lo].value.ticks)?;
                if duration == 0 {
                    return None;
                }
                Some(TrackSample::Speed(
                    (hi.tick - k[lo].tick) as f64 / duration as f64,
                ))
            }
            Self::Freeze(k) => Some(TrackSample::Freeze(k[active(k, tick, hold_last)?].value)),
            Self::Audio(k) => {
                let i = active(k, tick, true)?;
                let key = &k[i];
                let delta = tick - key.tick as f64;
                if key
                    .value
                    .duration_ticks
                    .is_some_and(|duration| delta > duration)
                {
                    return None;
                }
                Some(TrackSample::Audio {
                    key: key.value.clone(),
                    start: key.tick,
                    offset_seconds: delta / 20.0,
                })
            }
            Self::BlockOverride(k) => {
                Some(TrackSample::Blocks(k[active(k, tick, true)?].value.clone()))
            }
        }
    }
}
