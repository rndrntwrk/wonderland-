# Independent Creator-web session review by runtime lane

Scope: `tools/creator-web/src/session.rs`, its session tests, and directly relevant
Creator resource/sprite guards and UI messages. Runtime lane made no production
edits in Creator or Creator-web. Root owns the implementation and its build gates.

## Disposition

No remaining blocking finding from this review. Rechecked the final source after
root added actual SPR2 admission, displayed version separated from the immutable
EditGuard contract, bounded typed preview and package exports, raw fallbacks, and
unassigned-language display. This is a source/contract review plus inspection of
root's executable regression assertions; runtime lane did not claim an independent
browser automation pass or rerun root's entire session suite.

## Findings and resolution

1. **Malformed/versioned SPR2 inspection (Important):** The original branch
   returned `sprite_package:true` without decoding, while the library guard has no
   SPR2 version discriminator. A malformed one-byte payload could be labelled
   Unversioned without a raw preview/error. Root now calls actual bounded
   `ResourceDocument::export_sprite` to validate planes, palettes and exact
   representability; failures return raw bytes plus an error. Displayed version is
   independent of EditGuard, preserving transaction compatibility. Regressions
   cover malformed data and real SPR2 versions 1000/1001.
2. **Session/inspection memory composition:** Inspection/export initially passed
   the full codec allowance with retained document/history still live. Root now
   narrows operation limits, reserves inspection working storage, preflights JSON
   and typed record/text expansion before Value construction, and falls back to
   bounded raw bytes. Sprite imports/exports share a 1 MiB browser package limit.
   The actual Creator importer already accounts the retained SpritePackage, so no
   duplicate missing-package finding was raised. Regression rejects oversized
   escaped text before constructing typed output and preserves exact export.
3. **Unvalidated export wording:** Import preserves malformed/unknown resource
   payloads and export clones the current IFF source. Root changed the success
   text from a semantically validated-document claim to "current document".
4. **Unassigned source strings (Important display completeness):** Format -3
   retains language codes above 20 in `Strings.unassigned`. The initial inspector
   omitted them and could show no entries. Root includes them in bounds and JSON,
   labels their rows Unassigned, and explains that typed editing addresses the
   recognized sets. The language-21 regression checks text/comment and byte-exact
   source preservation.

## Checked invariants

- Edits stay in a detached ResourceDocument until encoding, digest and history
  admission succeed. Failed edits retain source bytes, selection and history.
- History reserves max(before,after) for reversibility. Undo/redo validates a
  candidate before popping history, then evicts farthest revisions as needed.
- Successful edit and restore invalidate pending read tickets; a late async read
  cannot replace an edited document. A checked monotonic ticket distinguishes
  consecutive imports; failed candidate import preserves the previous document.
- Source/resource hash guards are recomputed through the real Creator library.
  Displaying a sprite version never fabricates a mismatching edit guard.
- Unknown payloads retain raw bounded inspection and exact export. Unsupported
  typed previews fail explicitly rather than truncating the authored source.
- Current UI CFG reflects decoded branch bytes; it is not an executed trace.
  Creator-web still does not hold a live A runtime or bind SimDebugProvider. The
  runtime lane provides those APIs separately with the exact A boundary recorded
  in docs/swarm-b/runtime-bridge.md and runtime-extension-contract.md.

The session's byte budgets are conservative operation bounds, not a measurement
of browser DOM/JavaScript/process resident memory. Root owns final native/WASM,
formatting, static and browser interaction verification.


## Follow-up: portable workbench session

Read-only review of `tools/creator-web/src/workbench.rs` at SHA-256
`fad5322d55671595a7b94f76b8a521e8a638220b47f2c4a9c847a5da8e33cac0`, plus the actual Creator JSON, asset, city image,
mesh/glTF and patch-view entry points it calls. No blocking finding remained in
the reviewed aggregate admission and session-safety scope. The five inline test
cases were inspected; their execution remains root's gate. The new graphical
workbench components were still being authored and were outside this review.

