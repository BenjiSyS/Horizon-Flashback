fn hook(mods: &mut Vec<NativeMod>) {
    mods.push(NativeMod {
        id: "flashback".into(),
        title: "Flashback Rust (prototype)".into(),
        description:
            "Camera editing and experimental world packet recording/playback. Full parity and live behavior remain unverified."
                .into(),
        keys: vec![
            ["world_record".into(), "Arm / stop world recording".into(), "key.keyboard.f8".into()],
            ["world_open".into(), "Open latest world tape from title".into(), "key.keyboard.f9".into()],
            [
                "record".into(),
                "Record / stop camera".into(),
                "key.keyboard.f6".into(),
            ],
            [
                "editor".into(),
                "Open camera editor".into(),
                "key.keyboard.f7".into(),
            ],
        ],
    });
}
