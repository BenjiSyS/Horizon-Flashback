# Validation snapshot — 2026-11-03

Environment: Windows x64, Rust 1.98.1, Python 3.12.10.

Passed on the published source:

- `cargo fmt --check` and rustfmt checks for all native helpers.
- `cargo test --locked`: **21 tests passed** (8 library, 10 regression, 3 native
  adapter tests against a host double). No failures.
- `cargo clippy --all-targets --locked -- -D warnings`.
- `py -3.12 -I tests/test_native_tools.py`: **3 tests passed** for the source
  allowlist, immutable bundle revisions, and rejection of modified bundle data.
- The replay CLI created a synthetic 400-tick/800-event recording, read it back
  with five snapshots, and sought to tick 200: camera `(10,72,0)`, yaw 100,
  time of day 6200.
- Horizon Mixins applied **8 hooks, 1 support module and 1 engine crate** to the
  exact Horizon revision in `compatibility.json`.
- `cargo check --locked -p horizon-client --no-default-features` passed on the
  transformed full Horizon client with no warnings. Audio/controller features
  were disabled. Every bundle source hash was compared with the current source;
  [native-evidence.json](native-evidence.json) records those hashes and the pins.
- The first GitHub Rust CI run passed at commit `2166cc98440f43a8a0123db6009017593dbb851c`:
  https://github.com/BenjiSyS/Horizon-Flashback/actions/runs/41705288540
  Later commits have separate CI runs; this is not a claim about their results.

Not verified: native executable linking/launch, live camera behavior, full world
capture or playback, visual parity, audio/video output, Linux/macOS, and improved
performance. A compile check is not a live gameplay test. Owned Java 26.3 resources
were not available for a live test during this session.

Full port remains incomplete. See [STATUS.md](STATUS.md) for the feature matrix
and remaining integration work. No `.hnmod` release or registry PR was created.
