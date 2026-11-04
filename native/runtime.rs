// Native game glue. Preview sessions belong to live windows; split-screen is disabled.
use super::Game;
use horizon_client::hud::HudBatch;
use horizon_client::net::flashback_net::{self as network, engine as horizon_flashback};
use horizon_flashback::replay::{CameraPose, Vec3};
use horizon_flashback::session::Session;
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Weak},
};
use std::{
    fs, io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use winit::window::Window;

type WindowSession = (Weak<Window>, Rc<RefCell<Session>>);
thread_local! {
    static SESSIONS: RefCell<Vec<WindowSession>> = const { RefCell::new(Vec::new()) };
}
fn supported(game: &Game<'_>) -> bool {
    game.native_window()
        && game.window.is_some()
        && !game.options.split_in_main_window
        && !game.split_window
}
pub(super) fn state(game: &Game<'_>) -> Rc<RefCell<Session>> {
    let Some(window) = game.window.as_ref() else {
        return Rc::new(RefCell::new(Session::default()));
    };
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        sessions.retain(|(owner, _)| owner.strong_count() > 0);
        let owner = Arc::downgrade(window);
        if let Some((_, session)) = sessions.iter().find(|(w, _)| w.ptr_eq(&owner)) {
            return Rc::clone(session);
        }
        let session = Rc::new(RefCell::new(Session::default()));
        sessions.push((owner, Rc::clone(&session)));
        session
    })
}

pub(super) fn tick(game: &mut Game<'_>) {
    if !supported(game) {
        return;
    }
    let requested = state(game).borrow_mut().requested_seek.take();
    if let Some(tick) = requested {
        let address = game
            .remote
            .as_ref()
            .map(|r| r.session.address.clone())
            .unwrap_or_default();
        if let Ok(Some((_, path))) = network::parse_address(&address)
            && let Err(error) =
                game.connect_server(&network::replay_address(&path, tick), "ReplayViewer")
        {
            state(game).borrow_mut().status = format!("Replay seek failed: {error}");
        }
    }
    let dimension = game
        .native_player()
        .map(|player| player.dimension.to_owned());
    state(game).borrow_mut().tick(0.05, dimension.as_deref());
    if let Some(progress) = game
        .remote
        .as_ref()
        .and_then(|r| network::progress(&r.session))
    {
        let storage = state(game);
        let mut session = storage.borrow_mut();
        session.playhead = progress.tick;
        session.playing = !progress.paused && !progress.ended;
        session.duration = progress
            .duration
            .unwrap_or(progress.tick.ceil() as u64)
            .max(session.duration);
    }
}

pub(super) fn unload(game: &Game<'_>) {
    if !supported(game) {
        return;
    }
    let storage = state(game);
    let mut session = storage.borrow_mut();
    if session.recording {
        session.stop();
    }
    session.preview = false;
    session.playing = false;
}

