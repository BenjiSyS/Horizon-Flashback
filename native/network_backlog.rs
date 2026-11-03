fn hook(game: &mut Game<'_>) {
    if let Some(remote) = &game.remote {
        horizon_client::net::flashback_net::host_backlog(
            &remote.session,
            remote.backlog.len() > 32,
        );
    }
}
