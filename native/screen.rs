fn hook(game: &Game<'_>, batch: &mut HudBatch) {
    crate::game::flashback_native::screen(game, batch);
}
