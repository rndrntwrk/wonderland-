# PR50 native canvas observation repair

## Exact starting point and observed failure

Base: `df73e9d951e98a8705cc726ad78851c0642b0c7a`, tree
`67e511db1d03c2a81bf00e963aa2ecedb0eb6e8e`. This is a test-observation
correction above the published combined native/C-renderer implementation,
not another application rewrite.

Native browser run `37756994640` built the real release, passed the sixteen
baseline player checks and then failed `verify-avatars.mjs`' disconnected
frame comparison. Four avatar checks had completed; later browser journeys
were skipped. The separate Browser UI, native wire, live-session and pose
workflows passed, but cannot substitute for combined native-player acceptance.

The integration changed `canvasPixels` from underlying Canvas2D pixels to a
locator screenshot. A locator screenshot captures the composited page clipped
to the canvas rectangle, including overlapping tick text, menus and notices.
The native canvas fills the screen behind those elements. That conflated
changing HUD pixels with avatar animation and pose equality. In particular,
later idle tick comparisons and packet-grouping comparisons must not depend
on the displayed tick text. A matching DOM counter is not animation evidence.

## Two independent observations, neither substituted for the other

1. **Full accepted color framebuffer:** a test-only MutationObserver is installed
   before navigation. It observes the existing `data-frame-generation` marker
   published by `WorldGpuOwner.install` after its actual color draw. In that
   microtask it uses the standard canvas PNG encoder before the non-preserved
   default drawing buffer may be discarded. It does not create a context,
   invoke a draw/capture/export helper, dispatch resize, read an offscreen ID
   attachment, alter CSS, or alter browser/simulation time.
   Every decoded RGBA byte contributes to the compact SHA-256 identity.
   No pixels are masked or ignored in this equality comparison.
2. **Ordinary compositor:** unmodified page/canvas screenshots, visible red
   synthetic-avatar motion, actual pointer selection, hidden-avatar disappearance,
   and existing complete screenshots remain separately required. The visible
   red witness excludes changing HUD text. This fixture-specific red signature
   is not used instead of the full framebuffer equality check. A blank or frozen
   compositor cannot pass merely because a stored framebuffer is correct.

The observation retains only the latest canvas/PNG, with exact ownership,
generation and dimensions. It invalidates on loss/disposal/removal/size changes,
rejects malformed/oversized samples and refuses blank pose observations.
Limits are 1,048,576 pixels, 4,096 pixels per dimension and 8 MiB encoded PNG.
The native synthetic avatar must remain nonempty. A changed frame is never
silently replaced with an earlier matching one.

Waiting for already-scheduled animation frames is not a forced redraw.
An unsettled current surface times out rather than comparing an unrelated
frame. Real action-inspector buttons are used where necessary to expose the
avatar; no overlay is programmatically hidden by style injection.

## Regression and execution

The two initial Node regressions failed against the old helper: changing only
HUD pixels changed pose identity, while a changed framebuffer behind an
unchanged overlay was falsely considered identical. Assembly regressions also
failed before all four witnesses installed the observer before navigation.
A later regression exposed an unrelated canvas evicting native evidence; that
notification is now ignored. Existing framebuffer/image/authority thresholds
are not relaxed.

`verify-canvas-observation.mjs` uses an explicitly authored 64x64 WebGL2 fixture
to prove real non-preserved-buffer capture timing, independent HUD changes,
actual movement, deliberate blank-compositor rejection and genuine context
loss/restoration. It is not the Rust application. Both application source
imports/exports and all eight native-player journeys are then run against
the **same freshly built Trunk distribution** by Native browser CI.

Independent browser steps continue after another assertion fails, provided
the build/browser setup succeeded. Failures still fail the workflow. There
is no continue-on-error or fabricated success result.

Commands:

```sh
node --test tools/native-browser/canvas-*.test.mjs apps/web-shell/tests/gpu/*.test.mjs
node tools/native-browser/verify-canvas-observation.mjs
```

The normal Native browser workflow supplies the existing locked Playwright
dependency, compiled Rust services, fixture inputs and real release. No new
registry dependency or product renderer/VM/authority code is changed.

## Qualification

Local Chromium refused localhost with `ERR_BLOCKED_BY_ADMINISTRATOR` before
any test, so the local browser attempt is recorded as zero checks, not success.
Fresh hosted helper and full application results must be inspected before
marking this repair accepted. Historical PR50 failure remains retained.

This does not complete Full2D/hybrid, native lighting transport, missing
content/providers, physical-device qualification, independent review, Swarm E
services, or the full game. No merge or deployment is included.
