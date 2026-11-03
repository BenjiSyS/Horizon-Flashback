//! Adapter behavior against a minimal host double. This is not a Horizon build or live test.
extern crate self as glam;
extern crate self as horizon_client;
extern crate self as winit;
use std::sync::Arc;

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
    view: View,
    window: Option<Arc<window::Window>>,
    options: Options,
    split_window: bool,
    renderer: Option<Renderer>,
    open: bool,
    dimension: &'a str,
}
impl Game<'_> {
    fn new() -> Self {
        Self {
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
        }
    }
    fn native_window(&self) -> bool {
        true
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
    assert_eq!(flashback_native::camera(&mut game, first), first);
    flashback_native::fov(&game, [1.0, 65.0, 2.0], &mut 70.0);
    flashback_native::tick(&mut game);
    flashback_native::camera(&mut game, second);
    game.view.y_rot = 20.0;
    flashback_native::fov(&game, [5.0, 65.0, 2.0], &mut 70.0);
    flashback_native::key(&mut game, "key.keyboard.f6");
    flashback_native::key(&mut game, "key.keyboard.f7");
    flashback_native::key(&mut game, "key.keyboard.home");
    assert_eq!(flashback_native::camera(&mut game, second), first);
    let mut degrees = 85.0;
    flashback_native::fov(&game, [1.0, 65.0, 2.0], &mut degrees);
    assert_eq!(degrees, 70.0);
    game.open = false;
    assert_eq!(flashback_native::camera(&mut game, second), second);
    flashback_native::unload(&game);
}

#[test]
fn separate_windows_do_not_share_recordings_and_split_screen_is_disabled() {
    let mut first = Game::new();
    let mut second = Game::new();
    flashback_native::key(&mut first, "key.keyboard.f6");
    flashback_native::camera(&mut first, Some((DVec3::new(1.0, 2.0, 3.0), 0.0, 0.0)));
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
