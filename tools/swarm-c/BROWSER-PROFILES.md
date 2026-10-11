# Browser verification profiles

The standard command remains `bash tools/swarm-c/run-browser-engine.sh`, with
`WONDERLAND_ENGINE` and `WONDERLAND_BACKEND` selecting the real artifact.

WebGL2 uses explicitly requested headless Chromium/SwiftShader. On Linux,
WebGPU page verification uses a **headed Chromium process inside Xvfb**. The
wrapper requires `xvfb-run` and sets `WONDERLAND_HEADED=1`; both independent GPU
picking and ordinary image/lifecycle runners receive that profile. The reports
record `headless: false` and describe the virtual display. This is software
rendering evidence, not a physical-device performance result.

## Reproduced failure and controlled correction

On Chromium 151.0.7922.34, run
[37436202450](https://github.com/rndrntwrk/wonderland-/actions/runs/37436202450)
rendered a 32×32 clear without Bevy. The existing explicit Vulkan/SwiftShader
configuration returned mapped bytes `[64,128,191,255]`, while the page screenshot
contained the document background `[153,68,47,255]`. The two reduced flag sets
failed to supply a usable adapter or complete mapping. All observations remain
in the diagnostic artifact; failures were not converted into passes.

Run [37436583753](https://github.com/rndrntwrk/wonderland-/actions/runs/37436583753)
then executed the identical script and pixel assertions with only the browser
changed to headed execution under Xvfb. The existing explicit configuration
returned **both** mapped bytes and ordinary page pixels `[64,128,191,255]`.
The reduced configurations still failed.

Commit `bf5363e21d45123cbc95a219255dbaa6cc793cfb` applies that measured profile to
the three existing test runners. It does not change shader code, source fixtures,
logical or physical pixel dimensions, ID maps, color tolerances, or assertions.
Its exact three-file diff was checked before publication. Full engine runs on
the corrected source remain authoritative; a passing clear control is not a
passing engine or a production release.

## Required evidence

The ordinary page color and exact physical-ID comparisons must still run.
Offscreen picks and direct/raw readback are additional checks, never substitutes
for a failed page screenshot. Native engine and audio jobs, source-reference
checks, and physical-browser/service acceptance keep their independent scope.
The historical headless WebGPU presentation failure remains recorded; headed
success does not retrospectively qualify the headless profile.
