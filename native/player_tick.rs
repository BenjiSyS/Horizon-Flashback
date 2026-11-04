fn hook(game: &mut Game<'_>) {
    crate::game::flashback_player::tick(game);
}
