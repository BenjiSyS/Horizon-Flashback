// Host player state stays separate from the portable replay engine.
use super::Game;
use horizon_client::net::flashback_net::{self as network, LocalPlayerFrame};

pub(super) fn tick(game: &mut Game<'_>) {
    let Some(remote) = &game.remote else { return };
    if let Some(frame) = network::local_player(&remote.session) {
        let Some(player) = game.player.as_mut() else {
            return;
        };
        player.position.x = frame.position[0];
        player.position.y = frame.position[1];
        player.position.z = frame.position[2];
        player.velocity.x = frame.velocity[0];
        player.velocity.y = frame.velocity[1];
        player.velocity.z = frame.velocity[2];
        player.yaw = frame.yaw;
        player.pitch = frame.pitch;
        player.head_yaw = frame.head_yaw;
        player.ground.on_ground = frame.on_ground;
        player.sprinting = frame.sprinting;
        player.shift_key_down = frame.crouching;
        player.local_crouching = frame.crouching;
        player.swimming = frame.swimming;
        player.flying = frame.flying;
        player.fall_flying = frame.fall_flying;
        // Preserve the recorded interpolation endpoints, not prediction drift.
        game.previous_position.x = frame.previous_position[0];
        game.previous_position.y = frame.previous_position[1];
        game.previous_position.z = frame.previous_position[2];
        game.yaw = frame.yaw;
        game.pitch = frame.pitch;
        game.selected = frame.selected;
    } else if network::progress(&remote.session).is_none() {
        let Some(player) = game.player.as_ref() else {
            return;
        };
        network::record_local_player(
            &remote.session,
            &LocalPlayerFrame {
                position: [player.position.x, player.position.y, player.position.z],
                previous_position: [
                    game.previous_position.x,
                    game.previous_position.y,
                    game.previous_position.z,
                ],
                velocity: [player.velocity.x, player.velocity.y, player.velocity.z],
                yaw: game.yaw,
                pitch: game.pitch,
                head_yaw: player.head_yaw,
                on_ground: player.ground.on_ground,
                sprinting: player.sprinting,
                crouching: player.shift_key_down,
                swimming: player.swimming,
                flying: player.flying,
                fall_flying: player.fall_flying,
                selected: game.selected,
            },
        );
    }
}
