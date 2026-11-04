fn hook(
    game: &mut Game<'_>,
    partial: f32,
) -> core::ops::ControlFlow<Option<(glam::DVec3, f32, f32)>> {
    match crate::game::flashback_native::camera(game, None, partial) {
        Some(camera) => core::ops::ControlFlow::Break(Some(camera)),
        None => core::ops::ControlFlow::Continue(()),
    }
}
