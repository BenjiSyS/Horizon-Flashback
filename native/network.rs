// Source-level packet capture and offline replay. No socket is created on replay.
use super::{Event, Reason, Session};
use engine::tape::{PacketRecord, TapeMeta, TapeReader, TapeWriter};
pub use horizon_flashback as engine;
use horizon_protocol::{
    buf::Reader,
    connection::ConnWriter,
    packets::{configuration as cfg, play},
    registry::Dynamic,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs::{self, File},
    io::{self, BufReader, BufWriter},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak, mpsc::Sender},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const PREFIX: &str = "flashback-replay:";
const REVISION: &str = "d91353641866b85d851926da3b1a9fdee46392f1";
const JOIN: u8 = 1;
const PACKET: u8 = 2;
const FEATURES: u8 = 3;
const BRAND: u8 = 4;
const RECONFIGURING: u8 = 5;
const RECONFIGURED: u8 = 6;
const REGISTRY: u8 = 7;
const MAX_QUEUED: usize = 64;
const MAX_QUEUED_BYTES: usize = 8 * 1024 * 1024;
const MAX_REGISTRY_BYTES: usize = 64 * 1024 * 1024;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}
#[derive(Default)]
struct Arm {
    next: Option<PathBuf>,
    status: String,
}
fn arm_state() -> &'static Mutex<Arm> {
    static STATE: OnceLock<Mutex<Arm>> = OnceLock::new();
    STATE.get_or_init(Default::default)
}
fn captures() -> &'static Mutex<HashMap<usize, Capture>> {
    static CAPTURES: OnceLock<Mutex<HashMap<usize, Capture>>> = OnceLock::new();
    CAPTURES.get_or_init(Default::default)
}
fn controls() -> &'static Mutex<HashMap<usize, Weak<Control>>> {
    static CONTROLS: OnceLock<Mutex<HashMap<usize, Weak<Control>>>> = OnceLock::new();
    CONTROLS.get_or_init(Default::default)
}
fn key(writer: &Mutex<Option<ConnWriter>>) -> usize {
    std::ptr::from_ref(writer) as usize
}
fn session_key(session: &Session) -> usize {
    key(&session.writer)
}

