# Graphical Creator implementation and verification

The six-workbench application in `tools/creator-web` is Rust/Leptos compiled to WebAssembly. It invokes the real Creator and legacy-format libraries. Native file selection supplies the bytes; browser downloads contain the resulting source or interchange bytes. Its controls do not synthesize a runtime/provider response.

## Independent review and demonstrated corrections

The runtime reviewer separately examined the IFF session and portable workbench. Findings corrected before the final gate include invalid SPR2 authorability claims, conflation of sprite layout version with edit-guard version, aggregate preview admission, omitted unassigned STR# language entries, history reversal after resource growth, and OBJ admission for the wrong source kind.

A separate graphical review caught retained advanced drafts being rebound to a replacement source, transform controls being recreated after ordinary edits, and skeleton attachments omitted from unsaved-work state. The author also corrected optional neighborhood descriptions being inserted by an unchanged form and visible FSOm bounds not being recomputed with typed position edits. The coordinator's browser regressions first observed the old draft changing the new source, the no-op neighborhood export changing bytes, and element 1 resetting to 0; the corrected browser subsequently passes each case.

The formats reviewer independently reproduced and closed Creator-core allocation faults in glTF and both OBJ directions, aliased animation sample conflicts, and BMP32 interoperability. Core admission and graphical session admission are separate layers; both reserve retained data and bounded candidate work.

## Functional browser evidence

The browser runner exercises 25 workflows. Fourteen cover the IFF editor, seven cover the new workbenches, three reproduce the late review findings, and one blocks the application module to check startup failure handling. The final combined runner rebuilds the application and reruns this suite; its machine-readable report and actual artifact digest are linked from `REMAINING-VERIFICATION.md`.

The checks use literal independently assembled IFF, SPR2, PIFF, purchase-reference, JSON and BMP fixtures. A 78-byte hand-built compressed FSOm triangle exercises typed transform, bounds and source-bound GLB/OBJ round trips. The purchase-reference fixture contains `18364758544493064720`, above JavaScript's safe integer range; a gender edit preserves the exact 64-bit binary identity. Assertions inspect exported envelopes, fields, pixels, source hashes and provenance, rather than relying on screenshots.

The suite covers malformed/oversized uploads, failed edits, stale source packages, no-op exports, metadata/add/remove, empty-file undo/redo, retained independent histories, official/user patch precedence, exact source-name matching, source-order nearest ties, categorical pixel rejection, actual reciprocal road bytes and 390-pixel layouts. Focus assertions wait for the actual requested animation-frame focus change. No fixed sleep substitutes for a state observation. The successful run has no uncaught page errors or external resource requests.

## Visual fidelity and responsive decisions

The implementation follows the generated Creator concept: a dark resource rail, light work surface, indigo selection/actions, native file controls, BHAV instruction table/control flow, source details and a compact status bar. Locally bundled Inter replaces the concept's illustrative font rendering; an explicit SVG information icon replaces an unsupported glyph. DOM and SVG implement all controls and the graph.

The principal intentional additions are the six-tool navigation and persistent workbench panels. Their source forms, tables, map/geometry previews and revision/history details use the same spacing, colors, borders and focus treatment. The IFF concept's illustrative opcode names and values are replaced by actual decoded numbers and branch destinations. Unknown or invalid payloads retain bounded raw inspection and exact preservation rather than an unsupported semantic claim.

The coordinator visually inspected the concept and rendered desktop together, then inspected the final desktop IFF, desktop upgrade and 390-pixel neighborhood screens. Mobile navigation scrolls within its own bar; forms, previews and source details stack. All tested panels fit the page width. Fields keep full source values while overview text is bounded. Screenshot content consists of authored test fixtures.

## Qualification boundary

This verifies working source-file authoring, local session behavior and the tested desktop Chromium engine. It does not qualify physical mobile devices, every browser engine, a screen reader, live animation playback, production editing privileges or a running-lot debugger. The runtime extension contract lists the actual A interfaces still needed for instruction pause/step/resume and executed traces. Full skinning/material rendering and arbitrary scene import/retargeting remain separately stated tool capabilities.
