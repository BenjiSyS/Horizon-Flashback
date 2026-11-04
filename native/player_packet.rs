fn hook(game: &mut Game<'_>, packet: &C) -> core::ops::ControlFlow<Result<()>> {
    if matches!(packet, C::Login(_) | C::Respawn(_) | C::PlayerPosition(_))
        && let Some(remote) = &game.remote
    {
        // A correction/transition supersedes the last sample until the next
        // recorded player tick. Never restore a pose from the previous level.
        horizon_client::net::flashback_net::clear_local_player(&remote.session);
    }
    if let C::CustomPayload(payload) = packet
        && payload.channel == horizon_client::net::flashback_net::PLAYER_CHANNEL
    {
        // Consume this reserved channel even on a live connection. A server
        // cannot use it to control the replay player's local state.
        if let Some(remote) = &game.remote {
            let _ = horizon_client::net::flashback_net::apply_local_player(
                &remote.session,
                &payload.data,
            );
        }
        return core::ops::ControlFlow::Break(Ok(()));
    }
    core::ops::ControlFlow::Continue(())
}
