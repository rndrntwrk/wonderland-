# Facade implementation ledger — base 544ceb2b5e0faf99f723ad84d3ac1b91eab8dd36

Scope: loaded-world FSOf job, actual source inspector action, native CLI, source
lighting/mask preservation and consumer evidence. Existing original C derivative
modules are reused unchanged; source parity still has its own corpus/device gates.

Ruling: The old unpublished complete C source is not recoverable. This work is a
new implementation from PR32 and published PR12 modules, not proof that the old
avatar/Full2D/facade bundle existed on GitHub. Those other paths remain open.

Ruling: The facade contains the supplied light state only. Night is absent rather
than synthesized. Cost: consumers needing paired day/night must supply and
separately qualify a second source lighting state.

Ruling: Keep the existing pinned client flate2 1.1.10 dependency. The lockfile gains
one local dependency edge; no registry version changes. Requiring PR12's older
isolated flate2 resolution would duplicate or change the client dependency graph.

Implementation tests were written before the job/UI. Three initial valid-source
job cases failed against the unimplemented boundary. The assembly test failed
while the inspector lacked its actual facade component.

Review finding fixed: writer-collision cleanup removed a filename owned by a
competing writer. A failing native test preserved the witness. Cleanup now tracks
successful exclusive creates and removes only those files; all worker cases pass.

Review finding fixed: tile-less upper roofs used the ground lightmap fallback.
A non-vacuous upper-roof scene demonstrated different atlas cells and failed the
GPU-packet check. The renderer now uses each part's declared source level.

The portal facade acceptance fixture includes an exterior wall camera and an
unmasked control: top-down views cannot establish visibility of a vertical sheet.
Normal masks keep the disabled final group hidden; the portal final group must
have nonzero visible pixels and fewer pixels than the unmasked control.

Local final review is author self-review; no independent reviewer was available
in this execution environment. Test gates and their exact logs are retained in
the handoff. Actual hosted application outcomes are recorded only after running
and reading them. No pending check is converted to a pass.
