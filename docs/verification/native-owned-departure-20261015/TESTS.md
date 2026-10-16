# Candidate qualification

Matching reconstructed Rust/rustfmt/Clippy 1.99.0; locked offline dependencies, one build job, existing shared target, no optimization.

## Terminal gates

- Core: 58 tests across `runtime_owned_leave`, `owned_admission`, `lot_permission_state`, and `runtime_route_animation`.
- Boundary: 26 tests across `source_interactions` and `native_owned_recovery`.
- Contracts and host: 10 DTO tests and all 28 native host tests.
- External primitives: 16 tests, including synchronous source GenericTSO mode40 and unchanged TS1 dispatch.
- Runner/UI: 41 lightweight tests supplied by the runner owner, covering provider/socket/placement/departure helpers and the separately reviewed optional authored hook.
- Strict Clippy: `cargo clippy --locked --offline -j1 -p sim-core -p wonderland-game-runtime -p wonderland-native-host-contracts -p wonderland-native-lot-host --all-targets -- -D warnings`.
- Current web-shell wasm32: `cargo check --locked --offline -j1 -p wonderland-web-shell --target wasm32-unknown-unknown` passed. The retained log records only the existing third-party `proc-macro-error2` future-compatibility warning.
- Matching formatting: `cargo fmt --all -- --check` completed without output.
- Fresh real original-content negative placement and positive source leave/save/detach/re-entry witnesses: see `ORIGINAL-ROUNDTRIP.md` and retained sanitized logs.

The focused suites cover schema1–5 migration into schema6, replay, malformed retained departure state, exact generation fencing, failure atomicity, blocked unsupported effects, ordinary-actor fatal errors, source leave vs forced cleanup, save-before-detach, immutable host receipts, peer cancellation, unknown-action recovery and placement-only suspension. Existing source-interaction tests required materializing only three exact already-tracked FreeSO-authored IFF fixtures from the pin; their bytes were unchanged.

A final lint-only edit uses the equivalent key-based canonical field sort. The observed serialization golden tests passed before that equivalent expression change, and strict Clippy compiles all affected targets afterward. No runtime behavior was altered to satisfy the tests.

This is a bounded candidate qualification, not a fresh full-workspace run or a complete rendered-gameplay claim. The frozen prior route/core source retains its separate full-suite evidence. The current web-shell wasm32 terminal result is retained separately.
