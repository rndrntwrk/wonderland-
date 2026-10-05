# Wonderland game services

DOM-free shared contracts for the browser and native original-service gateway.
This crate owns source HTTP route builders and OAuth/XML parsers, lossless
decimal resource keys, typed directory queries, browser session/operation/event
DTOs, the operation epoch ledger, and the original Aries/Voltron/Electron wire
codec. It contains no account database, fabricated world or browser socket code.

Public modules are `account`, `directory`, `gateway`, `session`, and `protocol`;
all shared DTOs and account/directory functions are also re-exported at the crate
root. Protocol functions live under `protocol` to keep raw encoding separate
from gateway admission. Native authentication, ownership/actor checks, pinned
destinations, live EOD authority and original socket lifecycle belong to
`services/browser-gateway`.

The gateway README documents HTTP/WebSocket shapes, original route behavior,
resource boundaries, operation outcomes and concrete remaining adapters. Run
`cargo test -p wonderland-game-services` for source-format contract tests.