pub(super) fn key(game: &mut Game<'_>, key: &str) {
    if !supported(game) {
        return;
    }
    if key == game.native_mod_key("flashback", "world_record", "key.keyboard.f8") {
        let result = directory().and_then(|root| network::toggle_recording(&root));
        state(game).borrow_mut().status =
            result.unwrap_or_else(|e| format!("World recording failed: {e}"));
        return;
    }
    if key == game.native_mod_key("flashback", "world_open", "key.keyboard.f9")
        && game.native_player().is_none()
        && game.title.is_some()
        && game.remote.is_none()
        && game.integrated_starting.is_none()
    {
        let result = directory().and_then(|root| network::latest(&root));
        match result {
            Ok(path) => {
                if let Err(error) =
                    game.connect_server(&network::replay_address(&path, 0), "ReplayViewer")
                {
                    state(game).borrow_mut().status = format!("Replay open failed: {error}");
                }
            }
            Err(error) => state(game).borrow_mut().status = error,
        }
        return;
    }
    let editor_key = game.native_mod_key("flashback", "editor", "key.keyboard.f7");
    if key == editor_key {
        if game.native_mod_screen() == Some("flashback") {
            game.native_mod_screen_close("flashback");
            let storage = state(game);
            let mut session = storage.borrow_mut();
            session.preview = false;
            session.playing = false;
        } else {
            game.native_mod_screen_open("flashback");
        }
        return;
    }
    let open = game.native_mod_screen() == Some("flashback");
    let record_key = game.native_mod_key("flashback", "record", "key.keyboard.f6");
    if key == record_key && (game.native_playing() || open) {
        let dimension = game.native_player().map(|p| p.dimension.to_owned());
        let storage = state(game);
        let mut session = storage.borrow_mut();
        if session.recording {
            session.stop();
        } else if let Some(dimension) = dimension {
            session.start(&dimension);
        }
        return;
    }
    if !open {
        return;
    }
    if game
        .remote
        .as_ref()
        .is_some_and(|r| r.session.address.starts_with(network::PREFIX))
    {
        let address = game
            .remote
            .as_ref()
            .map(|r| r.session.address.clone())
            .unwrap_or_default();
        match key {
            "key.keyboard.space" => {
                if let Some(remote) = &game.remote {
                    network::toggle_pause(&remote.session);
                }
                return;
            }
            "key.keyboard.left" | "key.keyboard.right" | "key.keyboard.home" => {
                let current = game
                    .remote
                    .as_ref()
                    .and_then(|r| network::progress(&r.session))
                    .map_or(0.0, |p| p.tick);
                let tick = match key {
                    "key.keyboard.left" => (current - 1.0).max(0.0) as u64,
                    "key.keyboard.right" => (current as u64).saturating_add(1),
                    _ => 0,
                };
                if let Ok(Some((_, path))) = network::parse_address(&address)
                    && let Err(error) =
                        game.connect_server(&network::replay_address(&path, tick), "ReplayViewer")
                {
                    state(game).borrow_mut().status = format!("Replay seek failed: {error}");
                }
                return;
            }
            "key.keyboard.minus" | "key.keyboard.equal" => {
                if let Some(remote) = &game.remote {
                    network::set_speed(
                        &remote.session,
                        if key == "key.keyboard.minus" {
                            0.5
                        } else {
                            2.0
                        },
                    );
                }
                return;
            }
            _ => {}
        }
    }
    let storage = state(game);
    let mut session = storage.borrow_mut();
    match key {
        "key.keyboard.space" => session.toggle_play(),
        "key.keyboard.left" => {
            let tick = session.playhead - 1.0;
            session.seek(tick);
        }
        "key.keyboard.right" => {
            let tick = session.playhead + 1.0;
            session.seek(tick);
        }
        "key.keyboard.home" => session.seek(0.0),
        "key.keyboard.end" => {
            let tick = session.duration as f64;
            session.seek(tick);
        }
        "key.keyboard.k" => session.add_camera_key(),
        "key.keyboard.z" => {
            if session.editor.undo() {
                session.saved = false;
            }
        }
        "key.keyboard.y" => {
            if session.editor.redo() {
                session.saved = false;
            }
        }
        "key.keyboard.delete" => {
            let tick = session.playhead.round() as u64;
            if session.editor.delete_camera_key(tick).is_ok() {
                session.saved = false;
            }
        }
        "key.keyboard.n" => {
            session.new_take();
        }
        "key.keyboard.s" => save(&mut session),
        "key.keyboard.o" => load(&mut session),
        _ => {}
    }
}