Checked behavior:

- `operation_limits` subtracts current source, skeleton, every patch, both history
  stacks and their retained capacities, plus a fixed graphical allowance. Upload
  admission separately reserves twice the incoming bytes. Actual codec admission
  then enforces its existing JSON/asset expansion, image pixel and mesh/glTF
  graph/binary working-copy bounds against the remainder.
- Imports bind generation, exact declared byte size, source hash and compatible
  mode before publication. Invalid candidates retain the existing source and
  history. A superseded completion does not consume the newer ticket. Cancelling
  an older ticket leaves a newer pending read intact.
- Publication validates the candidate before changing source or history. It
  reserves max(previous,candidate) capacity for the latest reverse operation;
  undo/redo validate before moving a revision and retain the newest reverse
  operation while evicting older entries if needed. No-op edits keep exact
  original bytes and create no revision.
- Successful source edits, undo/redo, patch reorder/removal and patch source-name
  changes invalidate pending reads. Attachment completion consumes its checked
  ticket. Exchange imports retain the real source hash and, for animation,
  attached skeleton hash used by Creator's protected graph comparison.
- Patches have a count and aggregate byte cap, retain explicit user/default
  provenance, reject duplicate basenames and use the actual ordered resolver.
  Removing an invalid or incompatible attachment remains available without
  needing to resolve the effective document first.
- Typed JSON edits remain inside actual Creator parsing, representation checks
  and binary reopen validation. City paint and roads mutate detached decoded
  images and publish only encoded/validated candidates. Source export retains the
  exact current source bytes; inspection/export failures do not alter history.

One minor early-admission issue was reported and corrected by root during review:
`begin_read` initially admitted OBJ for an Animation source through the generic
exchange arm, though import rejected it safely. The reviewed code restricts that
arm to GLB/glTF; OBJ now requires FSOm at admission.

The 32 MiB limit is a conservative budget for one workbench panel's admitted
operations, not a measurement or guarantee of total browser/WASM/DOM resident
memory. Final UI retention, asynchronous file-reader behavior, native/static/WASM
gates and browser interactions remain coordinator-owned.


## Follow-up: graphical workbench source review

Read-only review of `tools/creator-web/src/app/workbench.rs` at SHA-256
`289c717fb40091d7f8638e0acfb3dbbf4561ec7da53b0603f15de8c6f08214f3`, its panel mounting in `app.rs`, and the
`public/bootstrap.js` unload handler. No builds or browser commands were run by
runtime lane for this review. Root owns executable reproduction and final fixes.
Runtime lane's production source remains frozen.

### Findings sent to the implementation owners

- **U1 — Advanced draft source binding (Important):** JsonTools retains its JSON
  draft when the source is replaced or history is restored, but obtains the
  expected source hash at submission. A draft composed against source A can
  therefore apply to source B if the paths happen to exist. Capture the draft's
  actual source revision, or reset it explicitly on source hash changes, before
  using the real guarded model. This finding was sent immediately to root and
  Creator; it must be resolved before graphical draft-binding signoff.
- **U2 — Skeleton attachment unload state (Minor):** Patch attachments and exact
  patch source-name changes contribute to dirty state. An attached skeleton does
  not, so an otherwise unmodified animation plus its newly attached companion
  has `data-dirty=false`. If companion selection is retained authoring state, it
  should participate in the unload guard. No loss of changed source bytes was
  claimed for this case.
- **U3 — Transform selection lifetime (Important workflow issue):** AssetTools
  dynamically constructs TransformTools through `ui.kind()`, which observes
  every UI revision. Each successful source edit reconstructs the component's
  group/geometry/element signals at zero. Memoizing the actual kind or limiting
  component recreation to real source/kind replacement preserves nonzero
  selections while its existing effect refreshes the stored values. A focused
  nonzero-index transform workflow was recommended to the browser-gate owner.

