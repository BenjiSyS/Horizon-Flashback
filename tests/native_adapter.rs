//! Adapter behavior against a minimal host double. This is not a Horizon build or live test.
extern crate self as glam;
extern crate self as horizon_client;
extern crate self as winit;
use std::sync::Arc;
pub mod net {
    #[derive(Default)]
    pub struct Session {
        pub address: String,
        pub tick: Option<f64>,
    }
    pub mod flashback_net {
        pub use horizon_flashback as engine;
        pub const PREFIX: &str = "flashback-replay:";
        pub struct Progress {
            pub tick: f64,
            pub paused: bool,
            pub ended: bool,
            pub duration: Option<u64>,
        }
        pub fn progress(session: &super::Session) -> Option<Progress> {
            session.tick.map(|tick| Progress {
                tick,
                paused: true,
                ended: false,
                duration: None,
            })
        }
        pub fn recording_status() -> String {
            String::new()
        }
        pub fn toggle_recording(_: &std::path::Path) -> Result<String, String> {
            Err("No recording in host double".into())
        }
        pub fn latest(_: &std::path::Path) -> Result<std::path::PathBuf, String> {
            Err("No tapes in host double".into())
        }
        pub fn replay_address(path: &std::path::Path, tick: u64) -> String {
            format!("{PREFIX}{tick}:{}", path.display())
        }
        pub fn parse_address(_: &str) -> Result<Option<(u64, std::path::PathBuf)>, String> {
            Ok(None)
        }
        pub fn toggle_pause(_: &super::Session) -> bool {
            false
        }
        pub fn set_speed(_: &super::Session, _: f64) -> bool {
            false
        }
    }
}