pub(super) fn camera(
    game: &mut Game<'_>,
    original: Option<(glam::DVec3, f32, f32)>,
    partial: f32,
) -> Option<(glam::DVec3, f32, f32)> {
    if !supported(game) {
        return original;
    }
    let storage = state(game);
    let mut session = storage.borrow_mut();
    // Closing with Escape must also release the detached camera.
    if game.native_mod_screen() != Some("flashback") {
        session.preview = false;
        session.playing = false;
    }
    let partial = if partial.is_finite() {
        partial.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let render_tick = game
        .remote
        .as_ref()
        .and_then(|r| network::progress(&r.session))
        .map_or_else(
            || {
                session.playhead
                    + if session.playing {
                        f64::from(partial)
                            * session.speed
                            * session
                                .editor
                                .timeline
                                .speed_at(session.playhead)
                                .unwrap_or(1.0)
                    } else {
                        0.0
                    }
            },
            |p| p.tick,
        );
    session.render_tick = Some(render_tick);
    session
        .camera_at(render_tick, |id| game.flashback_target_pose(id, partial))
        .map(|pose| {
            (
                glam::DVec3::new(pose.position.x, pose.position.y, pose.position.z),
                pose.yaw as f32,
                pose.pitch as f32,
            )
        })
        .or(original)
}

pub(super) fn fov(game: &Game<'_>, position: [f64; 3], degrees: &mut f32) {
    if !supported(game) {
        return;
    }
    let storage = state(game);
    let mut session = storage.borrow_mut();
    session.observe(CameraPose {
        position: Vec3 {
            x: position[0],
            y: position[1],
            z: position[2],
        },
        yaw: game.view.y_rot.into(),
        pitch: game.view.x_rot.into(),
        roll: 0.0,
        fov: (*degrees).into(),
    });
    if session.preview {
        let tick = session.render_tick.unwrap_or(session.playhead);
        if let Some(fov) = session.editor.timeline.fov_at(tick) {
            *degrees = fov as f32;
        } else if let Some(camera) = session.camera_at(tick, |_| None) {
            *degrees = camera.fov as f32;
        }
    }
}

fn directory() -> Result<PathBuf, String> {
    let directory = Game::native_config_dir()?.join("flashback");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory)
}

fn save(session: &mut Session) {
    if session.recording {
        session.stop();
    }
    let result = (|| -> Result<PathBuf, String> {
        let root = directory()?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = root.join(format!("camera-{stamp}.json"));
        let project_part = path.with_extension("json.part");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&project_part)
            .map_err(|e| e.to_string())?;
        session.save_project(&mut file)?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if let Some(replay) = &session.replay {
            let replay_part = path.with_extension("hfr.part");
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&replay_part)
                .map_err(|e| e.to_string())?;
            replay.write_to(&mut file).map_err(|e| format!("{e:?}"))?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            fs::rename(replay_part, path.with_extension("hfr")).map_err(|e| e.to_string())?;
        }
        // Publish the project last: the loader ignores incomplete .part files.
        fs::rename(project_part, &path).map_err(|e| e.to_string())?;
        Ok(path)
    })();
    session.status = match result {
        Ok(path) => {
            session.saved = true;
            format!("Saved {}", path.display())
        }
        Err(error) => format!("Save failed: {error}"),
    };
}

