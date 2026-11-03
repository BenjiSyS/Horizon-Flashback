# Implementation and evidence

This is an early Rust port, **not the full Flashback mod**. It is not ready for a
Horizon registry submission. Source-based native validation and live gameplay
validation are separate gates.

| Feature | Current implementation | Evidence |
|---|---|---|
| Typed replay scene/delta model | Camera, entity, block, dimension, weather, clock | Core unit/regression tests |
| Replay storage | Version 1 `.hfr`, framed JSON, CRC32, bounded records and total input | Core tests; not Java replay compatible |
| Seeking | Snapshot lookup and event reconstruction, including backwards seeks | Core tests |
| Recording controls | Start/pause/resume/stop core; native camera start/stop | Core lifecycle tests; native runtime not tested |
| Native capture | Camera position, yaw, pitch and FOV | Native adapter typechecks; live behavior not tested |
| Native world capture | Not implemented | No claim |
| Offline world playback | Not implemented | No claim |
| Camera timeline | Step, linear, Catmull-Rom positions, shortest-angle rotations | Core tests |
| FOV/speed/time tracks | Core sampling and serialization | Core tests; incomplete native/editor exposure |
| Editor history | Camera key copy, move, delete, undo/redo | Core tests |
| Native editor | Basic toolbar, camera row, seek, key insertion, save/load | Native adapter typechecks; live UI not tested |
| Original Flashback UI parity | Not implemented | No screenshot comparison |
| Orbit/entity tracking/shake/freeze/timelapse/audio/block override | Not implemented | No claim |
| Entity/particle/audio/skin rendering parity | Not implemented | No claim |
| Flashback ZIP recording import/export | Not implemented | `.hfr` is a different format |
| Video/audio/screenshot export and queue | Not implemented | Frame scheduling math only |
| Improved performance | Unmeasured | No Flashback/Horizon benchmark comparison |
| Platform support | Core compiled/tested on Windows | Linux/macOS and live native client not tested |
| Split-screen | Disabled in native adapter | Needs separate session ownership and tests |

## Native architecture

`native/horizon-mixin.toml` pins Horizon and declares a support module, engine crate,
and hooks for ticks, keys, camera output, FOV, screen drawing, clicks and the Mods
list. `tools/native.py` creates the source bundle from an explicit allowlist and
exports the pinned upstream source into ignored `target/`; it never rewrites the
reference clone. AST transformation is exclusively Horizon Mixins plan/apply.

Horizon Mixins currently rejects state injection onto the lifetime-generic
`Game<'a>`. The adapter therefore uses a render-thread session map keyed by a weak
window reference. It removes entries whose windows have closed and disables
split-screen. Server ticks never capture camera state.

The preview overrides the rendered camera only. It never applies recorded deltas
to the live player, world or server. The live world continues while the editor is
open. Leaving the world/dimension stops capture. Escape/F7 releases preview.

## Native development commands

Requires Python 3.12+, Git, Rust, and the exact source checkouts listed in
`compatibility.json`. Pass `--horizon PATH --mixins PATH` for other locations.

```powershell
py -3.12 -I tools/native.py stage
py -3.12 -I tools/native.py plan
py -3.12 -I tools/native.py check
py -3.12 -I tools/native.py build
```

`check` compiles the transformed client with `--no-default-features`; it does not
run it or verify playback. `build` also omits the optional audio/controller
features. A successful `plan` checks AST targets, not Rust borrowing/types.
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

## Remaining integration work

1. Capture configuration registries and initial loaded world state, then all play
   packets/local-player updates with stable protocol/version metadata.
2. Create an isolated offline replay session with no live-server writes; implement
   restoration of chunks, entities, attachments, dimensions, clocks and caches.
3. Port all editor panels, tracks, shortcuts, settings, visual overrides and
   resource handling; compare against the original UI and behavior.
4. Add deterministic render/audio stepping, GPU readback, encoders, export queue,
   cancellation and recovery. Verify output media with independent decoders.
5. Validate the complete native client in fresh player-data directories, then run
   repeatable Flashback/Rust comparisons on the same scene and hardware.
6. Include the written author grant and resolve source-review licensing before
   preparing a `.hnmod` release and registry entry.
