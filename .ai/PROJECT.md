# Project

## PROJECT PURPOSE
Port Flashback's recording, replay and cinematic editing workflows to Horizon in Rust.

## STACK
Rust 2024, Cargo, serde. Horizon native Rust source integration.

## IMPORTANT DIRECTORIES
- `src/`: testable engine.
- `src/tape.rs`: bounded streaming protocol-event tapes, independent of Horizon.
- `src/actor.rs`: validated local-player samples, independent of Horizon.
- `native/`: host-specific integration when implemented.
- `native/network.rs`: opt-in capture, socketless Session transport and native tests.
- `native/player.rs`: host player sampling and post-prediction pose restoration.
- `.upstream/`: ignored reference clones; never publish.
- `docs/`: compatibility and feature evidence.

## ARCHITECTURAL BOUNDARIES
Replay/timeline core must not depend on the full game. Native hooks must be pinned,
compiled and runtime tested separately. A core test is not native integration proof.

## IMPORTANT INTERFACES
Horizon native mixin bundles, native hooks, network packet application and camera extraction.

## GENERATED FILES
Cargo.lock and target/. Do not edit generated build files manually.

## DO-NOT-CASUALLY-EDIT AREAS
Player worlds/resources, authentication, original notices, upstream source checkouts.