struct Capture {
    writer: TapeWriter<BufWriter<File>>,
    started: Option<Instant>,
    path: PathBuf,
    last_flush: Instant,
}
impl Capture {
    fn record(&mut self, kind: u8, payload: Vec<u8>) -> io::Result<()> {
        let started = self.started.get_or_insert_with(Instant::now);
        let tick = (started.elapsed().as_millis() / 50).min(u64::MAX as u128) as u64;
        self.writer.append(&PacketRecord {
            tick,
            kind,
            payload,
        })?;
        if self.last_flush.elapsed() >= Duration::from_secs(1) {
            self.writer.flush()?;
            self.last_flush = Instant::now();
        }
        Ok(())
    }
}
fn close_capture(mut capture: Capture) -> String {
    match capture.writer.flush() {
        Ok(()) => format!("World tape saved: {}", capture.path.display()),
        Err(error) => format!("World tape flush failed: {error}"),
    }
}
pub fn recording_status() -> String {
    lock(arm_state()).status.clone()
}
/// Toggle opt-in capture. Arming applies only to the next new connection.
pub fn toggle_recording(directory: &Path) -> Result<String, String> {
    let active: Vec<_> = lock(captures())
        .drain()
        .map(|(_, capture)| capture)
        .collect();
    let mut arm = lock(arm_state());
    if !active.is_empty() || arm.next.is_some() {
        arm.next = None;
        arm.status = active
            .into_iter()
            .map(close_capture)
            .last()
            .unwrap_or_else(|| "World recording disarmed".into());
    } else {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        arm.next = Some(directory.to_path_buf());
        arm.status = "World recording armed: join/rejoin a world to begin".into();
    }
    Ok(arm.status.clone())
}
fn begin_capture(identity: usize) {
    let mut captures = lock(captures());
    let mut arm = lock(arm_state());
    let Some(directory) = arm.next.take() else {
        return;
    };
    let result = (|| -> io::Result<Capture> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let path = directory.join(format!("world-{stamp}.hfnet"));
        let file = File::options().write(true).create_new(true).open(&path)?;
        let meta = TapeMeta {
            schema: 1,
            horizon_revision: REVISION.into(),
            protocol: horizon_protocol::PROTOCOL_VERSION,
            game_version: horizon_protocol::GAME_VERSION.into(),
        };
        Ok(Capture {
            writer: TapeWriter::new(BufWriter::new(file), &meta)?,
            started: None,
            path,
            last_flush: Instant::now(),
        })
    })();
    match result {
        Ok(capture) => {
            arm.status = format!("Recording world tape: {}", capture.path.display());
            captures.insert(identity, capture);
        }
        Err(error) => arm.status = format!("World recording failed: {error}"),
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JoinMeta {
    uuid: u128,
    name: String,
    dialog_tags: BTreeMap<String, Vec<i32>>,
    custom_components: bool,
}
fn record_registries(
    capture: &mut Capture,
    registries: &Dynamic,
    kind: u8,
    uuid: u128,
    name: &str,
) -> io::Result<()> {
    let sorted: BTreeMap<_, _> = registries.registries.iter().collect();
    for (registry, entries) in sorted {
        capture.record(
            REGISTRY,
            cfg::Clientbound::RegistryData(cfg::RegistryData {
                registry: registry.clone(),
                entries: entries
                    .iter()
                    .map(|(id, data)| cfg::RegistryEntry {
                        id: id.clone(),
                        data: data.clone(),
                    })
                    .collect(),
            })
            .encode(),
        )?;
    }
    let meta = JoinMeta {
        uuid,
        name: name.into(),
        dialog_tags: registries
            .dialog_tags
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        custom_components: registries.custom_components,
    };
    capture.record(kind, serde_json::to_vec(&meta).map_err(io::Error::other)?)
}
fn allowed_packet(packet: &play::Clientbound) -> bool {
    match packet {
        // Replay never restores cookies, follows transfers, downloads packs, opens
        // server dialogs, or executes other mods' arbitrary control payloads.
        play::Clientbound::CookieRequest(_)
        | play::Clientbound::StoreCookie(_)
        | play::Clientbound::Transfer(_)
        | play::Clientbound::ResourcePackPush(_)
        | play::Clientbound::ResourcePackPop(_)
        | play::Clientbound::ServerLinks(_)
        | play::Clientbound::ShowDialog(_)
        | play::Clientbound::ClearDialog(_)
        | play::Clientbound::Disconnect(_) => false,
        play::Clientbound::CustomPayload(payload) => {
            payload.channel == horizon_protocol::attachment_sync::CHANNEL
        }
        _ => true,
    }
}
fn record_event(capture: &mut Capture, event: &Event) -> io::Result<()> {
    match event {
        Event::Joined {
            registries,
            uuid,
            name,
        } => record_registries(capture, registries, JOIN, *uuid, name),
        Event::Reconfigured(registries) => {
            record_registries(capture, registries, RECONFIGURED, 0, "")
        }
        Event::Packet(packet) if allowed_packet(packet) => capture.record(PACKET, packet.encode()),
        Event::Features(features) => capture.record(
            FEATURES,
            serde_json::to_vec(features).map_err(io::Error::other)?,
        ),
        Event::Brand(brand) => {
            capture.record(BRAND, serde_json::to_vec(brand).map_err(io::Error::other)?)
        }
        Event::Reconfiguring => capture.record(RECONFIGURING, Vec::new()),
        _ => Ok(()),
    }
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub tick: f64,
    pub paused: bool,
    pub ended: bool,
    pub duration: Option<u64>,
}
struct Clock {
    tick: f64,
    speed: f64,
    paused: bool,
    ended: bool,
    cancelled: bool,
    host_backlog: bool,
    last: Instant,
    pending: VecDeque<usize>,
    pending_bytes: usize,
    duration: Option<u64>,
}
impl Clock {
    fn update(&mut self) {
        let now = Instant::now();
        if !self.paused && !self.ended {
            self.tick += now.duration_since(self.last).as_secs_f64() * 20.0 * self.speed;
        }
        self.last = now;
    }
}
struct Control {
    clock: Mutex<Clock>,
    changed: Condvar,
}
fn control(session: &Session) -> Option<Arc<Control>> {
    lock(controls())
        .get(&session_key(session))
        .and_then(Weak::upgrade)
}
pub fn progress(session: &Session) -> Option<Progress> {
    let control = control(session)?;
    let mut clock = lock(&control.clock);
    clock.update();
    Some(Progress {
        tick: clock.tick,
        paused: clock.paused,
        ended: clock.ended,
        duration: clock.duration,
    })
}
pub fn toggle_pause(session: &Session) -> bool {
    let Some(control) = control(session) else {
        return false;
    };
    let mut clock = lock(&control.clock);
    clock.update();
    clock.paused = !clock.paused;
    control.changed.notify_all();
    true
}
pub fn set_speed(session: &Session, speed: f64) -> bool {
    if !speed.is_finite() || !(0.05..=16.0).contains(&speed) {
        return false;
    }
    let Some(control) = control(session) else {
        return false;
    };
    let mut clock = lock(&control.clock);
    clock.update();
    clock.speed = speed;
    control.changed.notify_all();
    true
}
pub fn host_backlog(session: &Session, full: bool) {
    if let Some(control) = control(session) {
        lock(&control.clock).host_backlog = full;
        control.changed.notify_all();
    }
}
pub fn disconnect(session: &Session) {
    if let Some(control) = lock(controls())
        .remove(&session_key(session))
        .and_then(|w| w.upgrade())
    {
        lock(&control.clock).cancelled = true;
        control.changed.notify_all();
    }
    let capture = lock(captures()).remove(&session_key(session));
    if let Some(capture) = capture {
        lock(arm_state()).status = close_capture(capture);
    }
}

pub fn poll(session: &Session, events: Vec<Event>) -> Vec<Event> {
    if let Some(control) = control(session) {
        let mut clock = lock(&control.clock);
        for _ in 0..events.len() {
            if let Some(bytes) = clock.pending.pop_front() {
                clock.pending_bytes = clock.pending_bytes.saturating_sub(bytes);
            }
        }
        control.changed.notify_all();
        return events;
    }
    let mut captures = lock(captures());
    let Some(capture) = captures.get_mut(&session_key(session)) else {
        return events;
    };
    let mut failure = None;
    for event in &events {
        if let Err(error) = record_event(capture, event) {
            failure = Some(error);
            break;
        }
    }
    if let Some(error) = failure {
        captures.remove(&session_key(session));
        lock(arm_state()).status = format!("World recording stopped after write failure: {error}");
    }
    events
}

fn wait_event(
    control: &Control,
    tick: u64,
    cost: usize,
    send: &Sender<Event>,
    event: Event,
) -> bool {
    let mut clock = lock(&control.clock);
    loop {
        clock.update();
        if clock.cancelled {
            return false;
        }
        let room = clock.pending.is_empty()
            || (clock.pending.len() < MAX_QUEUED
                && clock.pending_bytes.saturating_add(cost) <= MAX_QUEUED_BYTES);
        if tick as f64 <= clock.tick && room && !clock.host_backlog {
            clock.pending.push_back(cost);
            clock.pending_bytes = clock.pending_bytes.saturating_add(cost);
            drop(clock);
            return send.send(event).is_ok();
        }
        clock = control
            .changed
            .wait_timeout(clock, Duration::from_millis(20))
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}
pub fn replay_address(path: &Path, tick: u64) -> String {
    format!("{PREFIX}{tick}:{}", path.display())
}
pub fn parse_address(address: &str) -> Result<Option<(u64, PathBuf)>, String> {
    let Some(replay) = address.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    let (tick, path) = replay
        .split_once(':')
        .ok_or("Replay address needs a tick and path")?;
    let tick = tick
        .parse::<u64>()
        .map_err(|_| "Invalid replay seek tick")?;
    if path.is_empty() {
        return Err("Missing replay path".into());
    }
    Ok(Some((tick, PathBuf::from(path))))
}
pub fn latest(directory: &Path) -> Result<PathBuf, String> {
    let mut paths = fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "hfnet")
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("world-"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.pop().ok_or_else(|| "No world tapes found".into())
}

/// Called at the HEAD of net::run, before any address parsing, sockets or auth.
/// Some always cancels the original body, including malformed replay addresses.
pub fn run(
    address: &str,
    writer: &Mutex<Option<ConnWriter>>,
    send: &Sender<Event>,
) -> Option<Result<Reason, Reason>> {
    let parsed = match parse_address(address) {
        Ok(None) => {
            begin_capture(key(writer));
            return None;
        }
        Ok(Some(parsed)) => parsed,
        Err(error) => return Some(Ok(Reason::plain(error))),
    };
    let outcome = play_file(&parsed.1, parsed.0, key(writer), send);
    Some(Ok(Reason::plain(match outcome {
        Ok(()) => "Replay closed".into(),
        Err(error) => format!("Replay failed: {error}"),
    })))
}
fn play_file(path: &Path, seek: u64, identity: usize, send: &Sender<Event>) -> io::Result<()> {
    let control = Arc::new(Control {
        clock: Mutex::new(Clock {
            tick: seek as f64,
            speed: 1.0,
            paused: seek != 0,
            ended: false,
            cancelled: false,
            host_backlog: false,
            last: Instant::now(),
            pending: VecDeque::new(),
            pending_bytes: 0,
            duration: None,
        }),
        changed: Condvar::new(),
    });
    lock(controls()).insert(identity, Arc::downgrade(&control));
    struct Remove(usize);
    impl Drop for Remove {
        fn drop(&mut self) {
            lock(controls()).remove(&self.0);
        }
    }
    let _remove = Remove(identity);
    // Register cancellation before disk IO. An early Session drop either cancels
    // this control or closes the channel; neither leaves a detached file reader.
    if !wait_event(
        &control,
        0,
        0,
        send,
        Event::Status("Opening world replay".into()),
    ) {
        return Ok(());
    }
    if lock(&control.clock).cancelled {
        return Ok(());
    }
    let mut tape = TapeReader::new(BufReader::new(File::open(path)?))?;
    if tape.metadata.horizon_revision != REVISION
        || tape.metadata.protocol != horizon_protocol::PROTOCOL_VERSION
        || tape.metadata.game_version != horizon_protocol::GAME_VERSION
    {
        return Err(io::Error::other(
            "Replay requires its exact recorded Horizon/protocol/game version",
        ));
    }
    let mut registries = Dynamic::default();
    let mut registry_bytes = 0usize;
    let mut joined = false;
    let mut last_tick = 0;
    loop {
        if lock(&control.clock).cancelled {
            return Ok(());
        }
        let Some(record) = tape.next_record()? else {
            break;
        };
        last_tick = record.tick;
        let mut cost = record.payload.len();
        let event = match record.kind {
            REGISTRY => {
                registry_bytes = registry_bytes.saturating_add(record.payload.len());
                if registry_bytes > MAX_REGISTRY_BYTES {
                    return Err(io::Error::other("Replay registry snapshot exceeds 64 MiB"));
                }
                let mut reader = Reader::new(&record.payload);
                let id = reader.varint().map_err(io::Error::other)?;
                let cfg::Clientbound::RegistryData(packet) =
                    cfg::Clientbound::decode(id, reader.rest()).map_err(io::Error::other)?
                else {
                    return Err(io::Error::other("Invalid registry record"));
                };
                registries.add(packet);
                continue;
            }
            JOIN | RECONFIGURED => {
                let meta: JoinMeta =
                    serde_json::from_slice(&record.payload).map_err(io::Error::other)?;
                cost = cost.saturating_add(registry_bytes);
                registries.dialog_tags = meta.dialog_tags.into_iter().collect();
                registries.custom_components = meta.custom_components;
                let registries = Arc::new(std::mem::take(&mut registries));
                registry_bytes = 0;
                if record.kind == JOIN {
                    if joined {
                        return Err(io::Error::other("Duplicate replay join"));
                    }
                    joined = true;
                    Event::Joined {
                        registries,
                        uuid: meta.uuid,
                        name: meta.name,
                    }
                } else {
                    if !joined {
                        return Err(io::Error::other("Reconfiguration before replay join"));
                    }
                    Event::Reconfigured(registries)
                }
            }
            PACKET => {
                if !joined {
                    return Err(io::Error::other("Play packet before replay join"));
                }
                let mut reader = Reader::new(&record.payload);
                let id = reader.varint().map_err(io::Error::other)?;
                let packet =
                    play::Clientbound::decode(id, reader.rest()).map_err(io::Error::other)?;
                if !allowed_packet(&packet) {
                    return Err(io::Error::other(
                        "Replay contains a forbidden side-effect packet",
                    ));
                }
                Event::Packet(Box::new(packet))
            }
            FEATURES => {
                Event::Features(serde_json::from_slice(&record.payload).map_err(io::Error::other)?)
            }
            BRAND => {
                Event::Brand(serde_json::from_slice(&record.payload).map_err(io::Error::other)?)
            }
            RECONFIGURING => {
                if !record.payload.is_empty() || !joined {
                    return Err(io::Error::other("Invalid replay reconfiguration"));
                }
                Event::Reconfiguring
            }
            _ => return Err(io::Error::other("Unknown network replay event")),
        };
        if !wait_event(&control, record.tick, cost, send, event) {
            return Ok(());
        }
    }
    if !joined {
        return Err(io::Error::other("Replay has no completed join"));
    }
    let mut clock = lock(&control.clock);
    clock.ended = true;
    clock.paused = true;
    clock.tick = last_tick as f64;
    clock.duration = Some(last_tick);
    if tape.recovered_tail {
        lock(arm_state()).status = "Recovered incomplete world tape tail".into();
    }
    // Keep the last world visible until the viewer closes; returning would
    // trigger Horizon's ordinary Disconnected event and clear that world.
    while !clock.cancelled {
        clock = control
            .changed
            .wait_timeout(clock, Duration::from_millis(50))
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TempTape(PathBuf);
    impl Drop for TempTape {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    fn meta() -> TapeMeta {
        TapeMeta {
            schema: 1,
            horizon_revision: REVISION.into(),
            protocol: horizon_protocol::PROTOCOL_VERSION,
            game_version: horizon_protocol::GAME_VERSION.into(),
        }
    }
    fn create(records: &[(u64, play::Clientbound)]) -> TempTape {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "horizon-flashback-net-test-{}-{stamp}.hfnet",
            std::process::id()
        ));
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let mut capture = Capture {
            writer: TapeWriter::new(BufWriter::new(file), &meta()).unwrap(),
            started: None,
            path: path.clone(),
            last_flush: Instant::now(),
        };
        let mut registries = Dynamic::default();
        registries.add(cfg::RegistryData {
            registry: "minecraft:dimension_type".into(),
            entries: vec![cfg::RegistryEntry {
                id: "minecraft:overworld".into(),
                data: None,
            }],
        });
        record_registries(&mut capture, &registries, JOIN, 42, "ReplayTest").unwrap();
        for (tick, packet) in records {
            capture
                .writer
                .append(&PacketRecord {
                    tick: *tick,
                    kind: PACKET,
                    payload: packet.encode(),
                })
                .unwrap();
        }
        capture.writer.flush().unwrap();
        drop(capture);
        TempTape(path)
    }
    fn block(tick: i32) -> play::Clientbound {
        play::Clientbound::BlockUpdate(play::BlockUpdate {
            pos: [tick, 64, -1],
            state: tick + 1,
        })
    }
    fn connect(tape: &TempTape, seek: u64) -> Session {
        Session::connect(
            &replay_address(&tape.0, seek),
            "Viewer",
            None,
            cfg::ClientInformation::new(8),
            Arc::new(horizon_protocol::text::Language::new()),
            PathBuf::new(),
        )
    }
    fn collect(session: &Session, wanted: usize) -> Vec<Event> {
        let until = Instant::now() + Duration::from_secs(5);
        let mut events = Vec::new();
        while events.len() < wanted && Instant::now() < until {
            events.extend(
                session
                    .poll()
                    .into_iter()
                    .filter(|event| !matches!(event, Event::Status(_))),
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            events.len() >= wanted,
            "Replay timed out after {} events",
            events.len()
        );
        events
    }
    fn close(mut session: Session) {
        let control = control(&session).expect("offline control exists");
        session.disconnect();
        assert!(lock(&control.clock).cancelled);
        let until = Instant::now() + Duration::from_secs(2);
        while Arc::strong_count(&control) > 1 && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            Arc::strong_count(&control),
            1,
            "Replay worker failed to exit"
        );
        assert!(!lock(controls()).contains_key(&session_key(&session)));
    }
    #[test]
    fn replays_registry_and_block_packets_with_no_live_writer() {
        let tape = create(&[(0, block(0)), (1, block(1))]);
        let session = connect(&tape, 0);
        let events = collect(&session, 3);
        assert!(events.iter().any(|event| matches!(event, Event::Joined { registries, uuid:42, name } if name=="ReplayTest" && registries.name("minecraft:dimension_type",0)==Some("minecraft:overworld"))));
        assert_eq!(events.iter().filter(|event| matches!(event, Event::Packet(p) if matches!(&**p,play::Clientbound::BlockUpdate(_)))).count(),2);
        assert!(lock(&session.writer).is_none());
        session.send(play::Serverbound::ClientTickEnd(
            horizon_protocol::packets::Empty,
        ));
        assert!(lock(&session.writer).is_none());
        close(session);
    }
    #[test]
    fn seek_reconstructs_prefix_then_waits_paused_and_can_resume() {
        let tape = create(&[(0, block(0)), (2, block(2)), (4, block(4))]);
        let session = connect(&tape, 2);
        let events = collect(&session, 3);
        assert_eq!(events.len(), 3);
        let p = progress(&session).unwrap();
        assert_eq!(p.tick, 2.0);
        assert!(p.paused);
        assert!(session.poll().is_empty());
        assert!(!set_speed(&session, f64::NAN));
        assert!(set_speed(&session, 16.0));
        assert!(toggle_pause(&session));
        let events = collect(&session, 1);
        assert!(events.iter().any(|event| matches!(event, Event::Packet(p) if matches!(&**p,play::Clientbound::BlockUpdate(b) if b.pos[0]==4))));
        close(session);
    }
    #[test]
    fn malformed_replay_addresses_never_fall_through_to_network_login() {
        let session = Session::connect(
            "flashback-replay:broken",
            "Viewer",
            None,
            cfg::ClientInformation::new(8),
            Arc::new(horizon_protocol::text::Language::new()),
            PathBuf::new(),
        );
        let events = collect(&session, 1);
        assert!(events.iter().any(|event| matches!(event, Event::Disconnected { reason, from_server:true, .. } if reason.contains("Replay address"))));
        assert!(lock(&session.writer).is_none());
        assert!(parse_address("example.org:25565").unwrap().is_none());
        assert!(parse_address("flashback-replay:0:").is_err());
    }
    #[test]
    fn queue_and_host_backlog_apply_backpressure_and_close_still_cancels() {
        let tape = create(&(0..200).map(|i| (0, block(i))).collect::<Vec<_>>());
        let session = connect(&tape, 0);
        let until = Instant::now() + Duration::from_secs(5);
        let control = loop {
            if let Some(control) = control(&session) {
                break control;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        };
        std::thread::sleep(Duration::from_millis(30));
        {
            let clock = lock(&control.clock);
            assert!(clock.pending.len() <= MAX_QUEUED);
            assert!(clock.pending_bytes <= MAX_QUEUED_BYTES);
        }
        host_backlog(&session, true);
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            session.poll();
            if lock(&control.clock).pending.is_empty() {
                break;
            }
            assert!(Instant::now() < until, "In-flight event failed to drain");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(session.poll().is_empty());
        drop(control);
        close(session);
    }
    #[test]
    fn forbidden_side_effect_packet_is_rejected_during_replay() {
        let packet = play::Clientbound::CustomPayload(cfg::CustomPayload {
            channel: "malicious:control".into(),
            data: vec![1, 2, 3],
        });
        let tape = create(&[(0, packet)]);
        let session = connect(&tape, 0);
        let events = collect(&session, 2);
        assert!(events.iter().any(|event| matches!(event, Event::Disconnected { reason, .. } if reason.contains("forbidden side-effect"))));
        assert!(lock(&session.writer).is_none());
    }

    #[test]
    fn closing_immediately_before_polling_stops_the_network_worker() {
        let tape = create(&[(0, block(0)), (10000, block(1))]);
        let mut session = connect(&tape, 0);
        let thread = session.thread.take().expect("network worker was started");
        drop(session);
        let until = Instant::now() + Duration::from_secs(2);
        while !thread.is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            thread.is_finished(),
            "Early close left replay worker running"
        );
        thread.join().unwrap();
    }
}