fn load(session: &mut Session) {
    let result = (|| -> Result<PathBuf, String> {
        let root = directory()?;
        let mut projects = fs::read_dir(root)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
            .map(|entry| entry.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect::<Vec<_>>();
        projects.sort();
        let path = projects.last().ok_or("No saved camera projects")?.clone();
        let replay_path = path.with_extension("hfr");
        let replay = match fs::File::open(replay_path) {
            Ok(file) => Some(
                horizon_flashback::replay::Replay::read_from(file).map_err(|e| format!("{e:?}"))?,
            ),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        session.load_project(fs::File::open(&path).map_err(|e| e.to_string())?)?;
        session.replay = replay;
        Ok(path)
    })();
    session.status = match result {
        Ok(path) => format!("Loaded {}", path.display()),
        Err(error) => format!("Load failed: {error}"),
    };
}

pub(super) fn screen(game: &Game<'_>, batch: &mut HudBatch) {
    if !supported(game) || game.native_mod_screen() != Some("flashback") {
        return;
    }
    let storage = state(game);
    let session = storage.borrow();
    let (w, h) = (batch.width, batch.height);
    game.native_fill(batch, [0.0, 0.0, w, 37.0], 0xf0242429);
    game.native_text(
        batch,
        "FLASHBACK RUST / CAMERA PROTOTYPE",
        [8.0, 6.0],
        0xffffff,
        false,
    );
    game.native_text(
        batch,
        "F6 Camera   F7 Close   F8 World REC   Space Play",
        [8.0, 22.0],
        0xbcbcc8,
        false,
    );
    let top = (h - 100.0).max(40.0);
    game.native_fill(batch, [0.0, top, w, h - top], 0xf0222226);
    let button_width = ((w - 14.0) / 6.0).max(1.0);
    for (i, title) in ["Play", "Key", "Undo", "Redo", "Save", "Load"]
        .iter()
        .enumerate()
    {
        let x = 7.0 + i as f32 * button_width;
        game.native_fill(batch, [x, top + 5.0, button_width - 4.0, 17.0], 0xff3a3a42);
        game.native_text(batch, title, [x + 3.0, top + 10.0], 0xffffff, false);
    }
    let left = 60.0;
    let width = (w - left - 10.0).max(1.0);
    game.native_text(batch, "Camera", [5.0, top + 39.0], 0xdddde5, false);
    game.native_fill(batch, [left, top + 30.0, width, 25.0], 0xff303037);
    let duration = session.duration.max(1) as f32;
    for key in &session.editor.timeline.camera {
        let x = left + key.tick as f32 / duration * width;
        game.native_fill(batch, [x - 2.0, top + 35.0, 4.0, 14.0], 0xffffb347);
    }
    let x = left + session.playhead as f32 / duration * width;
    game.native_fill(batch, [x, top + 27.0, 1.0, 32.0], 0xff68aaff);
    let label = format!(
        "Tick {:.1} / {}   {}",
        session.playhead, session.duration, session.status
    );
    game.native_text(batch, &label, [8.0, top + 65.0], 0xbcbcc8, false);
    let network_status = network::recording_status();
    let footer = if network_status.is_empty() {
        "Live testing unverified. F9 opens a world tape from title."
    } else {
        &network_status
    };
    game.native_text(batch, footer, [8.0, top + 81.0], 0xffb347, false);
}

pub(super) fn click(game: &Game<'_>, mouse: [f32; 2], button: i32) -> bool {
    if !supported(game) || game.native_mod_screen() != Some("flashback") || button != 0 {
        return false;
    }
    let Some(renderer) = game.renderer.as_ref() else {
        return false;
    };
    let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
    let top = (batch.height - 100.0).max(40.0);
    let [x, y] = mouse;
    let storage = state(game);
    let mut session = storage.borrow_mut();
    if (top + 5.0..top + 22.0).contains(&y) && x >= 7.0 {
        match ((x - 7.0) / ((batch.width - 14.0) / 6.0).max(1.0)) as usize {
            0 => {
                if let Some(remote) = &game.remote
                    && remote.session.address.starts_with(network::PREFIX)
                {
                    network::toggle_pause(&remote.session);
                } else {
                    session.toggle_play();
                }
            }
            1 => session.add_camera_key(),
            2 => {
                if session.editor.undo() {
                    session.saved = false;
                }
            }
            3 => {
                if session.editor.redo() {
                    session.saved = false;
                }
            }
            4 => save(&mut session),
            5 => load(&mut session),
            _ => {}
        }
    } else if (top + 30.0..top + 55.0).contains(&y) && x >= 60.0 {
        let tick = ((x - 60.0) / (batch.width - 70.0).max(1.0)) as f64 * session.duration as f64;
        if game
            .remote
            .as_ref()
            .is_some_and(|r| r.session.address.starts_with(network::PREFIX))
        {
            session.requested_seek = Some(tick.max(0.0) as u64);
        } else {
            session.seek(tick);
        }
    }
    true
}
