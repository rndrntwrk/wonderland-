# Connected Wonderland game integration

## Intent and authority

The user requests completion of the remaining work after the feature-preservation correction in PR #10. The existing action-focused design is retained: visual character choices, selectable map locations, actions attached to people and objects, direct construction in the world, and compact contextual panels. The correction is the base, not permission to reduce FreeSO's capabilities. The user has authorized implementation and PR publication; deployment and merging are separate actions.

The source of truth for player scope is `docs/design/action-first/player-capability-map.md`. Original `TSOClient` and `Other` source remains intact. Existing preview saves remain readable. Original game identities, prices, capacity, permissions, catalogs and lot geometry must come from their source or authoritative service. Resource-safety bounds must not become new product limits.

## Existing work to integrate

| Area | Exact source |
| --- | --- |
| Corrected browser shell | PR #10, `7ee14583ca10ed86511d717566295ea44d481bf7` |
| Swarm A simulation | `8a0e251d19e222a0a6833d7408ca629f674e1729` |
| Swarm B content/runtime/tools | `3d6b04e0a600cb5f947145123cddf6cb2c75c5d7` |
| Swarm C rendering/audio | `f6f78be1fef247f2db47e19f56d94054f0c9e88c` |
| Original source baseline | `4c6b3e8f5835b228723caea3c9f683c62f244f73` |

These branches are separate implementations. Imported source must retain attribution and the exact upstream revision. Preserve the correction's original-content decoder fixes when reconciling Swarm B. Do not replace the root Cargo lock with another branch's standalone lock.

## Connection model

Use the original server's existing OAuth HTTP route, XML roster/shard/selection responses and native protocol semantics. The server URL is operator configuration, never an assumed connection to the upstream public service. Credentials and bearer tokens must not enter logs, URLs, persistent browser storage or preview saves. Login, roster, shard selection, admission, disconnect and reconnect have explicit states; a token alone is not a city session.

HTTP capability uses `POST /userapi/oauth/token`, then bearer-authenticated `/cityselector/app/AvatarDataServlet` and `/cityselector/app/ShardSelectorServlet`; shard discovery uses `/cityselector/shard-status.jsp`. Validate error bodies even on HTTP 200. Preserve original 64-bit outfit keys without conversion through JavaScript numbers. The real roster does not carry live motives; unknown motives and balances must remain unknown until a live projection supplies them.

A browser cannot open the original city/lot TCP sockets. A native Rust gateway must mediate the authenticated legacy session. Its destination comes from authenticated server selection and operator configuration. Do not accept arbitrary browser-supplied TCP destinations. Keep account, avatar, shard, lot and incarnation identities distinct. Every write uses the actual authority, accepted/rejected response and correlated operation identity. A transport connection is not proof of a working service operation.

## World and simulation

Import the portable source renderer and actual simulation/content bridge. Use a normalized, versioned world document for rendering inputs; a Home owner is not a lot ID. Geometry includes source dimensions, levels, heightfield, floors, wall segments/patterns, roofs, water/pools and placed objects. Original blueprint XML is a valid source input, with provenance. A normalized document is also the boundary for live architecture or supplied saved lots.

Render actual source geometry with a real camera and depth-tested picking. The software path must use the existing reference renderer's depth and ID buffers; painter-order avatar fallback is not a world picker. Camera, floor and wall modes must visibly affect the same scene that receives interaction. Preserve source model and texture identities; missing resources are not replaced by invented original-game objects. Existing illustrated previews remain explicitly identified preview content.

The simulation bridge imports real BHAV/OBJD/TTAB/SLOT/BCON data and runs actual accepted ticks. Offers must use source check-tree semantics and authoritative revalidation, not simply expose every TTAB row as executable. Effects, queue state, needs and lifecycle events are projected from runtime state. Construction uses actual architecture command semantics, permissions, costs/refunds and commit/rejection. Never change displayed world data to simulate a successful server transaction.

## Remaining player surfaces

Complete usable, contextual surfaces for account/city choice; roster/create/retire; owned wardrobe; people/profile/relationships/bookmarks; lot/private chat and inbox; property/residents/rights; neighborhood/bulletins/rankings/elections; and source EOD sessions. Use service data, action availability, pending state, rejection and recovery. Source functionality disabled in the original client is not invented as working legacy behavior. Existing fields and options remain represented.

Graphics, camera, floors/walls, sound, volume and motion settings control actual implemented adapters. Audio must use source cues/assets and user-gesture activation. Reduced motion and keyboard/touch navigation remain functional. Avoid informational implementation details in the normal player flow; configuration belongs in operator documentation.

## Acceptance and truthful delivery

Tests must exercise real boundaries: original XML/numeric parsing, unauthorized and stale operations, source geometry and picking, actual runtime action/query semantics, interrupted responses, reconnect and saved-data preservation. Test fixtures are identified as fixtures. Browser evidence distinguishes live server, source-backed local execution and visual preview. No live account, city, lot or service may be called verified without a successful observed connection. Full game parity cannot be inferred from imports, visible controls or compilation.

Publish connected increments to the authorized Wonderland fork as PRs, with exact verification and any external integration prerequisite documented. Continue through the capability map until every row has implementation evidence or a concrete external blocker; never silently defer a surface.
