use horizon_flashback::{
    replay::*,
    timeline::{FrameSchedule, Interpolation, Key, Timeline},
};
use std::io::Cursor;

fn event(tick: u64, delta: Delta) -> TickEvent {
    TickEvent { tick, delta }
}

#[test]
fn populated_blocks_roundtrip_and_backward_seek() {
    let mut scene = SceneState::default();
    scene.blocks.insert([1, 64, -2], "minecraft:stone".into());
    let mut replay = Replay::new(0, 2, scene);
    replay.push(event(0, Delta::TimeOfDay(12))).unwrap();
    replay
        .push(event(
            2,
            Delta::BlockSet(BlockState {
                position: [2, 64, -2],
                state: "minecraft:dirt".into(),
            }),
        ))
        .unwrap();
    replay.push(event(2, Delta::TimeOfDay(30))).unwrap();
    replay.rebuild_snapshots();
    replay.push(event(2, Delta::TimeOfDay(31))).unwrap();
    replay
        .push(event(4, Delta::BlockRemove([1, 64, -2])))
        .unwrap();
    replay.rebuild_snapshots();
    let mut bytes = vec![];
    replay.write_to(&mut bytes).unwrap();
    let loaded = Replay::read_from(Cursor::new(bytes)).unwrap();
    assert_eq!(loaded.state_at(4).unwrap().blocks.len(), 1);
    assert_eq!(loaded.state_at(2).unwrap().blocks.len(), 2);
    assert_eq!(loaded.state_at(2).unwrap().time_of_day, 31);
    assert_eq!(loaded.state_at(0).unwrap().time_of_day, 12);
}

#[test]
fn dimension_change_forgets_old_dimension_objects() {
    let mut scene = SceneState {
        dimension: "overworld".into(),
        ..Default::default()
    };
    scene.blocks.insert([0, 0, 0], "stone".into());
    let mut replay = Replay::new(0, 1, scene);
    replay
        .push(event(1, Delta::Dimension("nether".into())))
        .unwrap();
    assert!(replay.state_at(1).unwrap().blocks.is_empty());
    assert_eq!(replay.state_at(0).unwrap().blocks.len(), 1);
}

#[test]
fn incomplete_first_header_is_never_recovered_as_empty_replay() {
    let replay = Replay::new(0, 10, SceneState::default());
    let mut bytes = vec![];
    replay.write_to(&mut bytes).unwrap();
    for end in 0..bytes.len() {
        assert!(
            Replay::read_from(&bytes[..end]).is_err(),
            "accepted first header cut at {end}"
        );
    }
}

#[test]
fn checksum_covers_tick_and_kind_metadata() {
    let replay = Replay::new(0, 10, SceneState::default());
    let mut bytes = vec![];
    replay.write_to(&mut bytes).unwrap();
    bytes[22] ^= 1;
    assert!(Replay::read_from(bytes.as_slice()).is_err());
}

#[test]
fn corrupted_length_is_not_treated_as_an_incomplete_tail() {
    let mut replay = Replay::new(0, 100, SceneState::default());
    let mut header = vec![];
    replay.write_to(&mut header).unwrap();
    replay.push(event(1, Delta::TimeOfDay(10))).unwrap();
    let mut bytes = vec![];
    replay.write_to(&mut bytes).unwrap();
    // Increase the complete event's declared length while keeping it under the bound.
    bytes[header.len()] = bytes[header.len()].wrapping_add(1);
    assert!(Replay::read_from(bytes.as_slice()).is_err());
}

#[test]
fn slow_playback_accumulates_fractional_ticks() {
    let mut playback = Playback::new(0);
    playback.speed = 0.25;
    for _ in 0..4 {
        playback.advance(1.0).unwrap();
    }
    assert_eq!(playback.tick, 1);
    playback.paused = true;
    playback.advance(100.0).unwrap();
    assert_eq!(playback.tick, 1);
    assert!(playback.advance(-1.0).is_err());
}

#[test]
fn invalid_insertion_is_transactional_and_nan_sampling_is_safe() {
    let mut timeline = Timeline::default();
    let before = timeline.clone();
    assert!(
        timeline
            .insert_fov(Key {
                tick: 0,
                value: f64::NAN,
                interpolation: Interpolation::Linear
            })
            .is_err()
    );
    assert_eq!(timeline, before);
    timeline
        .insert_fov(Key {
            tick: 0,
            value: 70.0,
            interpolation: Interpolation::Linear,
        })
        .unwrap();
    timeline
        .insert_fov(Key {
            tick: 5,
            value: 90.0,
            interpolation: Interpolation::Linear,
        })
        .unwrap();
    assert!(timeline.fov_at(f64::NAN).is_none());
    assert!(timeline.fov_at(f64::INFINITY).is_none());
}

#[test]
fn time_of_day_wraps_at_twenty_four_thousand_and_export_clock_uses_game_ticks() {
    let mut timeline = Timeline::default();
    for (tick, value) in [(0, 23_000.0), (10, 1_000.0)] {
        timeline
            .insert_time(Key {
                tick,
                value,
                interpolation: Interpolation::Linear,
            })
            .unwrap();
    }
    assert_eq!(timeline.time_of_day_at(5.0), Some(0.0));
    let schedule = FrameSchedule::new(60_000, 1_001).unwrap();
    assert_eq!(
        schedule.tick_time_nanos(60_000, 20).unwrap(),
        20_020_000_000_000
    );
    assert_eq!(
        FrameSchedule::new(60, 1)
            .unwrap()
            .tick_time_nanos(60, 20)
            .unwrap(),
        20_000_000_000
    );
    assert!(
        FrameSchedule::new(1, u32::MAX)
            .unwrap()
            .tick_time_nanos(u64::MAX, u32::MAX)
            .is_err()
    );
}

#[test]
fn new_take_preserves_unsaved_work() {
    use horizon_flashback::session::Session;
    let mut session = Session::default();
    session.start("overworld");
    session.observe(CameraPose::default());
    session.stop();
    assert!(!session.new_take());
    assert!(session.replay.is_some());
    session.saved = true;
    assert!(session.new_take());
    assert!(session.replay.is_none());
    assert!(session.start("overworld"));
}

#[test]
fn tiny_snapshot_interval_has_bounded_cache_and_keeps_seek_correct() {
    let mut replay = Replay::new(0, 1, SceneState::default());
    for tick in 1..=10_000 {
        replay.push(event(tick, Delta::TimeOfDay(tick))).unwrap();
    }
    replay.rebuild_snapshots();
    assert!(replay.snapshots.len() <= 64);
    assert_eq!(replay.state_at(9999).unwrap().time_of_day, 9999);
    assert_eq!(replay.state_at(2).unwrap().time_of_day, 2);
}
