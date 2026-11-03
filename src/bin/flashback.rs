//! Small command-line harness for inspecting the independent replay core.
use horizon_flashback::replay::{Delta, Marker, Replay, SceneState, TickEvent};
use std::{
    env,
    fs::{File, OpenOptions},
    process::ExitCode,
};

fn run() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.as_slice() {
        [command, path] if command == "demo" => {
            let mut replay = Replay::new(
                0,
                100,
                SceneState {
                    dimension: "minecraft:overworld".into(),
                    ..Default::default()
                },
            );
            for tick in 1..=400 {
                let mut camera = replay.header.initial.camera;
                camera.position.x = tick as f64 * 0.05;
                camera.position.y = 72.0;
                camera.yaw = (tick as f64 * 0.5) % 360.0;
                replay
                    .push(TickEvent {
                        tick,
                        delta: Delta::Camera(camera),
                    })
                    .map_err(|e| format!("{e:?}"))?;
                replay
                    .push(TickEvent {
                        tick,
                        delta: Delta::TimeOfDay(6000 + tick),
                    })
                    .map_err(|e| format!("{e:?}"))?;
            }
            replay.add_marker(Marker {
                tick: 200,
                color: 0xffffb347,
                position: None,
                dimension: None,
                description: Some("Halfway through synthetic camera take".into()),
            });
            replay.rebuild_snapshots();
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            replay.write_to(&mut file).map_err(|e| format!("{e:?}"))?;
            file.sync_all().map_err(|e| e.to_string())?;
            println!("Created synthetic core demo: {path} (400 ticks, 800 events)");
        }
        [command, path] if command == "inspect" => {
            let replay = Replay::read_from(File::open(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("{e:?}"))?;
            let summary = serde_json::json!({
                "format": replay.header.version,
                "initial_tick": replay.header.initial_tick,
                "last_event_tick": replay.events.last().map(|e| e.tick),
                "events": replay.events.len(), "snapshots": replay.snapshots.len(),
                "markers": replay.markers,
                "initial_dimension": replay.header.initial.dimension,
                "java_flashback_compatible": false,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
            );
        }
        [command, path] if command == "tape-inspect" => {
            let mut tape = horizon_flashback::tape::TapeReader::new(
                File::open(path).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let mut counts = std::collections::BTreeMap::<u8, u64>::new();
            let mut last_tick = 0;
            let mut payload_bytes = 0u64;
            while let Some(record) = tape.next_record().map_err(|e| e.to_string())? {
                *counts.entry(record.kind).or_default() += 1;
                last_tick = record.tick;
                payload_bytes += record.payload.len() as u64;
            }
            let summary = serde_json::json!({"metadata": tape.metadata, "events_by_kind": counts,
                "last_tick": last_tick, "payload_bytes": payload_bytes, "recovered_incomplete_tail": tape.recovered_tail});
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
            );
        }
        [command, path, tick] if command == "seek" => {
            let tick = tick
                .parse::<u64>()
                .map_err(|_| "Tick must be an unsigned integer")?;
            let replay = Replay::read_from(File::open(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("{e:?}"))?;
            let state = replay.state_at(tick).map_err(|e| format!("{e:?}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?
            );
        }
        _ => {
            return Err(
                "Usage: flashback demo NEW_FILE.hfr | inspect FILE.hfr | seek FILE.hfr TICK | tape-inspect FILE.hfnet".into(),
            );
        }
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
