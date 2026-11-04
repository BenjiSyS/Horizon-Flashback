# Network replay source implementation

This layer is experimental. Headless transport tests do not prove that a real
Minecraft scene renders or that Flashback's behavior has been matched. The user
selected continued source/build work with live testing left unverified.

## Capture

F8 arms the next new connection. The hook runs at the beginning of Horizon's
network routine, before login, and consumes that opt-in for one connection.
Recording begins with the first supported semantic event, so authentication and
connection setup are not copied into the recording. Arming halfway through a
world does not manufacture a missing initial snapshot: rejoin to start capture.

The adapter records configuration registries, dialog tags, custom-component
identity, player UUID/name, feature flags, brand, ordinary play packets and
reconfiguration transitions. Registries are encoded with Horizon's actual protocol
codec. The event stream is captured before the client's asynchronous chunk
decoder and is delivered in the same order during replay.

Cookies, account/session tokens, resource-pack requests, server transfers, server
links/dialogs and arbitrary mod control payloads are excluded. Only the recognized
attachment-sync custom payload is retained. Original gameplay still receives the
unchanged events: these exclusions apply to the recording, not the live session.

Post-tick local-player samples share the packet tape's clock. They contain position
and its previous interpolation endpoint, velocity, body/head rotation, movement
flags and selected hotbar slot. These are
typed kind-8 records, not trusted arbitrary custom payloads. Samples reject
nonfinite numbers, invalid pitch/slots, unknown fields and payloads over 4 KiB.
The reader converts them to a reserved internal event in packet order. Only an
active replay Session can accept that event; live server payloads cannot use it.
The game applies samples after its predicted movement tick. This is an initial
pose restoration path, not a complete independent recorded player actor.

Tapes are created with `create_new` in `config/flashback/world-*.hfnet`. Recording
flushes periodically and when stopped/disconnected. A write error stops capture
and is reported in recording status. Existing files are not overwritten.

## Format and bounds

`src/tape.rs` has no game or networking dependency. A tape has an `HFNET001`
signature, a schema/revision/protocol/game-version metadata frame, then frames
containing a tick, event kind and opaque payload. Header and payload CRCs detect
complete-frame corruption. A truncated last frame can be recovered after valid
metadata; a truncated metadata frame or a complete corrupt frame is rejected.

- At most 16 MiB per payload and 1 GiB per tape.
- At most 64 queued replay events, normally within an 8 MiB encoded-data budget.
  One larger event can pass alone; a registry snapshot has a separate 64 MiB cap.
- The replay producer stops while Horizon's decoded-event backlog exceeds 32.
- Playback streams the file and does not load the entire tape into memory.
- Protocol frames retain their original IDs/bytes. Exact recorded Horizon,
  protocol and game versions must match before events are replayed.

## Replay and lifecycle

The `flashback-replay:TICK:PATH` address is intercepted before the normal network
routine can parse an address, open a socket, or authenticate with a server.
Malformed replay addresses fail in that branch and never fall back to ordinary
network login. The Session's live writer remains absent, so outgoing play packets
are discarded by Horizon's normal send method.

Recorded registries become `Event::Joined`/`Reconfigured`; play packets use Horizon's
normal decoder and enter its usual event queue. Unsafe control packet kinds are
also rejected when reading a tape, including files produced outside this adapter.
The last scene remains open at EOF. Closing/disconnecting cancels the worker and
wakes it even while paused, at EOF, or blocked by backpressure. Cancellation is
registered before file IO; an early close also closes the event channel.

Seeking reconnects to a fresh replay session and replays the prefix from the
beginning. It does not seek by changing a live connection's state. There is no
indexed world-snapshot acceleration yet, so long seeks can be slow.

Replay suppresses queued gameplay input. Pausing or reaching EOF skips the
client-level tick while the outer client tick still services native editor controls.
This does not yet freeze every animation: the outer rocket tick and render-driven
effects still need coordinated simulation control. Live pause behavior is unverified.

## Important remaining work

- Local-player samples restore selected pose/movement fields. Inventory prediction,
  equipment, full animation, riding, model, voice and audio still need a separate
  recorded actor. Render-clock alignment and prediction side effects remain unfinished.
- Timestamping follows observed packet arrival at 20 ticks/second. Pausing the
  packet producer does not yet freeze all client animations/simulation. Full
  deterministic stepping and media export remain unimplemented.
- Camera `.hfr` projects and world `.hfnet` tapes are not yet associated as one
  aligned replay project. Their clocks and capture start times still need integration.
- Game/render integration, real tapes, dimension transitions and full resource
  behavior remain **NOT TESTED**. A matching revision is required, but matching
  installed content/resource packs are not yet hashed and validated.
- "Socketless" describes the replay Session's server transport. Horizon's normal
  game-side services-key/profile/skin lookup paths have not been removed or
  proven offline. This is not a whole-process network isolation claim.
- Original Flashback ZIP recordings are not accepted. These are new Rust formats.
- UI layout/shortcuts and full feature parity are unfinished. No performance
  advantage over Flashback has been measured.

The native tests use the real `Session::connect` entry point, synthetic registry,
block-update and local-player records, and temporary tapes. They test fail-closed
routing, pause/seek, queue limits, ordered player delivery, forbidden payloads, and
cancellation. They never launch a game or access real player worlds/resources.
