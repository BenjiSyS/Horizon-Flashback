fn hook(game: &mut Game<'_>, key: &str, pressed: bool) {
    if pressed {
        crate::game::flashback_native::key(game, key);
    }
}
