# Implementation and evidence

This is an early Rust port, **not the full Flashback mod**. It is not ready for a
Horizon registry submission. Source-based native validation and live gameplay
validation are separate gates.

| Feature | Current implementation | Evidence |
|---|---|---|
| Typed replay scene/delta model | Camera, entity, block, dimension, weather, clock | Core unit/regression tests |
| Replay storage | `.hfr` scene files and streaming `.hfnet` protocol tapes; checksummed bounded frames | Core tests; not Java replay compatible |
| Seeking | Snapshot lookup and event reconstruction, including backwards seeks | Core tests |
| Recording controls | Start/pause/resume/stop core; native camera start/stop | Core lifecycle tests; native runtime not tested |
| Native capture | Camera position, yaw, pitch and FOV | Native adapter typechecks; live behavior not tested |
| Native world capture | Opt-in capture from the next connection: registries, feature flags, brand, play packets, reconfiguration | Source implementation and codec tests; real recording NOT TESTED |
| Offline world playback | Socketless Session transport, timed packet delivery, pause/speed, prefix replay for seeking | Native Session tests; rendered scenes NOT TESTED |
| Camera timeline | Step, linear, Catmull-Rom positions, shortest-angle rotations | Core tests |
| FOV/speed/time tracks | Core sampling and serialization | Core tests; incomplete native/editor exposure |
| Editor history | Camera key copy, move, delete, undo/redo | Core tests |
| Native editor | Basic toolbar, camera row, seek, key insertion, save/load | Native adapter typechecks; live UI not tested |
| Original Flashback UI parity | Not implemented | No screenshot comparison |
| Orbit/entity tracking/shake/freeze/timelapse/audio/block override | Not implemented | No claim |
| Entity/particle/audio/skin rendering parity | Not implemented | No claim |
| Flashback ZIP recording import/export | Not implemented | `.hfr` and `.hfnet` are different formats |
| Video/audio/screenshot export and queue | Not implemented | Frame scheduling math only |
| Improved performance | Unmeasured | No Flashback/Horizon benchmark comparison |
| Platform support | Core compiled/tested on Windows | Linux/macOS and live native client not tested |
| Split-screen | Disabled in native adapter | Needs separate session ownership and tests |

## Native architecture

`native/horizon-mixin.toml` pins Horizon and declares game/network support modules,
an engine crate linked through `horizon-client-lib`, and hooks for networking,
ticks, keys, camera output, FOV, screen drawing, clicks and the Mods list.
`tools/native.py` creates the source bundle from an explicit allowlist and
exports the pinned upstream source into ignored `target/`; it never rewrites the
reference clone. AST transformation is exclusively Horizon Mixins plan/apply.

Horizon Mixins currently rejects state injection onto the lifetime-generic
`Game<'a>`. The adapter therefore uses a render-thread session map keyed by a weak
window reference. It removes entries whose windows have closed and disables
split-screen. Server ticks never capture camera state.

Camera preview overrides the rendered camera. World tapes open through a separate
replay connection whose network run returns before socket creation or server
authentication. Its outgoing writer stays absent. F9 opens a tape only from an
idle title screen; seeking is restricted to an already-open replay connection.
Leaving the world/dimension stops camera capture. Escape/F7 releases its preview.
See [NETWORK-REPLAY.md](NETWORK-REPLAY.md) for protocol scope and remaining gaps.

## Native development commands

Requires Python 3.12+, Git, Rust, and the exact source checkouts listed in
`compatibility.json`. Pass `--horizon PATH --mixins PATH` for other locations.

```powershell
py -3.12 -I tools/native.py stage
py -3.12 -I tools/native.py plan
py -3.12 -I tools/native.py check
py -3.12 -I tools/native.py build
py -3.12 -I tools/native.py test-net
py -3.12 -I tools/native.py verify
```

`check` compiles the transformed client with `--no-default-features`; it does not
run it or verify playback. `build` also omits the optional audio/controller
features. A successful `plan` checks AST targets, not Rust borrowing/types.
`test-net` runs the adapter's tests through Horizon's real `Session::connect` entry
point, without loading game resources. `verify` runs both the full native typecheck
and these tests plus native-library Clippy after one transformation. CI has separate
root-core and native-protocol jobs; the latter fetches the exact source pins.
`target/native-validation.json` records completed actions and source hashes.
Bundles and transformed stages use separate directories for each source revision,
so concurrent/repeated checks do not replace a stage being compiled. Bundle reuse
verifies every staged source byte and rejects unexpected edits.
Prepared Minecraft resources must come from the player's own installation and
must never be published.

Prototype controls: F6 records/stops camera; F7 opens/closes editor; Space plays;
Left/Right step; Home/End seek; K inserts/replaces a camera key; Z/Y undo/redo;
Delete removes the current camera key; S saves; O loads the latest saved camera
project; N starts a fresh take after the current one has been saved. Saves use
temporary files and publish the project file after its recording is ready.
Saves live in Horizon's `config/flashback/`, never in the source tree.

World-tape controls: F8 arms recording for the next join/rejoin, or stops/disarms
recording. F9 opens the latest tape from the title screen. In a replay's F7 editor,
Space pauses packet delivery; Left/Right reconstruct a requested tick from the
beginning; Home restarts; Minus/Equals select half/double packet speed. Timeline
clicks request a replay seek. A nonzero seek opens paused. These UI paths have not
been tested in a live game. The standalone CLI can inspect container integrity:
`cargo run --locked --bin flashback -- tape-inspect PATH.hfnet`.

## Remaining integration work

1. Complete local-player recording/reconstruction, resource/mod identity checks,
   custom payload handling and indexed world snapshots for fast seeking.
2. Validate restoration of real chunks, entities, dimensions, clocks and caches;
   implement true paused/stepped client simulation and deterministic rendering.
3. Port all editor panels, tracks, shortcuts, settings, visual overrides and
   resource handling; compare against the original UI and behavior.
4. Add deterministic render/audio stepping, GPU readback, encoders, export queue,
   cancellation and recovery. Verify output media with independent decoders.
5. Validate the complete native client in fresh player-data directories, then run
   repeatable Flashback/Rust comparisons on the same scene and hardware.
6. Include the written author grant and resolve source-review licensing before
   preparing a `.hnmod` release and registry entry.
