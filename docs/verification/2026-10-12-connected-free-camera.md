# Connected free-flight camera

This increment connects the existing Rust first-person camera to the actual WASM lot view. It does not qualify the full FreeSO rewrite, hosted multiplayer, or physical-device performance.

## Behavior

- Enter from the orbit camera's actual eye and facing; leave to the previous orbit framing.
- Drag to look; WASD/arrows move horizontally, Q/E move vertically, Shift accelerates, Z/X change the lens.
- Six hold-to-fly controls support touch and keyboard activation.
- The original Rust damping model, actual terrain-corner interpolation, and selected-floor height bound constrain movement.
- Rendering and spatial sound use the same active camera. Camera input has no simulation command handle.
- Blur, document hiding, mode changes, and unmount clear held inputs and velocity. Catch-up time is bounded.
- A stationary click/tap still selects a real world tile. Dragging rotates without selecting.

## Executed evidence

The executable `free_camera_journey` covers camera entry/facing, 30/60 Hz flight, screen-right strafing, stopping, invalid input, sloped grounding, serialization, and world-state preservation. The original ignored-mode, wrong-strafe, and slope failures were reproduced before repair.

The actual compiled browser journey in `tools/native-browser/verify.mjs` runs against a real Rust authority with declared original-BHAV fixtures. It checks changing GPU framebuffer output while flying, held-key blur, immediate hidden/resume without a timer tick, no new Sim commands, orbit return, source action completion, two-browser state, cancellation, and cleanup. Populated queues and all six flight controls are hit-tested at 1440×1000, 390×844, 320×600, and 844×390.

`verify-construction.mjs` starts in free camera and checks real tile selection, exact-price purchase confirmation, reconnect persistence, floor painting, responsive panels, and touchscreen quote/cancellation. No real money is involved in this controlled authority.

The existing workspace regression suite passed 2,023 tests with five pre-existing ignored cases. Strict native and WASM lint checks and the release WASM build are required before publication. Browser screenshots depict declared controlled lots, not a finished expansive game.

## Remaining parity and release limits

Returning to orbit restores the old framing instead of the source controller's flown-position-derived center. This is an explicit partial parity result. No live deployment, hosted authentication, public economy, physical phone, or complete asset-pack acceptance follows from these local checks.
