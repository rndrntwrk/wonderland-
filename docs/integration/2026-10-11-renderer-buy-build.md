# Renderer and native Buy/Build integration

This candidate combines PR #50 (`aff5e2b3c4b50be2202aa133af092304e554d19f`)
with recovered PR #55 (`294036d264901100ee3e90e0ba362146803fc268`).
It preserves the later native checkpoint-tail and construction changes.

## Integration choices

- Retain both native construction modules and the renderer facade modules.
- Keep all existing workflow journeys, source fingerprints and evidence reports.
- Retain the renderer's completed-frame observer and ordinary compositor avatar
  visibility checks. Do not compare discarded WebGL default buffers as evidence.
- Upgrade the recovered checkpoint-tail journey to the same observation contract.
- Construction readiness accepts the real GPU post-draw observation. Actual tile
  clicks wait for the quote action to become enabled: GPU picking is asynchronous,
  so checking an already-visible disabled button immediately cancels pending picks.
  No synthetic hit, redraw, timer override or product-state mutation is used.

The first integrated run retained its failed result: its readiness helper only
accepted the old software renderer. A second run reached the catalogue but exposed
that synchronous-selection assumption. After these harness corrections, the
complete original construction journey passed against the unchanged product build.

## Fresh cloud execution on 11 October 2026

Pinned Rust 1.99.0, Trunk 0.21.14, Playwright 1.56.1 and Chromium 141.0.7390.37.
Native gateway/runtime examples compile; optimized browser and worker WASM build;
workspace formatting passes. Real browser acceptance:

| Journey | Checks | Result |
| --- | ---: | --- |
| Buy/Build, price consent, committed object, lost-reply recovery, mobile touch | 17 | Pass |
| Avatar resources, rendering and recovery | 10 | Pass |
| Ended pose retention | 6 | Pass |
| 1/3/18-tick pose batches | 8 | Pass |
| Checkpoint ending inside an 18-pose recovery tail | 8 | Pass |
| Real receipt timers and late-result cleanup | 7 | Pass |
| Accepted native audio and device cleanup | 14 | Pass |
| Player actions, cancellation, reconnect and responsive controls | 16 | Pass |
| Completed framebuffer and compositor observation | 6 | Pass |

All reports above have no page/console errors. Construction includes widths
320, 390, 844 and 1440; screenshots of the actual mobile construction and
checkpoint-recovery views were inspected. These are authored original-format
fixtures, controlled authentication and RAM-only construction accounting.
They do not establish original commercial-art coverage, durable purchases,
production authentication or independently authenticated multiplayer users.

## Remaining acceptance

The source-application GPU import/export journey could not launch full Chromium
locally: its profile socket creation returned EPERM. The permitted escalation
failed in platform bubblewrap mount setup before execution. This is unverified,
not a pass and not a product failure diagnosis. The existing hosted Native browser
workflow runs that exact journey on the packaged release; its result must be read
before merge acceptance. Combined audio/avatar and terrain variants, all original
content/device coverage, and independent review also remain distinct gates.
No main-branch merge or deployment is claimed.
