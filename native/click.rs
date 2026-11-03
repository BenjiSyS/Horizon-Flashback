fn hook(game: &Game<'_>, screen: &NativeScreenView<'_>, button: i32, canceled: &mut bool) {
    if crate::game::flashback_native::click(game, screen.mouse, button) {
        *canceled = true;
    }
}
