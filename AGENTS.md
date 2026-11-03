# Horizon Flashback

Native Rust replay/cinematic mod for Horizon. Read `.ai/PROJECT.md`,
`.ai/ROUTING.md`, and `.ai/VERIFY.md` before changing architecture.

`src/replay.rs` owns replay storage and deterministic state reconstruction.
`src/timeline.rs` owns cinematic tracks and editor history.
`src/tape.rs` owns the bounded streaming network tape container.
`native/network.rs` owns native capture, replay transport, flow control, and its tests.
Keep host-dependent integration separate from the testable replay engine.
`.upstream/` and `target/` are ignored reference/build trees. Never commit them.
Never commit Minecraft assets, player recordings, authentication data or binaries.
Preserve Moulberry's original copyright. Do not invent a permissive license grant.
Do not claim parity, native runtime support, or improved speed without evidence.

Verify with `cargo test --locked`, `cargo fmt --check`, and
`cargo clippy --all-targets --locked -- -D warnings`. Review the diff before completion.
For network changes also run `py -3.12 -I tools/native.py verify`. Its native tests
use synthetic tapes through the real Horizon Session API; they do not prove live gameplay.
No Haiku. Use Laya's benji roles. Never commit unless requested.
