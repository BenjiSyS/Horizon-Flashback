//! Testable replay storage, recording lifecycle, and deterministic reconstruction.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
};

pub const MAGIC: &[u8; 8] = b"HFRPLY01";
pub const FORMAT_VERSION: u16 = 1;
pub const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 256 * 1024 * 1024;
const MAX_SNAPSHOTS: usize = 64;
const SNAPSHOT_BUDGET: usize = 32 * 1024 * 1024;
const RECORD_HEADER: usize = 4 + 4 + 4 + 8 + 1;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraPose {
    pub position: Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    pub fov: f64,
}
impl Default for CameraPose {
    fn default() -> Self {
        Self {
            position: Vec3::default(),
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            fov: 70.0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityState {
    pub id: u64,
    pub kind: String,
    pub position: Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub data: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockState {
    pub position: [i32; 3],
    pub state: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WeatherState {
    pub rain: f32,
    pub thunder: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SceneState {
    pub camera: CameraPose,
    pub entities: BTreeMap<u64, EntityState>,
    #[serde(with = "block_map")]
    pub blocks: BTreeMap<[i32; 3], String>,
    pub dimension: String,
    pub time_of_day: u64,
    pub weather: WeatherState,
}
// JSON object keys cannot be coordinate arrays. Encode a stable list of pairs.
mod block_map {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        map: &BTreeMap<[i32; 3], String>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        map.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<[i32; 3], String>, D::Error> {
        let entries = Vec::<([i32; 3], String)>::deserialize(deserializer)?;
        let count = entries.len();
        let map: BTreeMap<_, _> = entries.into_iter().collect();
        if map.len() != count {
            return Err(serde::de::Error::custom("duplicate block coordinate"));
        }
        Ok(map)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Delta {
    Camera(CameraPose),
    EntityUpsert(EntityState),
    EntityRemove(u64),
    BlockSet(BlockState),
    BlockRemove([i32; 3]),
    Dimension(String),
    TimeOfDay(u64),
    Weather(WeatherState),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TickEvent {
    pub tick: u64,
    pub delta: Delta,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub tick: u64,
    pub color: u32,
    pub position: Option<Vec3>,
    pub dimension: Option<String>,
    pub description: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayHeader {
    pub version: u16,
    pub initial_tick: u64,
    pub snapshot_interval: u64,
    pub initial: SceneState,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Replay {
    pub header: ReplayHeader,
    pub events: Vec<TickEvent>,
    pub snapshots: BTreeMap<u64, SceneState>,
    pub markers: Vec<Marker>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    Io(String),
    BadMagic,
    UnsupportedVersion(u16),
    CorruptRecord { offset: usize },
    OversizedRecord(usize),
    Invalid(String),
    TickOrder,
}
impl From<io::Error> for ReplayError {
    fn from(e: io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
impl Replay {
    pub fn new(initial_tick: u64, snapshot_interval: u64, initial: SceneState) -> Self {
        let mut snapshots = BTreeMap::new();
        snapshots.insert(initial_tick, initial.clone());
        Self {
            header: ReplayHeader {
                version: FORMAT_VERSION,
                initial_tick,
                snapshot_interval: snapshot_interval.max(1),
                initial,
            },
            events: Vec::new(),
            snapshots,
            markers: Vec::new(),
        }
    }
    pub fn push(&mut self, event: TickEvent) -> Result<(), ReplayError> {
        if self.events.last().is_some_and(|e| e.tick > event.tick)
            || event.tick < self.header.initial_tick
        {
            return Err(ReplayError::TickOrder);
        };
        validate_delta(&event.delta)?;
        self.snapshots
            .retain(|&tick, _| tick == self.header.initial_tick || tick < event.tick);
        self.events.push(event);
        Ok(())
    }
    pub fn add_marker(&mut self, marker: Marker) {
        self.markers.push(marker);
        self.markers.sort_by_key(|m| m.tick)
    }
    pub fn state_at(&self, tick: u64) -> Result<SceneState, ReplayError> {
        let (&base_tick, base) = self
            .snapshots
            .range(..=tick)
            .next_back()
            .unwrap_or((&self.header.initial_tick, &self.header.initial));
        let mut state = base.clone();
        let start = self.events.partition_point(|e| {
            if base_tick == self.header.initial_tick {
                e.tick < base_tick
            } else {
                e.tick <= base_tick
            }
        });
        let end = self.events.partition_point(|e| e.tick <= tick);
        for e in &self.events[start.min(end)..end] {
            apply_delta(&mut state, &e.delta)?;
        }
        Ok(state)
    }
    pub fn rebuild_snapshots(&mut self) {
        self.snapshots.clear();
        self.snapshots
            .insert(self.header.initial_tick, self.header.initial.clone());
        let mut s = self.header.initial.clone();
        let mut i = 0;
        let mut last_snapshot = self.header.initial_tick;
        let mut cache_bytes = 0usize;
        while i < self.events.len() {
            let tick = self.events[i].tick;
            while i < self.events.len() && self.events[i].tick == tick {
                let _ = apply_delta(&mut s, &self.events[i].delta);
                i += 1;
            }
            if tick.saturating_sub(last_snapshot) >= self.header.snapshot_interval.max(1) {
                if self.snapshots.len() >= MAX_SNAPSHOTS {
                    break;
                }
                // Never let a short malicious snapshot interval multiply the entire
                // scene into an unbounded cache. Counting serialization allocates no buffer.
                let mut counter = CountBytes {
                    bytes: 0,
                    limit: SNAPSHOT_BUDGET.saturating_sub(cache_bytes) / 8,
                };
                if serde_json::to_writer(&mut counter, &s).is_err() {
                    break;
                }
                cache_bytes = cache_bytes.saturating_add(counter.bytes.saturating_mul(8));
                self.snapshots.insert(tick, s.clone());
                last_snapshot = tick;
            }
        }
    }
    pub fn write_to<W: Write>(&self, mut w: W) -> Result<(), ReplayError> {
        validate_scene(&self.header.initial)?;
        if self.header.snapshot_interval == 0 {
            return Err(ReplayError::Invalid("zero snapshot interval".into()));
        }
        w.write_all(MAGIC)?;
        w.write_all(&FORMAT_VERSION.to_le_bytes())?;
        let b =
            serde_json::to_vec(&self.header).map_err(|e| ReplayError::Invalid(e.to_string()))?;
        write_record(&mut w, 0, self.header.initial_tick, &b)?;
        for e in &self.events {
            validate_delta(&e.delta)?;
            let b = serde_json::to_vec(e).map_err(|x| ReplayError::Invalid(x.to_string()))?;
            write_record(&mut w, 1, e.tick, &b)?;
        }
        for (&tick, scene) in self
            .snapshots
            .iter()
            .filter(|(tick, _)| **tick > self.header.initial_tick)
        {
            let b = serde_json::to_vec(scene).map_err(|x| ReplayError::Invalid(x.to_string()))?;
            write_record(&mut w, 3, tick, &b)?;
        }
        for m in &self.markers {
            let b = serde_json::to_vec(m).map_err(|x| ReplayError::Invalid(x.to_string()))?;
            write_record(&mut w, 2, m.tick, &b)?;
        }
        Ok(())
    }
    /// Reads records incrementally. An incomplete final frame is recovered as a truncated tail.
    pub fn read_from<R: Read>(mut r: R) -> Result<Self, ReplayError> {
        let mut magic = [0; 8];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(ReplayError::BadMagic);
        }
        let mut v = [0; 2];
        r.read_exact(&mut v)?;
        let version = u16::from_le_bytes(v);
        if version != FORMAT_VERSION {
            return Err(ReplayError::UnsupportedVersion(version));
        }
        let mut replay = None;
        let mut offset = 10usize;
        loop {
            let mut h = [0; RECORD_HEADER];
            let mut got = 0;
            while got < h.len() {
                match r.read(&mut h[got..]) {
                    Ok(0) => break,
                    Ok(n) => got += n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            if got == 0 {
                break;
            }
            if got < h.len() {
                break;
            }
            let len = u32::from_le_bytes(
                h[..4]
                    .try_into()
                    .map_err(|_| ReplayError::CorruptRecord { offset: 0 })?,
            ) as usize;
            if len > MAX_RECORD_BYTES {
                return Err(ReplayError::OversizedRecord(len));
            }
            offset = offset
                .checked_add(RECORD_HEADER + len)
                .ok_or_else(|| ReplayError::Invalid("file length overflow".into()))?;
            if offset > MAX_FILE_BYTES {
                return Err(ReplayError::Invalid("replay exceeds 256 MiB".into()));
            }
            let crc = u32::from_le_bytes(
                h[8..12]
                    .try_into()
                    .map_err(|_| ReplayError::CorruptRecord { offset: 0 })?,
            );
            let tick = u64::from_le_bytes(
                h[12..20]
                    .try_into()
                    .map_err(|_| ReplayError::CorruptRecord { offset: 0 })?,
            );
            let kind = h[20];
            let header_crc = u32::from_le_bytes(
                h[4..8]
                    .try_into()
                    .map_err(|_| ReplayError::CorruptRecord { offset })?,
            );
            if frame_header_crc(len as u32, kind, tick) != header_crc {
                return Err(ReplayError::CorruptRecord { offset });
            }
            let mut payload = vec![0; len];
            let mut n = 0;
            while n < len {
                match r.read(&mut payload[n..]) {
                    Ok(0) => {
                        let mut recovered: Replay =
                            replay.ok_or(ReplayError::CorruptRecord { offset })?;
                        recovered.rebuild_snapshots();
                        return Ok(recovered);
                    }
                    Ok(k) => n += k,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            if record_crc(kind, tick, &payload) != crc {
                return Err(ReplayError::CorruptRecord { offset });
            }
            match kind {
                0 => {
                    if replay.is_some() {
                        return Err(ReplayError::CorruptRecord { offset });
                    }
                    let header: ReplayHeader = serde_json::from_slice(&payload)
                        .map_err(|_| ReplayError::CorruptRecord { offset: 0 })?;
                    if header.version != version {
                        return Err(ReplayError::UnsupportedVersion(header.version));
                    }
                    if header.initial_tick != tick || header.snapshot_interval == 0 {
                        return Err(ReplayError::CorruptRecord { offset });
                    }
                    validate_scene(&header.initial)?;
                    replay = Some(Replay::new(
                        header.initial_tick,
                        header.snapshot_interval,
                        header.initial,
                    ));
                }
                1 => {
                    let e: TickEvent = serde_json::from_slice(&payload)
                        .map_err(|_| ReplayError::CorruptRecord { offset: 0 })?;
                    if e.tick != tick {
                        return Err(ReplayError::CorruptRecord { offset: 0 });
                    }
                    if let Some(r) = replay.as_mut() {
                        r.push(e)?;
                    } else {
                        return Err(ReplayError::CorruptRecord { offset: 0 });
                    }
                }
                2 => {
                    let m: Marker = serde_json::from_slice(&payload)
                        .map_err(|_| ReplayError::CorruptRecord { offset: 0 })?;
                    if m.tick != tick {
                        return Err(ReplayError::CorruptRecord { offset: 0 });
                    }
                    if let Some(r) = replay.as_mut() {
                        r.add_marker(m);
                    } else {
                        return Err(ReplayError::CorruptRecord { offset: 0 });
                    }
                }
                3 => {
                    let scene: SceneState = serde_json::from_slice(&payload)
                        .map_err(|_| ReplayError::CorruptRecord { offset: 0 })?;
                    validate_scene(&scene)?;
                    // Snapshots are an acceleration hint. Rebuild from authoritative events
                    // rather than trusting a file-supplied state that could contradict them.
                    if replay.is_none() {
                        return Err(ReplayError::CorruptRecord { offset: 0 });
                    }
                }
                _ => return Err(ReplayError::CorruptRecord { offset: 0 }),
            }
        }
        let mut replay = replay.ok_or(ReplayError::CorruptRecord { offset })?;
        replay.rebuild_snapshots();
        Ok(replay)
    }
}
struct CountBytes {
    bytes: usize,
    limit: usize,
}
impl Write for CountBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let total = self.bytes.saturating_add(bytes.len());
        if total > self.limit {
            return Err(io::Error::other("snapshot cache budget"));
        }
        self.bytes = total;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn write_record<W: Write>(w: &mut W, kind: u8, tick: u64, p: &[u8]) -> Result<(), ReplayError> {
    if p.len() > MAX_RECORD_BYTES {
        return Err(ReplayError::OversizedRecord(p.len()));
    }
    w.write_all(&(p.len() as u32).to_le_bytes())?;
    w.write_all(&frame_header_crc(p.len() as u32, kind, tick).to_le_bytes())?;
    w.write_all(&record_crc(kind, tick, p).to_le_bytes())?;
    w.write_all(&tick.to_le_bytes())?;
    w.write_all(&[kind])?;
    w.write_all(p)?;
    Ok(())
}
fn frame_header_crc(length: u32, kind: u8, tick: u64) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&length.to_le_bytes());
    hasher.update(&tick.to_le_bytes());
    hasher.update(&[kind]);
    hasher.finalize()
}
fn record_crc(kind: u8, tick: u64, payload: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&[kind]);
    hasher.update(&tick.to_le_bytes());
    hasher.update(payload);
    hasher.finalize()
}
fn validate_camera(v: &CameraPose) -> Result<(), ReplayError> {
    if ![
        v.position.x,
        v.position.y,
        v.position.z,
        v.yaw,
        v.pitch,
        v.roll,
        v.fov,
    ]
    .iter()
    .all(|v| v.is_finite())
        || !(1.0..=179.0).contains(&v.fov)
    {
        return Err(ReplayError::Invalid("invalid camera pose".into()));
    }
    Ok(())
}
fn validate_delta(delta: &Delta) -> Result<(), ReplayError> {
    match delta {
        Delta::Camera(v) => validate_camera(v)?,
        Delta::EntityUpsert(v)
            if ![v.position.x, v.position.y, v.position.z, v.yaw, v.pitch]
                .iter()
                .all(|v| v.is_finite()) =>
        {
            return Err(ReplayError::Invalid("invalid entity pose".into()));
        }
        Delta::Weather(v)
            if ![v.rain, v.thunder]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)) =>
        {
            return Err(ReplayError::Invalid("invalid weather".into()));
        }
        _ => {}
    }
    Ok(())
}
fn validate_scene(scene: &SceneState) -> Result<(), ReplayError> {
    validate_camera(&scene.camera)?;
    validate_delta(&Delta::Weather(scene.weather.clone()))?;
    for (id, entity) in &scene.entities {
        if *id != entity.id {
            return Err(ReplayError::Invalid("entity map ID mismatch".into()));
        }
        validate_delta(&Delta::EntityUpsert(entity.clone()))?;
    }
    Ok(())
}
fn apply_delta(s: &mut SceneState, d: &Delta) -> Result<(), ReplayError> {
    validate_delta(d)?;
    match d {
        Delta::Camera(v) => s.camera = *v,
        Delta::EntityUpsert(v) => {
            s.entities.insert(v.id, v.clone());
        }
        Delta::EntityRemove(id) => {
            s.entities.remove(id);
        }
        Delta::BlockSet(v) => {
            s.blocks.insert(v.position, v.state.clone());
        }
        Delta::BlockRemove(p) => {
            s.blocks.remove(p);
        }
        Delta::Dimension(v) => {
            if s.dimension != *v {
                s.entities.clear();
                s.blocks.clear();
            }
            s.dimension = v.clone();
        }
        Delta::TimeOfDay(v) => s.time_of_day = *v,
        Delta::Weather(v) => s.weather = v.clone(),
    }
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Paused,
    Stopped,
}
#[derive(Debug, Clone)]
pub struct Recorder {
    pub state: RecordingState,
    pub replay: Replay,
    last_tick: u64,
}
impl Recorder {
    pub fn new(tick: u64, scene: SceneState) -> Self {
        Self {
            state: RecordingState::Idle,
            replay: Replay::new(tick, 100, scene),
            last_tick: tick,
        }
    }
    pub fn start(&mut self) -> Result<(), ReplayError> {
        if self.state != RecordingState::Idle {
            return Err(ReplayError::Invalid("recording already started".into()));
        }
        self.state = RecordingState::Recording;
        Ok(())
    }
    pub fn pause(&mut self) {
        if self.state == RecordingState::Recording {
            self.state = RecordingState::Paused
        }
    }
    pub fn resume(&mut self) {
        if self.state == RecordingState::Paused {
            self.state = RecordingState::Recording
        }
    }
    pub fn stop(&mut self) {
        if self.state == RecordingState::Recording || self.state == RecordingState::Paused {
            self.state = RecordingState::Stopped
        }
    }
    pub fn record(&mut self, e: TickEvent) -> Result<(), ReplayError> {
        if self.state != RecordingState::Recording {
            return Err(ReplayError::Invalid("recorder is not recording".into()));
        }
        if e.tick < self.last_tick {
            return Err(ReplayError::TickOrder);
        }
        let tick = e.tick;
        self.replay.push(e)?;
        self.last_tick = tick;
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Playback {
    pub tick: u64,
    pub speed: f64,
    pub paused: bool,
    fraction: f64,
}
impl Playback {
    pub fn new(tick: u64) -> Self {
        Self {
            tick,
            speed: 1.0,
            paused: false,
            fraction: 0.0,
        }
    }
    pub fn advance(&mut self, delta: f64) -> Result<(), ReplayError> {
        if !delta.is_finite() || delta < 0.0 || !self.speed.is_finite() || self.speed < 0.0 {
            return Err(ReplayError::Invalid("invalid playback timing".into()));
        }
        if !self.paused {
            let advance = delta * self.speed + self.fraction;
            if !advance.is_finite() {
                return Err(ReplayError::Invalid("playback timing overflow".into()));
            }
            self.tick = self.tick.saturating_add(advance as u64);
            self.fraction = advance.fract();
        }
        Ok(())
    }
    pub fn step(&mut self, n: i64) {
        self.fraction = 0.0;
        self.tick = if n < 0 {
            self.tick.saturating_sub(n.unsigned_abs())
        } else {
            self.tick.saturating_add(n as u64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(t: u64, d: Delta) -> TickEvent {
        TickEvent { tick: t, delta: d }
    }
    #[test]
    fn record_roundtrip_and_seek() {
        let mut r = Replay::new(0, 2, SceneState::default());
        r.push(e(1, Delta::Dimension("overworld".into()))).unwrap();
        r.push(e(2, Delta::TimeOfDay(1200))).unwrap();
        r.push(e(
            3,
            Delta::Weather(WeatherState {
                rain: 0.5,
                thunder: 0.0,
            }),
        ))
        .unwrap();
        r.rebuild_snapshots();
        let mut bytes = Vec::new();
        r.write_to(&mut bytes).unwrap();
        let rr = Replay::read_from(bytes.as_slice()).unwrap();
        assert_eq!(rr.state_at(2).unwrap().time_of_day, 1200);
        assert_eq!(rr.state_at(1).unwrap().dimension, "overworld");
        assert_eq!(rr.state_at(0).unwrap().time_of_day, 0);
    }
    #[test]
    fn checksum_corruption_truncation_and_size() {
        let mut r = Replay::new(0, 1, SceneState::default());
        r.push(e(1, Delta::TimeOfDay(7))).unwrap();
        let mut b = Vec::new();
        r.write_to(&mut b).unwrap();
        let mut bad = b.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(matches!(
            Replay::read_from(bad.as_slice()),
            Err(ReplayError::CorruptRecord { .. })
        ));
        let recovered = Replay::read_from(&b[..b.len() - 3]).unwrap();
        assert!(recovered.events.is_empty());
        let mut huge = MAGIC.to_vec();
        huge.extend(FORMAT_VERSION.to_le_bytes());
        huge.extend((MAX_RECORD_BYTES as u32 + 1).to_le_bytes());
        huge.extend([0; 17]);
        assert!(matches!(
            Replay::read_from(huge.as_slice()),
            Err(ReplayError::OversizedRecord(_))
        ));
    }
    #[test]
    fn recording_lifecycle() {
        let mut rec = Recorder::new(0, SceneState::default());
        assert!(rec.record(e(1, Delta::TimeOfDay(1))).is_err());
        rec.start().unwrap();
        rec.record(e(1, Delta::TimeOfDay(1))).unwrap();
        rec.pause();
        assert!(rec.record(e(2, Delta::TimeOfDay(2))).is_err());
        rec.resume();
        rec.record(e(2, Delta::TimeOfDay(2))).unwrap();
        rec.stop();
        assert!(rec.record(e(3, Delta::TimeOfDay(3))).is_err());
    }
}
