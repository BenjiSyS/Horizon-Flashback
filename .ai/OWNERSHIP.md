# Ownership

- Every implementation worker has explicit file/subsystem ownership.
- Two agents do not concurrently modify the same files unless intentionally coordinated.
- Read-only investigation may overlap.
- Git status/diff is authoritative for what changed.
- Each handoff reports changed paths and verification status.
- Unresolved conflicts are integrated by the designated root.
