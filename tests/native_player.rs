//! Actual player hooks against a host double; not a live Horizon gameplay test.
extern crate self as horizon_client;
use horizon_flashback::actor::LocalPlayerFrame;
use std::{cell::RefCell, marker::PhantomData, ops::ControlFlow};
type Result<T> = std::result::Result<T, String>;

pub mod net {
    use super::*;
    #[derive(Default)]
    pub struct Session {
        pub address: String,
        pub replay: bool,
        pub paused: bool,
        pub player: RefCell<Option<LocalPlayerFrame>>,
        pub recorded: RefCell<Vec<LocalPlayerFrame>>,
    }
    pub mod flashback_net {
        use super::*;
        pub use horizon_flashback::actor::LocalPlayerFrame;
        pub const PREFIX: &str = "flashback-replay:";
        pub const PLAYER_CHANNEL: &str = "flashback:local_player";
        pub struct Progress {
            pub paused: bool,
            pub ended: bool,
        }
        pub fn progress(s: &Session) -> Option<Progress> {
            s.replay.then_some(Progress {
                paused: s.paused,
                ended: false,
            })
        }
        pub fn local_player(s: &Session) -> Option<LocalPlayerFrame> {
            s.player.borrow().clone()
        }
        pub fn clear_local_player(s: &Session) {
            s.player.replace(None);
        }
        pub fn apply_local_player(s: &Session, bytes: &[u8]) -> Result<()> {
            if !s.replay {
                return Err("Outside replay".into());
            }
            s.player.replace(Some(LocalPlayerFrame::decode(bytes)?));
            Ok(())
        }
        pub fn record_local_player(s: &Session, frame: &LocalPlayerFrame) {
            s.recorded.borrow_mut().push(frame.clone());
        }
    }
}
#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Vector {
    x: f64,
    y: f64,
    z: f64,
}
#[derive(Default)]
struct Ground {
    on_ground: bool,
}
#[derive(Default)]
struct Player {
    position: Vector,
    velocity: Vector,
    ground: Ground,
    yaw: f32,
    pitch: f32,
    head_yaw: f32,
    sprinting: bool,
    shift_key_down: bool,
    local_crouching: bool,
    swimming: bool,
    flying: bool,
    fall_flying: bool,
}
struct Remote {
    session: net::Session,
}
struct Game<'a> {
    remote: Option<Remote>,
    player: Option<Player>,
    previous_position: Vector,
    yaw: f32,
    pitch: f32,
    selected: usize,
    lifetime: PhantomData<&'a ()>,
}
struct Payload {
    channel: String,
    data: Vec<u8>,
}
enum C {
    Login(()),
    Respawn(()),
    PlayerPosition(()),
    CustomPayload(Payload),
}
#[path = "../native/player.rs"]
mod player;
mod packet {
    use super::*;
    include!("../native/player_packet.rs");
    pub(super) fn run(g: &mut Game<'_>, p: &C) -> ControlFlow<Result<()>> {
        hook(g, p)
    }
}
mod pause {
    use super::*;
    include!("../native/replay_pause.rs");
    pub(super) fn run(g: &mut Game<'_>) -> ControlFlow<Result<()>> {
        hook(g)
    }
}
mod input {
    use super::*;
    include!("../native/replay_input.rs");
    pub(super) fn run(g: &mut Game<'_>) -> ControlFlow<()> {
        hook(g)
    }
}
fn frame() -> LocalPlayerFrame {
    LocalPlayerFrame {
        position: [5.0, 70.0, -8.0],
        previous_position: [4.8, 70.1, -8.3],
        velocity: [0.2, -0.1, 0.3],
        yaw: 90.0,
        pitch: 20.0,
        head_yaw: 85.0,
        on_ground: false,
        sprinting: true,
        crouching: true,
        swimming: false,
        flying: false,
        fall_flying: true,
        selected: 4,
    }
}
fn game(replay: bool) -> Game<'static> {
    Game {
        remote: Some(Remote {
            session: net::Session {
                replay,
                address: if replay {
                    "flashback-replay:0:take.hfnet"
                } else {
                    "localhost"
                }
                .into(),
                ..Default::default()
            },
        }),
        player: Some(Player::default()),
        previous_position: Vector::default(),
        yaw: 0.0,
        pitch: 0.0,
        selected: 0,
        lifetime: PhantomData,
    }
}

#[test]
fn replay_consumption_restores_pose_after_prediction_without_recapture() {
    let mut game = game(true);
    let sample = frame();
    let payload = C::CustomPayload(Payload {
        channel: net::flashback_net::PLAYER_CHANNEL.into(),
        data: serde_json::to_vec(&sample).unwrap(),
    });
    assert!(matches!(
        packet::run(&mut game, &payload),
        ControlFlow::Break(Ok(()))
    ));
    game.player.as_mut().unwrap().position.x = 999.0; // Simulated prediction drift.
    player::tick(&mut game);
    let p = game.player.as_ref().unwrap();
    assert_eq!([p.position.x, p.position.y, p.position.z], sample.position);
    assert_eq!([p.velocity.x, p.velocity.y, p.velocity.z], sample.velocity);
    assert_eq!(
        [
            game.previous_position.x,
            game.previous_position.y,
            game.previous_position.z
        ],
        sample.previous_position
    );
    assert_eq!(game.selected, sample.selected);
    assert_eq!((game.yaw, game.pitch), (sample.yaw, sample.pitch));
    assert!(p.local_crouching && p.fall_flying);
    assert!(
        game.remote
            .as_ref()
            .unwrap()
            .session
            .recorded
            .borrow()
            .is_empty()
    );
    for transition in [C::Login(()), C::Respawn(()), C::PlayerPosition(())] {
        let _ = packet::run(&mut game, &payload);
        assert!(packet::run(&mut game, &transition).is_continue());
        assert!(
            game.remote
                .as_ref()
                .unwrap()
                .session
                .player
                .borrow()
                .is_none()
        );
    }
}

#[test]
fn live_payload_cannot_control_player_and_pause_keeps_native_outer_tick_available() {
    let mut game = game(false);
    let payload = C::CustomPayload(Payload {
        channel: net::flashback_net::PLAYER_CHANNEL.into(),
        data: serde_json::to_vec(&frame()).unwrap(),
    });
    let _ = packet::run(&mut game, &payload);
    assert!(
        game.remote
            .as_ref()
            .unwrap()
            .session
            .player
            .borrow()
            .is_none()
    );
    assert!(input::run(&mut game).is_continue());
    assert!(pause::run(&mut game).is_continue());
    player::tick(&mut game);
    assert_eq!(
        game.remote
            .as_ref()
            .unwrap()
            .session
            .recorded
            .borrow()
            .len(),
        1
    );
    game.remote.as_mut().unwrap().session.replay = true;
    game.remote.as_mut().unwrap().session.address = "flashback-replay:0:take.hfnet".into();
    game.remote.as_mut().unwrap().session.paused = true;
    assert!(input::run(&mut game).is_break());
    assert!(matches!(pause::run(&mut game), ControlFlow::Break(Ok(()))));
    // The outer tick remains callable while only the client-level helper cancels.
    player::tick(&mut game);
    assert_eq!(
        game.remote
            .as_ref()
            .unwrap()
            .session
            .recorded
            .borrow()
            .len(),
        1
    );
}
