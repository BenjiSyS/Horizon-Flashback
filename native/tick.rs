fn hook(game: &mut Game<'_>) {
    crate::game::flashback_native::tick(game);
}
