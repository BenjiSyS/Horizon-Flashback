# Full-port implementation ledger

Reference: Flashback `bf046b763ddf466bdc1b6d9c6cc7ba06e527f3e2`, Horizon
`d91353641866b85d851926da3b1a9fdee46392f1`. This ledger tracks source work and
acceptance evidence. It does not declare full parity. Live gameplay testing remains
unverified at the user's request. A source model alone does not complete a feature.

## Cinematic keyframes

All eleven families are registered in upstream `Flashback.java:288–298`.

| Upstream family | Rust engine | Host/editor work remaining |
|---|---|---|
| Camera | Position/rotation curves, generic track editing and history | Complete editor panels, roll and camera-path visualization |
| Camera orbit | Full-turn yaw, center/distance curves, eye-space camera conversion | Native typecheck and editor controls |
| Track entity | UUID, head/body/root, offsets, sided target selection and pose conversion | Native subject coverage, typecheck and selection controls |
| Camera shake | Independent X/Y frequency/amplitude curves and serialization | Matching FastNoiseLite noise, quaternion application and UI |
| FOV | Focal-length interpolation and preview override | Full panel/shortcut exposure and live comparison |
| Speed | Factor curves, track precedence and serialization | Real-time interpolation map, native simulation/export clock |
| Timelapse | Cumulative-time keys produce tickrate factors | Native clock, UI, invalid-key feedback |
| Time of day | Multi-day scalar interpolation, integer changes | Native environment override and UI |
| Freeze | Frozen/delay keys, held changes | Original slowdown formula, simulation/render integration and UI |
| Audio | Clip path, timing, duration, gain and overlapping tracks | Decode/cache, waveform, preview, mixing/export and UI |
| Block override | Dimension/block data and transactional editing | Palette compatibility, brush/editor, rendering overlay and undo integration |

Interpolation modes: hold, linear, smooth, ease-in, ease-out, ease-in-out and
Hermite are represented. Sided easing and time-aware centripetal numeric outputs
are compared with the actual pinned Java source. The Java harness supplies small
arithmetic/vector stubs; it does not run Minecraft. Hermite polynomial tests cover
known polynomials and hold boundaries. Curves still need speed-aware real-time
mapping and complete host/editor coverage.

`tools/reference_curves.py` regenerates the numeric fixture with a JDK and the
exact upstream source checkout. The fixture records source hashes. No game assets
are used. `tests/cinematic_tracks.rs` covers track ordering, enable state,
serialization, actor-relative camera math, transactional editing and project
version migration. `tests/native_adapter.rs` checks actual adapter code against
a host double, including sub-tick camera/FOV alignment.

## Editor and user workflows

The following upstream window units need full ports, interaction tests and visual
comparison. The current HUD toolbar does not satisfy these acceptance criteria.

- Main menu bar and replay/scene/project workflows.
- Timeline: track creation/reordering, properties, interpolation, drag/resize,
  selection, clipboard, markers, snapping, zoom/pan and camera preview.
- Movement and free-camera controls.
- Visuals and camera paths.
- Render filters.
- Player list and selected-entity popup.
- Preferences and configurable keybinds.
- Start export and screenshot settings.
- Export queue, progress, cancellation and export-done window.
- Docking/window visibility and layout persistence.

Reference units: `editor/ui/windows/*.java`, `ReplayUI.java`,
`EditorMovementControls.java`, `editor/keybinds/*.java`, `state/*.java` and
`configuration/*.java`. Each panel's individual options must be audited before
its implementation can be marked complete.

## Recording, playback and storage

- Existing: bounded checksummed scene/tape formats, semantic packet capture,
  exact protocol pin checks, socketless replay Session, prefix seeking, queue
  backpressure and cancellation.
- Existing initial actor path: local position/interpolation endpoints, velocity,
  rotations, selected slot and movement flags.
- Remaining: independent recorded local actor/model; equipment, inventory,
  animations, riding, skins, accurate positions and all relevant local events.
- Remaining: resource/mod identity, safe resource handling, chunk cache,
  indexed snapshots, fast/reverse seeking, reconfiguration/dimension restoration,
  recording pause/resume/recovery and recording management screens.
- Remaining: original Flashback ZIP metadata/action/chunk formats and compatible
  import/export. New `.hfr`/`.hfnet` files are not Java replay files.
- Remaining: one project linking scene/timeline, world tape, capture clock,
  media references, identity and recovery information.
- Remaining: deterministic pause/step/speed across world, player, entities,
  particles, weather, animations, audio and render interpolation.

## Media export

Reference units: `exporting/*.java`, `sound/*.java` and export windows.

- Exact rational frame schedule exists; full export execution does not.
- Remaining: deterministic simulation/render stepping, GPU readback and bounded
  framebuffer queue; resolution/aspect/crop and screenshot output.
- Remaining: video containers/codecs/pixel formats, FFmpeg lifecycle, PNG sequence,
  transparency/depth/HDR options where supported upstream, encoder constraints
  and failure reporting.
- Remaining: deterministic audio capture/mixing/resampling, imported audio and
  synchronization with video.
- Remaining: persistent jobs, queue order, progress, cancellation, recovery,
  overwrite policy, output validation and taskbar integration.

## Completion gates

Every implemented behavior needs traceable source/reference evidence, deterministic
tests where possible, native compilation and reviewed diffs. Releases must retain
original notices and the user's permission scope. No Minecraft resources, real
recordings, credentials or binaries may enter Git. Live visual/gameplay parity and
performance comparisons cannot be asserted from source/build tests.
