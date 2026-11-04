use horizon_flashback::cinematic::*;
use horizon_flashback::session::Session;
use horizon_flashback::timeline::{CameraKey, Editor, Interpolation, Key, Timeline, Vec3};

fn key<T>(tick: u64, value: T) -> Key<T> {
    Key {
        tick,
        value,
        interpolation: Interpolation::Linear,
    }
}
fn orbit(yaw: f64) -> OrbitKey {
    OrbitKey {
        center: Vec3 {
            x: 10.0,
            y: 64.0,
            z: 20.0,
        },
        distance: 5.0,
        yaw,
        pitch: 0.0,
    }
}
fn tracking(target: u128) -> TrackingKey {
    TrackingKey {
        target: EntityId::from_u128(target),
        body_part: BodyPart::Head,
        yaw_offset: 0.0,
        pitch_offset: 0.0,
        position_offset: Vec3::default(),
        view_offset: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 2.0,
        },
        roll: 10.0,
    }
}

#[test]
fn orbit_preserves_full_rotations_and_tracking_uses_entity_relative_offsets() {
    let track = TrackData::Orbit(vec![key(0, orbit(0.0)), key(20, orbit(720.0))]);
    let TrackSample::Orbit(mid) = track.sample(10.0, false).unwrap() else {
        panic!()
    };
    assert_eq!(mid.yaw, 360.0);
    let camera = mid.camera();
    assert!((camera.position.x - 10.0).abs() < 1e-10);
    assert!((camera.position.z - 15.0).abs() < 1e-10);
    let pose = TargetPose {
        position: Vec3 {
            x: 10.0,
            y: 64.0,
            z: 20.0,
        },
        height: 2.0,
        eye_height: 1.6,
        yaw: 25.0,
        body_yaw: 0.0,
        head_yaw: 90.0,
        pitch: 0.0,
    };
    let camera = tracking(1).camera(pose);
    assert!((camera.position.x - 12.0).abs() < 1e-10);
    assert!((camera.position.y - 65.6).abs() < 1e-10);
    assert_eq!((camera.yaw, camera.pitch, camera.roll), (90.0, 0.0, 10.0));
    let mut root = tracking(1);
    root.body_part = BodyPart::Root;
    let camera = root.camera(pose);
    assert!((camera.position.z - 18.0).abs() < 1e-10);
    assert_eq!((camera.position.y, camera.yaw), (64.0, 0.0));
}

#[test]
fn discrete_targets_obey_sided_easing_and_missing_targets_preserve_previous_camera() {
    let mut keys = vec![key(0, tracking(1)), key(10, tracking(2))];
    keys[0].interpolation = Interpolation::EaseOut;
    let data = TrackData::Tracking(keys);
    let TrackSample::Tracking(at_six) = data.sample(6.0, false).unwrap() else {
        panic!()
    };
    assert_eq!(at_six.target, EntityId::from_u128(1)); // Cubic eased amount is 0.216.
    let mut timeline = Timeline::default();
    timeline.tracks.push(Track::new(
        1,
        TrackData::Orbit(vec![key(0, orbit(0.0)), key(10, orbit(90.0))]),
    ));
    timeline.tracks.push(Track::new(2, data));
    assert!(timeline.camera_at(6.0).is_some());
    let camera = timeline
        .camera_at_with_targets(6.0, |id| {
            assert_eq!(id, EntityId::from_u128(1));
            Some(TargetPose {
                position: Vec3::default(),
                height: 2.0,
                eye_height: 1.5,
                yaw: 0.0,
                body_yaw: 0.0,
                head_yaw: 0.0,
                pitch: 0.0,
            })
        })
        .unwrap();
    assert_eq!(camera.position.y, 1.5);
}

