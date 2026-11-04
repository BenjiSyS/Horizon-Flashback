# Validation snapshot — 2026-11-04

Environment: Windows x64, Rust 1.98.1, Python 3.12.10.

Passed on the current source revision:

- `cargo fmt --check` and rustfmt checks for all native helpers.
- `cargo test --locked`: **38 tests passed** (14 library, 7 cinematic integration, 10 regression, 7 native
  adapter/player tests against host doubles). No failures. New coverage includes
  pinned Java interpolation outputs, transactional multi-track editing, orbit and
  entity tracking, fractional camera/FOV alignment, player pose restoration, transition
  resets, rejected live control payloads, and replay input/pause branches.
- `cargo clippy --all-targets --locked -- -D warnings`.
- `py -3.12 -I tests/test_native_tools.py`: **3 tests passed** for the source
  allowlist, immutable bundle revisions, and rejection of modified bundle data.
- The replay CLI created a synthetic 400-tick/800-event recording, read it back
  with five snapshots, and sought to tick 200: camera `(10,72,0)`, yaw 100,
  time of day 6200.
- The `tape-inspect` CLI read an independently Python-generated checksummed tape
  container: one event at tick 40, 17 payload bytes, no recovered tail.
- Horizon Mixins applied **16 hooks, 4 support modules and 1 engine crate** to the
  exact Horizon revision in `compatibility.json`.
- `cargo check --locked -p horizon-client --no-default-features` passed on the
  transformed full Horizon client with no warnings. Audio/controller features
  were disabled. Every bundle source hash was compared with the current source;
  [native-evidence.json](native-evidence.json) records those hashes and the pins.
- **7 targeted native Session tests passed** through the real `Session::connect`
  path: registry/block-packet roundtrip with no live writer, paused prefix seeking,
  fail-closed malformed addresses, forbidden control payloads, bounded queues and
  host backpressure, cancellation including immediate close before polling, and
  ordered local-player samples with malformed-record rejection. Reader delivery
  does not apply player state ahead of game-thread consumption.
  Other Horizon tests were filtered out; this does not claim the full upstream suite.
- Native-library Clippy passed with warnings denied:
  `cargo clippy --locked -p horizon-client-lib --no-default-features --lib -- -D warnings`.
- The required combined gate, `py -3.12 -I tools/native.py verify`, passed on the
  final source: full client typecheck, all 7 targeted Session tests, native Clippy.
- The full native client executable built and linked (SHA-256 and byte size in
  `native-evidence.json`):
  `cargo build --locked -p horizon-client --bin horizon-client`, from the transformed
  workspace, with default audio and controller features enabled. The required
  `tools/native.py verify` gate still uses `--no-default-features` separately.
  Local artifact: `target/native-game/target/debug/horizon-client.exe`; its hash and
  size are in `native-evidence.json`. It has **not been launched**.
- CI defines separate core and native-protocol jobs. Native CI fetches the exact
  source pins, typechecks, runs the Session tests, and runs native-library Clippy.
  This document makes no claim about a future CI run before it completes.

Not verified: native executable launch, live camera behavior, full world
capture or playback, visual parity, audio/video output, Linux/macOS, and improved
performance. A compile check or linked executable is not a live gameplay test.
The user selected source/build work and asked to leave live testing unverified.

Full port remains incomplete. See [STATUS.md](STATUS.md) for the feature matrix
and remaining integration work. No `.hnmod` release or registry PR was created.
