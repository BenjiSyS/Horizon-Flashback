fn hook(game: &mut Game<'_>) -> core::ops::ControlFlow<()> {
    if game.remote.as_ref().is_some_and(|r| {
        r.session
            .address
            .starts_with(horizon_client::net::flashback_net::PREFIX)
    }) {
        core::ops::ControlFlow::Break(())
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