#[test]
fn track_order_enable_state_and_tickrate_precedence_match_change_families() {
    let mut timeline = Timeline {
        tracks: vec![
            Track::new(1, TrackData::Fov(vec![key(0, 60.0), key(10, 120.0)])),
            Track::new(2, TrackData::Fov(vec![key(0, 90.0), key(10, 90.0)])),
            Track::new(
                3,
                TrackData::Timelapse(vec![
                    key(0, TimelapseKey { ticks: 0 }),
                    key(10, TimelapseKey { ticks: 20 }),
                ]),
            ),
            Track::new(4, TrackData::Speed(vec![key(0, 3.0), key(10, 3.0)])),
        ],
        ..Timeline::default()
    };
    let expected = (1.0
        / ((1.0 / (30f64.to_radians()).tan() + 1.0 / (60f64.to_radians()).tan()) * 0.5))
        .atan()
        .to_degrees()
        * 2.0;
    assert!((timeline.fov_at(5.0).unwrap() - expected).abs() < 1e-10);
    assert_eq!(timeline.speed_at(5.0), Some(0.5));
    assert_eq!(timeline.changes_at(5.0).count(), 2);
    timeline.tracks[0].enabled = false;
    assert!((timeline.fov_at(5.0).unwrap() - 90.0).abs() < 1e-10);
    assert!(timeline.fov_at(-1.0).is_none());
    assert!(timeline.speed_at(11.0).is_none());
}

#[test]
fn every_track_family_roundtrips_and_audio_tracks_can_overlap() {
    let families = vec![
        TrackData::Camera(vec![key(0, CameraKey::default())]),
        TrackData::Orbit(vec![key(0, orbit(45.0))]),
        TrackData::Tracking(vec![key(0, tracking(42))]),
        TrackData::Shake(vec![key(
            0,
            ShakeKey {
                frequency_x: 2.0,
                amplitude_x: 1.0,
                frequency_y: 3.0,
                amplitude_y: 0.5,
                split_params: true,
            },
        )]),
        TrackData::Fov(vec![key(0, 70.0)]),
        TrackData::Speed(vec![key(0, 1.0)]),
        TrackData::Timelapse(vec![
            key(0, TimelapseKey { ticks: 0 }),
            key(20, TimelapseKey { ticks: 10 }),
        ]),
        TrackData::TimeOfDay(vec![key(0, 48000.0)]),
        TrackData::Freeze(vec![Key {
            tick: 0,
            value: FreezeKey {
                frozen: true,
                delay: 5,
            },
            interpolation: Interpolation::Step,
        }]),
        TrackData::Audio(vec![key(
            0,
            AudioKey {
                path: "music.wav".into(),
                duration_ticks: Some(100.0),
                gain: 0.75,
            },
        )]),
        TrackData::BlockOverride(vec![key(
            0,
            BlockOverrideKey {
                dimension: "minecraft:overworld".into(),
                blocks: vec![],
            },
        )]),
    ];
    let mut timeline = Timeline {
        tracks: families
            .into_iter()
            .enumerate()
            .map(|(i, d)| Track::new(i as u64, d))
            .collect(),
        ..Timeline::default()
    };
    let mut second = timeline.tracks[9].clone();
    second.id = 100;
    timeline.tracks.push(second);
    let json = timeline.to_json().unwrap();
    assert_eq!(Timeline::from_json(&json).unwrap(), timeline);
    let audio: Vec<_> = timeline
        .changes_at(40.0)
        .filter(|change| matches!(change, TrackSample::Audio { .. }))
        .collect();
    assert_eq!(audio.len(), 2);
    assert!(matches!(
        &audio[0],
        TrackSample::Audio {
            offset_seconds: 2.0,
            ..
        }
    ));
    assert_eq!(
        timeline
            .changes_at(101.0)
            .filter(|change| matches!(change, TrackSample::Audio { .. }))
            .count(),
        0
    );
    assert!(timeline.changes_at(1000.0).any(|c| matches!(
        c,
        TrackSample::Freeze(FreezeKey {
            frozen: true,
            delay: 5
        })
    )));
}

