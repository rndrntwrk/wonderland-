# Cloud browser build

The browser application is Rust/WASM, not a replacement JavaScript mockup.
These scripts restore the pinned build prerequisites after a disposable cloud
workspace is recreated. They install only inside `WONDERLAND_TOOLS_ROOT` and do
not change the user's shell profile or deploy anything.

```
bash tools/cloud/bootstrap-browser.sh
bash tools/cloud/build-browser.sh
```

The scripts require outbound access to official Rust, crates.io, npm and
Playwright distribution endpoints. Browser launch also needs the environment's
supported process/socket permissions. A successful compile is not browser QA.
`WONDERLAND_INSTALL_BROWSER=0` skips the browser download when using an already
available supported browser; it does not declare the browser checks passed.

The existing full browser journeys are in `apps/web-shell/tests/gpu/` and
`tools/native-browser/`. Use their documented fixture/service setup and preserve
their reports. Preview mode is not a hosted multiplayer acceptance result.

## Use the default browser from a new shell

The bootstrap runs as a child process, so its exports do not survive when it
returns. Before running a browser journey, restore both the module location and
browser download location in the current shell. Run this from the repository root
and reuse the same `WONDERLAND_TOOLS_ROOT` value passed to the bootstrap:

```sh
ROOT=$(pwd)
TOOLS="${WONDERLAND_TOOLS_ROOT:-$ROOT/.cloud-tools}"
TOOLS=$(cd "$TOOLS" && pwd)
export WONDERLAND_PLAYWRIGHT_MODULE="$TOOLS/browser/node_modules/playwright/index.mjs"
export PLAYWRIGHT_BROWSERS_PATH="$TOOLS/playwright-browsers"
export CARGO_HOME="$TOOLS/cargo" RUSTUP_HOME="$TOOLS/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
cargo run -p wonderland-world-view --example gpu_fixture --locked -- tests/output/source-gpu
GITHUB_SHA=$(git rev-parse HEAD) node apps/web-shell/tests/gpu/application.mjs
```

The application build must already exist. Other browser journeys still need
their documented services and fixtures. When using the explicit baseline profile
below, use its module and browser-directory exports instead of this default pair.

## Exact browser used for the 12 October baseline

The saved 39-scenario baseline used Playwright 1.56.1 and its Chromium 141
headless-shell revision 1194. The default bootstrap targets Playwright 1.62.1;
it is a separate browser profile and has not reproduced this baseline here.
Use this explicit fallback profile to reproduce the recorded browser:

```sh
ROOT=$(pwd)
TOOLS="${WONDERLAND_TOOLS_ROOT:-$ROOT/.cloud-tools}"
mkdir -p "$TOOLS"
TOOLS=$(cd "$TOOLS" && pwd)
npm install --prefix "$TOOLS/browser-baseline" --ignore-scripts --save-exact playwright@1.56.1
export PLAYWRIGHT_BROWSERS_PATH="$TOOLS/browser-baseline-binaries"
node "$TOOLS/browser-baseline/node_modules/playwright/cli.js" install chromium-headless-shell
export CARGO_HOME="$TOOLS/cargo" RUSTUP_HOME="$TOOLS/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
cargo run -p wonderland-world-view --example gpu_fixture --locked -- tests/output/source-gpu
GITHUB_SHA=$(git rev-parse HEAD) WONDERLAND_PLAYWRIGHT_MODULE="$TOOLS/browser-baseline/node_modules/playwright/index.mjs" WONDERLAND_CHROMIUM_EXECUTABLE="$TOOLS/browser-baseline-binaries/chromium_headless_shell-1194/chrome-linux/headless_shell" node apps/web-shell/tests/gpu/application.mjs
```

Run from the repository root after building. The raw original report omitted
GITHUB_SHA; its source field was null. The committed summary records the
restored Git parent as independently observed provenance, not an automatic
runtime source assertion. Source was not recompiled after the test-launcher-only
override. Its distribution file hashes matched all 145 files at review time.
