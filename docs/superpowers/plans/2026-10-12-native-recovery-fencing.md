# Fenced replacement increment

Continue approved full-game Task 3 using the existing native runtime epoch adoption. The private coordinator records a stable recovery grant before any replacement starts and rejects fresh old-owner commits/checkpoints while recovery is pending. Repeated recovery requests for the same predecessor return that record rather than allocating epochs repeatedly.

The Rust replacement first proves the old committed checkpoint/tail cursor, validates the grant against the old binding and exact successor epoch/incarnation, then adopts the epoch through GameRuntime's narrow authority method. Its marker binds the old cursor, new cursor, new binding and new checkpoint digest. The coordinator verifies and reads back the new R2 anchor before atomically finishing recovery. Exact finish retry returns the recorded marker; altered retries fail.

Use real workerd and actual concurrent old/new Rust processes. Keep the old host alive while proving stale-write rejection, restart recovery of a pending grant, stable logical effect IDs with new dispatch epochs, and exact successor acceptance after the new anchor. No deployment or public admission is implied; the supervisor binding remains the private trust boundary.