#[test]
fn generic_key_edits_are_transactional_and_track_history_restores_order() {
    let mut editor = Editor::new(Timeline::default());
    editor
        .add_track(Track::new(1, TrackData::empty(TrackKind::Orbit)))
        .unwrap();
    editor
        .set_track_key(1, 10, KeyValue::Orbit(orbit(45.0)), Interpolation::Hermite)
        .unwrap();
    editor
        .edit_track_key(1, KeyEdit::Copy { from: 10, to: 20 })
        .unwrap();
    let before = editor.timeline.clone();
    assert!(
        editor
            .edit_track_key(1, KeyEdit::Move { from: 10, to: 20 })
            .is_err()
    );
    assert!(
        editor
            .set_track_key(1, 10, KeyValue::Fov(90.0), Interpolation::Linear)
            .is_err()
    );
    assert_eq!(editor.timeline, before);
    editor
        .edit_track_key(1, KeyEdit::Move { from: 20, to: 30 })
        .unwrap();
    assert_eq!(editor.timeline.tracks[0].data.key_times().len(), 2);
    assert!(editor.undo());
    assert_eq!(editor.timeline, before);
    assert!(editor.redo());
    editor
        .add_track(Track::new(2, TrackData::empty(TrackKind::Audio)))
        .unwrap();
    editor.move_track(2, 0).unwrap();
    assert_eq!(editor.timeline.tracks[0].id, 2);
    editor
        .update_track(2, "Music".into(), 0xff00ff00, false)
        .unwrap();
    assert!(!editor.timeline.tracks[0].enabled);
    editor.delete_track(2).unwrap();
    assert_eq!(editor.timeline.tracks.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.timeline.tracks[0].name, "Music");
}

#[test]
fn project_versions_preserve_tracks_and_loading_cannot_discard_unsaved_work() {
    let mut session = Session::default();
    session.duration = 20;
    session
        .editor
        .add_track(Track::new(
            1,
            TrackData::Orbit(vec![key(0, orbit(0.0)), key(20, orbit(180.0))]),
        ))
        .unwrap();
    let mut saved = Vec::new();
    session.save_project(&mut saved).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&saved).unwrap()["version"],
        2
    );
    let mut loaded = Session::default();
    loaded.load_project(saved.as_slice()).unwrap();
    assert_eq!(loaded.editor.timeline, session.editor.timeline);
    loaded.saved = false;
    assert!(loaded.load_project(saved.as_slice()).is_err());
    assert!(!loaded.start("minecraft:overworld"));
    let mut value: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    value["duration"] = 10.into();
    assert!(
        Session::default()
            .load_project(serde_json::to_vec(&value).unwrap().as_slice())
            .is_err()
    );
    let legacy=br#"{"version":1,"duration":0,"timeline":{"camera":[],"fov":[],"speed":[],"time_of_day":[],"markers":[]}}"#;
    Session::default().load_project(legacy.as_slice()).unwrap();
}

#[test]
fn malformed_identity_and_invalid_track_data_are_rejected() {
    let id = EntityId::from_u128(0x123456789abcdef00123456789abcdef);
    let text = String::from(id);
    assert_eq!(EntityId::try_from(text).unwrap(), id);
    assert!(EntityId::try_from(String::from("00000000-00000000-00000000-00000")).is_err());
    assert!(
        TrackData::Freeze(vec![key(
            0,
            FreezeKey {
                frozen: true,
                delay: 11
            }
        )])
        .validate()
        .is_err()
    );
    assert!(
        TrackData::Timelapse(vec![
            key(0, TimelapseKey { ticks: 10 }),
            key(20, TimelapseKey { ticks: 5 })
        ])
        .validate()
        .is_err()
    );
    assert!(TrackData::Speed(vec![key(0, f64::NAN)]).validate().is_err());
    let timeline = Timeline {
        tracks: vec![
            Track::new(1, TrackData::empty(TrackKind::Audio)),
            Track::new(1, TrackData::empty(TrackKind::Fov)),
        ],
        ..Timeline::default()
    };
    assert!(timeline.validate().is_err());
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/interpolation-java.json")).unwrap();
    let pins: serde_json::Value =
        serde_json::from_str(include_str!("../compatibility.json")).unwrap();
    assert_eq!(fixture["reference"]["revision"], pins["flashback_revision"]);
}
