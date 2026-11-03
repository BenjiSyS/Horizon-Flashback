fn hook(game: &mut Game<'_>) -> core::ops::ControlFlow<Option<(glam::DVec3, f32, f32)>> {
    match crate::game::flashback_native::camera(game, None) {
        Some(camera) => core::ops::ControlFlow::Break(Some(camera)),
        None => core::ops::ControlFlow::Continue(()),
    }
}
