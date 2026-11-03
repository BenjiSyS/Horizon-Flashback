# Horizon Flashback

Work in progress: a native Rust port of Flashback for the Horizon game client.
This repository is not yet a complete or playable Flashback replacement.
Feature status and runtime validation must be checked before installing or submitting it.

See [feature status and native build instructions](docs/STATUS.md),
[pinned source revisions](compatibility.json), and [permission record](docs/PERMISSION.md).

Upstream projects:
- [Flashback by Moulberry](https://github.com/Moulberry/Flashback)
- [Horizon](https://github.com/horizon-rust/horizon)
- [Horizon mod registry](https://github.com/horizon-rust/horizon-mods)

The project owner reports author permission to fully port and publish Flashback in
Rust and retain any revenue. The written grant is not included yet. Flashback's
original copyright and restrictive license remain in effect for its original work.
No Minecraft code, assets, prepared resources, recordings, or authentication data
may be committed here. No speed or feature-parity claim is made without measurements.

## Development

```powershell
cargo test --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
```

Exercise the replay core without game resources:

```powershell
cargo run --locked --bin flashback -- demo target/demo.hfr
cargo run --locked --bin flashback -- inspect target/demo.hfr
cargo run --locked --bin flashback -- seek target/demo.hfr 200
```

The demo contains synthetic camera/time data. It is not a Minecraft world recording.
The native camera/editor prototype and the remaining full-port work are detailed
in [STATUS.md](docs/STATUS.md).