pub mod window {
    pub struct Window;
}
pub mod hud {
    #[derive(Default)]
    pub struct HudBatch {
        pub width: f32,
        pub height: f32,
        pub text: Vec<String>,
    }
    impl HudBatch {
        pub fn new(size: [u32; 2]) -> Self {
            Self {
                width: size[0] as f32,
                height: size[1] as f32,
                ..Default::default()
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl DVec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}
struct Options {
    split_in_main_window: bool,
}
struct Config {
    width: u32,
    height: u32,
}
struct Renderer {
    config: Config,
}
struct Player {
    dimension: &'static str,
}
#[derive(Default)]
struct View {
    y_rot: f32,
    x_rot: f32,
}
struct Game<'a> {
    remote: Option<Remote>,
    title: Option<()>,
    integrated_starting: Option<()>,
    view: View,
    window: Option<Arc<window::Window>>,
    options: Options,
    split_window: bool,
    renderer: Option<Renderer>,
    open: bool,
    dimension: &'a str,
    target: Option<(
        horizon_flashback::cinematic::EntityId,
        horizon_flashback::cinematic::TargetPose,
    )>,
}
struct Remote {
    session: net::Session,
}
impl Game<'_> {
    fn new() -> Self {
        Self {
            remote: None,
            title: None,
            integrated_starting: None,
            view: View::default(),
            window: Some(Arc::new(window::Window)),
            options: Options {
                split_in_main_window: false,
            },
            split_window: false,
            renderer: Some(Renderer {
                config: Config {
                    width: 640,
                    height: 360,
                },
            }),
            open: false,
            dimension: "minecraft:overworld",
            target: None,
        }
    }
    fn native_window(&self) -> bool {
        true
    }
    fn flashback_target_pose(
        &self,
        id: horizon_flashback::cinematic::EntityId,
        _: f32,
    ) -> Option<horizon_flashback::cinematic::TargetPose> {
        self.target
            .filter(|(target, _)| *target == id)
            .map(|(_, pose)| pose)
    }
    fn connect_server(&mut self, _: &str, _: &str) -> Result<(), String> {
        Err("No network in host double".into())
    }
    fn native_player(&self) -> Option<Player> {
        Some(Player {
            dimension: "minecraft:overworld",
        })
    }
    fn native_mod_key(&self, _: &str, _: &str, default: &str) -> String {
        default.into()
    }
    fn native_mod_screen(&self) -> Option<&str> {
        self.open.then_some("flashback")
    }
    fn native_mod_screen_open(&mut self, _: &str) -> bool {
        self.open = true;
        true
    }
    fn native_mod_screen_close(&mut self, _: &str) {
        self.open = false;
    }
    fn native_playing(&self) -> bool {
        !self.open && !self.dimension.is_empty()
    }
    fn native_config_dir() -> Result<std::path::PathBuf, String> {
        Err("Disk access disabled in host double".into())
    }
    fn native_fill(&self, _: &mut hud::HudBatch, _: [f32; 4], _: u32) {}
    fn native_text(&self, batch: &mut hud::HudBatch, text: &str, _: [f32; 2], _: u32, _: bool) {
        batch.text.push(text.into());
    }
}

#[path = "../native/runtime.rs"]
mod flashback_native;

#[test]
fn detached_preview_changes_render_output_and_escape_restores_it() {
    let mut game = Game::new();
    let first = Some((DVec3::new(1.0, 65.0, 2.0), 0.0, 0.0));
    let second = Some((DVec3::new(5.0, 65.0, 2.0), 20.0, 0.0));
    flashback_native::key(&mut game, "key.keyboard.f6");
    assert_eq!(flashback_native::camera(&mut game, first, 0.0), first);
    flashback_native::fov(&game, [1.0, 65.0, 2.0], &mut 70.0);
    flashback_native::tick(&mut game);
    flashback_native::camera(&mut game, second, 0.0);
    game.view.y_rot = 20.0;
    flashback_native::fov(&game, [5.0, 65.0, 2.0], &mut 70.0);
    flashback_native::key(&mut game, "key.keyboard.f6");
    flashback_native::key(&mut game, "key.keyboard.f7");
    flashback_native::key(&mut game, "key.keyboard.home");
    assert_eq!(flashback_native::camera(&mut game, second, 0.0), first);
    let mut degrees = 85.0;
    flashback_native::fov(&game, [1.0, 65.0, 2.0], &mut degrees);
    assert_eq!(degrees, 70.0);
    game.open = false;
    assert_eq!(flashback_native::camera(&mut game, second, 0.0), second);
    flashback_native::unload(&game);
}

#[test]
fn separate_windows_do_not_share_recordings_and_split_screen_is_disabled() {
    let mut first = Game::new();
    let mut second = Game::new();
    flashback_native::key(&mut first, "key.keyboard.f6");
    flashback_native::camera(&mut first, Some((DVec3::new(1.0, 2.0, 3.0), 0.0, 0.0)), 0.0);
    flashback_native::key(&mut second, "key.keyboard.f7");
    let mut batch = hud::HudBatch::new([640, 360]);
    flashback_native::screen(&second, &mut batch);
    assert!(batch.text.iter().any(|line| line.contains("Tick 0.0 / 0")));
    second.split_window = true;
    second.open = false;
    flashback_native::key(&mut second, "key.keyboard.f7");
    assert!(!second.open);
    assert!(!flashback_native::click(&second, [100.0, 300.0], 0));
}

#[test]
fn toolbar_seek_click_and_save_failure_are_visible() {
    let mut game = Game::new();
    flashback_native::key(&mut game, "key.keyboard.f7");
    assert!(flashback_native::click(&game, [8.0, 267.0], 0));
    flashback_native::key(&mut game, "key.keyboard.s");
    let mut batch = hud::HudBatch::new([640, 360]);
    flashback_native::screen(&game, &mut batch);
    assert!(
        batch
            .text
            .iter()
            .any(|line| line.contains("Save failed: Disk access disabled"))
    );
}

#[test]
fn replay_step_at_extreme_timestamp_does_not_overflow() {
    let mut game = Game::new();
    game.open = true;
    game.remote = Some(Remote {
        session: net::Session {
            address: "flashback-replay:0:test.hfnet".into(),
            tick: Some(f64::MAX),
        },
    });
    flashback_native::key(&mut game, "key.keyboard.right");
}

#[test]
fn cinematic_render_uses_partial_ticks_and_shares_its_clock_with_fov() {
    use horizon_flashback::{
        cinematic::*,
        timeline::{Interpolation, Key, Vec3},
    };
    let mut game = Game::new();
    game.open = true;
    let storage = flashback_native::state(&game);
    {
        let mut session = storage.borrow_mut();
        session.preview = true;
        session.playing = true;
        session.playhead = 5.0;
        session.duration = 10;
        session.editor.timeline.tracks = vec![
            Track::new(
                1,
                TrackData::Orbit(vec![
                    Key {
                        tick: 0,
                        value: OrbitKey {
                            center: Vec3::default(),
                            distance: 10.0,
                            yaw: 0.0,
                            pitch: 0.0,
                        },
                        interpolation: Interpolation::Linear,
                    },
                    Key {
                        tick: 10,
                        value: OrbitKey {
                            center: Vec3::default(),
                            distance: 10.0,
                            yaw: 180.0,
                            pitch: 0.0,
                        },
                        interpolation: Interpolation::Linear,
                    },
                ]),
            ),
            Track::new(
                2,
                TrackData::Fov(vec![
                    Key {
                        tick: 0,
                        value: 60.0,
                        interpolation: Interpolation::Linear,
                    },
                    Key {
                        tick: 10,
                        value: 120.0,
                        interpolation: Interpolation::Linear,
                    },
                ]),
            ),
        ];
    }
    let camera = flashback_native::camera(&mut game, None, 0.5).unwrap();
    assert!((camera.1 - 99.0).abs() < 1e-6);
    let mut fov = 70.0;
    flashback_native::fov(&game, [0.0; 3], &mut fov);
    let expected = storage.borrow().editor.timeline.fov_at(5.5).unwrap();
    assert!((f64::from(fov) - expected).abs() < 1e-5);
    let id = EntityId::from_u128(42);
    game.target = Some((
        id,
        TargetPose {
            position: Vec3 {
                x: 10.0,
                y: 64.0,
                z: 20.0,
            },
            height: 2.0,
            eye_height: 1.5,
            yaw: 0.0,
            body_yaw: 0.0,
            head_yaw: 0.0,
            pitch: 0.0,
        },
    ));
    {
        let mut session = storage.borrow_mut();
        session.playing = false;
        session.editor.timeline.tracks.push(Track::new(
            3,
            TrackData::Tracking(vec![Key {
                tick: 0,
                interpolation: Interpolation::Step,
                value: TrackingKey {
                    target: id,
                    body_part: BodyPart::Head,
                    yaw_offset: 0.0,
                    pitch_offset: 0.0,
                    position_offset: Vec3::default(),
                    view_offset: Vec3::default(),
                    roll: 0.0,
                },
            }]),
        ));
    }
    let camera = flashback_native::camera(&mut game, None, 0.75).unwrap();
    assert_eq!(camera.0, DVec3::new(10.0, 65.5, 20.0));
    {
        let mut session = storage.borrow_mut();
        session.editor.timeline.tracks = vec![Track::new(
            1,
            TrackData::Fov(vec![Key {
                tick: 0,
                value: 90.0,
                interpolation: Interpolation::Step,
            }]),
        )];
    }
    assert!(flashback_native::camera(&mut game, None, 0.5).is_none());
    flashback_native::fov(&game, [0.0; 3], &mut fov);
    assert!((fov - 90.0).abs() < 1e-5);
}
