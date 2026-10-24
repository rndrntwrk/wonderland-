# Native Property Directory Implementation Plan

**Goal:** Select a Sim, search the authenticated configured properties, inspect a property, enter through existing source-owned admission, leave back to the directory, and repeat safely after an interrupted reply.

**Architecture:** Reuse `native-account.mjs` roster destinations and the existing Rust account screen, directory-row/placard design, and authoritative enter/leave path. Expose only the avatar/lot identity of an already retained pending entry so UI cannot silently switch targets during an ambiguous result. Do not add an invented city, properties, backend directory service, presence, ownership, map imagery or purchase rights.

**Tech stack:** Existing Leptos/Rust UI, native account JavaScript provider, pinned offline WASM build, existing two-account Playwright/native fixture. Browser plugin is unavailable in this executor; use the established sandbox-enabled Playwright path. No dependencies, agents or provider changes.

**Spec:** Parent's 24 October instruction selects the smallest actionable native directory journey when retained content cannot support a real-object needs cycle. The retained browser cohort contains avatar content only; original Timer artwork is separately missing. Full city, multi-lot orchestration and real-object needs remain open.

## Tasks

- [x] Add a behavioral failing test: after a lost entry response the published pending selection is the exact avatar/lot, exposes no operation/token, rejects changing target, and clears only after confirmed admission/sign-out.
- [x] Add the minimal sanitized pending-entry snapshot. Preserve the existing exact operation/retry and authority implementation; run owned-account regressions.
- [x] Replace native configured-property buttons with searchable directory rows and a source-location/details placard using the existing chrome. Pending entry fixes both selected Sim and property and offers an explicit identical retry. No client-generated identity or authority.
- [x] Save source; run targeted WASM check and one matching offline release package. Reuse the verified native host because native/service inputs are unchanged.
- [x] Run the existing local native browser journey: search empty/matching, open/close property details, actual entry/animated actor, leave/return, deliberately lost committed entry reply and same-operation retry, mobile geometry and sign-out. Preserve failure evidence; do not fabricate successful states.
- [ ] Review scoped source, update truthful full-game inventory/evidence, publish a new draft stacked on #84 and verify full remote postimages. No merge/deployment.

## Review focus

Stale directory records must never authorize entry; the provider/server retain their checks. Pending identity must remain visible without exposing secrets. Late replies after sign-out cannot restore selection. Filtering must dismiss hidden details. Desktop/mobile controls and status must remain reachable. Creator, chat, Timer and movement implementation stay intact. No independent review is claimed under the one-task/no-additional-agent direction.

Implementation, responsive correction and rendered qualification are complete at `fbd8ae16`. The final publication receipt/PR records delivery and remote verification; publication is not inferred from this pre-publication plan.
