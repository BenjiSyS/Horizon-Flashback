fn hook(game: &mut Game<'_>) -> core::ops::ControlFlow<Result<()>> {
    if game
        .remote
        .as_ref()
        .and_then(|r| horizon_client::net::flashback_net::progress(&r.session))
        .is_some_and(|p| p.paused || p.ended)
    {
        core::ops::ControlFlow::Break(Ok(()))
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