Creator independently identified optional absent/null Neighborhood Description
preservation and FSOm visible-bound recomputation refinements. Those are owned by
Creator/root and are not represented here as independently reproduced findings.

### Checked behavior without further blocking findings

- Ordinary editable source fields use `field_text`, retaining the complete value.
  The clipped `value_text` helper is used for overview/preview strings. The
  64 KiB JSON preview is separate from the source-backed document and edit data.
- Mesh/FSOm source codecs reject non-finite floats and out-of-range triangle
  indices. The SVG projection therefore receives finite binary32 coordinates,
  promoted to binary64 for bounds/projection, and validated indices. Its output
  is capped at 1,500 segments and is explicitly labelled as stored geometry
  before skinning/material rendering. No fabricated runtime rendering is claimed.
- Each panel supplies its own WorkbenchUi through a scoped Provider. All panels
  stay mounted while navigation changes visibility, retaining separate documents,
  histories and reactive state. No runtime context is substituted for authoring.
- Authoring controls, history controls and export actions are disabled during
  file reads. Read completion checks the current model ticket before copying
  bytes or publishing, checks the actual ArrayBuffer size and cancels only its
  own ticket. Superseded reads leave a newer read to complete its own busy state.
- The unload handler queries every dirty element, including hidden workbench
  panels. It therefore protects committed source edits and patch composition
  when the user switches to a different panel. U2 records the companion-input
  exception precisely.
- City preview uses actual decoded pixels, limits its visible image to 512 by
  512, and revokes previous/current object URLs on refresh/cleanup. Model budgets
  reserve a separate graphical allowance; this is not a guarantee of total
  browser process memory or garbage-collection timing.

This section records source findings and review coverage. It does not assert the
new graphical workflows passed their pending coordinator browser gates.


## Graphical findings U1/U2/U3: source recheck disposition

**U1, U2 and U3 are resolved in the reviewed source. No blocking finding remains
from those three requested corrections.** This is a read-only source recheck;
runtime lane ran no build or browser command. Root's final graphical regression
gate remains the authority for browser execution results.

Reviewed snapshots:

- `tools/creator-web/src/app/workbench.rs`: `3970515d13bb34cb03c230db8bc116398cc4111e0e835d1c191b3533a60b67e7`
- `tools/creator-web/src/workbench.rs`: `c7804b0c31936dcf0b584fc6a639ac459fab131e7c8b4fef08322bbf60512056`

**U1:** JsonTools initializes and stores `draft_guard` with the current source
hash. Submission passes that captured value to the guarded model. A revision
effect compares the actual source hash and explicitly clears the previous draft
when it changes, updating the guard and showing a clear notice. Before the effect
runs, an old captured hash still causes the actual model guard to reject a stale
submission; it cannot silently acquire the replacement source's hash. Unchanged
hash revisions do not clear a draft needlessly.

**U2:** WorkbenchSession dirty state now includes `skeleton.is_some()`. A
successful skeleton attachment updates the UI revision and its existing
`data-dirty` binding, so the existing all-panel unload handler sees the companion
selection. Opening a replacement source still clears the old attachment through
the established source-import transaction.

**U3:** AssetTools now memoizes the actual optional asset kind. Its dynamic
TransformTools construction subscribes to that memo instead of every source
revision. An unchanged kind therefore retains the same form and its selected
group, geometry and element. The retained form's own revision effect still
reloads current stored vector values. A genuine kind change replaces the form as
required.

The coordinator's saved old-bundle browser evidence was inspected:
`creator-workbench-browser-red.json` reports the stale advanced draft failure,
nonzero transform selection resetting to zero, and the separately author-found
Neighborhood no-op preservation failure. This confirms the U1/U3 source findings
were observable browser defects. No green browser result is inferred here from
source inspection, and the Neighborhood/bounds helper work was not expanded into
an additional review in this pass.
