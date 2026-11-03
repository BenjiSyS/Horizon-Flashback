fn hook(mods: &mut Vec<NativeMod>) {
    mods.push(NativeMod {
        id: "flashback".into(),
        title: "Flashback Rust (prototype)".into(),
        description:
            "Camera recording and cinematic timeline preview. Full world replay is not implemented."
                .into(),
        keys: vec![
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
