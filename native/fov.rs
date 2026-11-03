fn hook(game: &Game<'_>, position: [f64; 3], degrees: &mut f32) {
    crate::game::flashback_native::fov(game, position, degrees);
}
