/**
 * Manual connected-layout regression checks against the controlled replay fixture.
 *
 * Prerequisites:
 *   - Node.js 20+ and Playwright 1.56.1+ with its Chromium browser installed.
 *   - A release web build: npm --prefix apps/web-shell run build
 *   - The controlled gateway:
 *       cargo build -p wonderland-browser-gateway --example controlled_replay
 *
 * Run from any directory:
 *   node /path/to/repo/apps/web-shell/scripts/check-connected-layout.mjs
 *
 * Environment overrides:
 *   WONDERLAND_QA_DIST       Built web directory (default: apps/web-shell/dist).
 *   WONDERLAND_QA_OUTPUT     Evidence directory (default: OS temporary directory,
 *                           wonderland-connected-layout-qa).
 *   WONDERLAND_QA_GATEWAY    Controlled-replay executable (default:
 *                           target/debug/examples/controlled_replay).
 *   PLAYWRIGHT_MODULE       Module name, file path, or file URL (default: playwright).
 *                           WONDERLAND_QA_PLAYWRIGHT_MODULE is also accepted.
 *   WONDERLAND_QA_SIZES      JSON viewport pairs, e.g. '[[390,844],[844,390]]'.
 *   WONDERLAND_QA_WIDTH      A single viewport width; also supply the height below.
 *   WONDERLAND_QA_HEIGHT     A single viewport height.
 *   WONDERLAND_QA_PORT       Loopback static-server port (default: 4191).
 *   WONDERLAND_QA_GATEWAY_PORT  Loopback controlled-gateway port (default: 18991).
 *
 * This script starts its own controlled gateway and uses test-only credentials.
 * It never targets a production gateway and does not send the retained chat draft.
 * It tests built assets without injecting source CSS. No dependencies are installed.
 */
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { extname, isAbsolute, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const repoRoot = fileURLToPath(new URL('../../../', import.meta.url));
const dist = resolve(process.env.WONDERLAND_QA_DIST || fileURLToPath(new URL('../dist/', import.meta.url)));
const output = resolve(process.env.WONDERLAND_QA_OUTPUT || `${tmpdir()}/wonderland-connected-layout-qa`);
const gatewayBinary = resolve(
  process.env.WONDERLAND_QA_GATEWAY ||
  `${repoRoot}/target/debug/examples/controlled_replay${process.platform === 'win32' ? '.exe' : ''}`,
);
const playwrightModule = process.env.WONDERLAND_QA_PLAYWRIGHT_MODULE ||
  process.env.PLAYWRIGHT_MODULE || 'playwright';
const playwrightSpecifier = isAbsolute(playwrightModule) || playwrightModule.startsWith('.')
  ? pathToFileURL(resolve(playwrightModule)).href
  : playwrightModule;
const { chromium } = await import(playwrightSpecifier);

function port(value, fallback, name) {
  const number = Number(value ?? fallback);
  assert(Number.isInteger(number) && number > 0 && number <= 65535, `Invalid ${name}: ${value}`);
  return number;
}

const staticPort = port(process.env.WONDERLAND_QA_PORT, 4191, 'static-server port');
const gatewayPort = port(process.env.WONDERLAND_QA_GATEWAY_PORT, 18991, 'gateway port');
assert.notEqual(staticPort, gatewayPort, 'Static server and gateway need different ports');
const origin = `http://127.0.0.1:${staticPort}`;
const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
const defaultSizes = [[1440, 1000], [390, 844], [320, 600], [844, 390]];
let sizes = defaultSizes;
if (process.env.WONDERLAND_QA_SIZES) {
  sizes = JSON.parse(process.env.WONDERLAND_QA_SIZES);
} else if (process.env.WONDERLAND_QA_WIDTH || process.env.WONDERLAND_QA_HEIGHT) {
  sizes = [[Number(process.env.WONDERLAND_QA_WIDTH), Number(process.env.WONDERLAND_QA_HEIGHT)]];
}
assert(
  Array.isArray(sizes) && sizes.length > 0 && sizes.every(size =>
    Array.isArray(size) && size.length === 2 &&
    size.every(value => Number.isSafeInteger(value) && value > 0)),
  'Viewports must be nonempty [width, height] pairs of positive integers',
);

const mime = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
  '.wasm': 'application/wasm', '.css': 'text/css', '.json': 'application/json',
  '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2', '.ttf': 'font/ttf', '.dat': 'application/octet-stream',
};
const delay = milliseconds => new Promise(resolveDelay => setTimeout(resolveDelay, milliseconds));
const near = (a, b) => Math.abs(a - b) <= 1;
const overlap = (a, b) => a.x < b.x + b.width - 1 && a.x + a.width > b.x + 1 &&
  a.y < b.y + b.height - 1 && a.y + a.height > b.y + 1;
const results = [];
let browser;
let gateway;
let gatewayError;
let gatewayLog = '';

await mkdir(output, { recursive: true });
await readFile(resolve(dist, 'index.html'));
const server = createServer(async (request, response) => {
  try {
    const path = decodeURIComponent(new URL(request.url, origin).pathname);
    const file = resolve(dist, path === '/' ? 'index.html' : `.${path}`);
    if (!file.startsWith(`${dist}${sep}`)) {
      response.writeHead(403).end();
      return;
    }
    const content = await readFile(file);
    response.writeHead(200, {
      'Content-Type': mime[extname(file)] || 'application/octet-stream',
      'Cache-Control': 'no-store',
    }).end(content);
  } catch {
    response.writeHead(404).end();
  }
});

async function saveResults() {
  await writeFile(resolve(output, 'results.json'), `${JSON.stringify(results, null, 2)}\n`);
}

async function waitForGateway() {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (gatewayError) throw gatewayError;
    if (gateway.exitCode !== null) throw Error(`Controlled gateway exited: ${gatewayLog}`);
    try {
      const health = await fetch(`${gatewayUrl}/health`, { signal: AbortSignal.timeout(1000) });
      if (health.ok) return;
    } catch {
      // The newly spawned controlled gateway may still be starting.
    }
    await delay(50);
  }
  throw Error(`Controlled gateway did not become ready: ${gatewayLog}`);
}

async function checkViewport(width, height) {
  const context = await browser.newContext({
    viewport: { width, height },
    reducedMotion: 'reduce',
    isMobile: width < 600,
    hasTouch: width < 600,
  });
  const page = await context.newPage();
  const checks = [];
  const errors = [];
  const expectedAborts = [];
  const warnings = [];
  const result = { width, height, passed: false, checks, errors, expectedAborts, warnings };
  page.on('pageerror', error => errors.push({ type: 'pageerror', message: error.message }));
  page.on('console', message => {
    const entry = { type: message.type(), message: message.text() };
    if (message.type() === 'error') errors.push(entry);
    else if (message.type() === 'warning') warnings.push(entry);
  });
  page.on('requestfailed', request => {
    const entry = { url: request.url(), message: request.failure()?.errorText };
    if (entry.message === 'net::ERR_ABORTED') expectedAborts.push(entry);
    else errors.push({ type: 'requestfailed', ...entry });
  });
  const record = (name, evidence) => {
    checks.push({ name, evidence });
    console.log(`${width}x${height}`, name, JSON.stringify(evidence));
  };
  const capture = async name => {
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({
      path: resolve(output, `${name}-${width}x${height}.png`),
      animations: 'disabled',
    });
  };
  const box = selector => page.locator(selector).boundingBox();
  const bounded = async (selector, name) => {
    const bounds = await box(selector);
    assert(bounds && bounds.x >= -1 && bounds.y >= -1 &&
      bounds.x + bounds.width <= width + 1 && bounds.y + bounds.height <= height + 1,
    `${name} outside viewport: ${JSON.stringify(bounds)}`);
    record(name, bounds);
    return bounds;
  };
  const clearControls = async () => {
    const controls = await page.locator('.source-world-tools button').evaluateAll(elements =>
      elements.map(element => {
        const rect = element.getBoundingClientRect();
        const hit = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
        return {
          name: element.getAttribute('aria-label') || element.innerText,
          clear: !!hit && (hit === element || element.contains(hit)),
          width: rect.width,
          height: rect.height,
        };
      }));
    assert(controls.every(control => control.clear),
      `Property controls covered: ${JSON.stringify(controls.filter(control => !control.clear))}`);
    record('Property controls remain exposed', controls);
  };

  try {
    await page.route('**/wonderland-config.json', route => route.fulfill({
      contentType: 'application/json',
      body: JSON.stringify({ version: 1, mode: 'connected', gateway_url: gatewayUrl }),
    }));
    await page.goto(origin);
    await page.getByLabel('Account name', { exact: true }).fill('controlled-player');
    await page.getByLabel('Password', { exact: true }).fill('test-only');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await page.getByRole('heading', { name: 'Choose your Sim', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Play as Controlled Alice', exact: true }).click();
    await page.getByRole('heading', { name: 'Controlled City', exact: true }).waitFor();
    await page.locator('.source-city-terrain[data-renderer="source-city-software-3d"]').waitFor();
    record('Page identity', { url: page.url(), title: await page.title() });

    await bounded('.connected-city-heading', 'City heading contained');
    await bounded('.connected-hud', 'City HUD contained');
    await bounded('.connected-discovery', 'Directory contained');
    const top = await page.locator(
      'button[aria-label="Sound settings"],button[aria-label="Player menu"],button[aria-label="Options"]',
    ).evaluateAll(elements => elements.map(element => {
      const rect = element.getBoundingClientRect();
      return { label: element.getAttribute('aria-label'), x: rect.x, y: rect.y, w: rect.width, h: rect.height };
    }));
    record('Top control alignment', top);
    assert(top.every(control => control.w === 40 && control.h === 40) &&
      top.every(control => near(control.y, top[0].y)), `Uneven top tools: ${JSON.stringify(top)}`);

    const directoryToggle = page.locator('.connected-discovery-toggle');
    if (await directoryToggle.isVisible()) {
      if (await directoryToggle.getAttribute('aria-expanded') === 'true') await directoryToggle.click();
      const search = page.getByRole('textbox', { name: 'Search this city', exact: true });
      await search.waitFor({ state: 'hidden' });
      const collapsedFocus = await box('.source-city-focus');
      await directoryToggle.click();
      await search.waitFor({ state: 'visible' });
      const expandedFocus = await box('.source-city-focus');
      assert(collapsedFocus && expandedFocus &&
        collapsedFocus.width * collapsedFocus.height > expandedFocus.width * expandedFocus.height,
      `Collapsed directory must free map space: ${JSON.stringify({ collapsedFocus, expandedFocus })}`);
      const retainedSearch = 'Retained city layout QA query';
      await search.fill(retainedSearch);
      await capture('01-city-directory-expanded');
      await directoryToggle.click();
      await search.waitFor({ state: 'hidden' });
      await directoryToggle.click();
      await search.waitFor({ state: 'visible' });
      assert.equal(await search.inputValue(), retainedSearch, 'City search survives Hide/Show');
      await directoryToggle.click();
      await search.waitFor({ state: 'hidden' });
      assert.equal(await directoryToggle.getAttribute('aria-expanded'), 'false');
      record('Directory Hide/Show preserves search and restores map space', {
        collapsedFocus, expandedFocus, retainedSearch, finalState: 'collapsed',
      });
    }
    await capture('01-city');

    await page.getByRole('button', { name: 'Select Controlled Source Lot', exact: true }).click();
    await page.getByRole('heading', { name: 'Property', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Visit', exact: true }).click();
    await page.getByRole('region', { name: 'Connected property', exact: true }).waitFor();
    await page.locator('.connected-world canvas[data-renderer="source-software-3d"]').waitFor();
    await bounded('.source-world-header', 'Property header contained');
    await bounded('.source-world-tools', 'Property toolbar contained');
    await clearControls();
    const groups = await page.locator('.source-world-tools>.source-control-group').evaluateAll(elements =>
      elements.map(element => {
        const rect = element.getBoundingClientRect();
        return {
          name: element.getAttribute('aria-label'), x: rect.x, y: rect.y,
          width: rect.width, height: rect.height,
          scrollWidth: element.scrollWidth, clientWidth: element.clientWidth,
        };
      }));
    assert(groups.every(group => group.scrollWidth <= group.clientWidth + 1),
      `Toolbar group overflows: ${JSON.stringify(groups)}`);
    record('Property groups do not overflow', groups);
    await capture('02-property');

    await page.getByRole('button', { name: 'Needs', exact: true }).click();
    await bounded('.connected-world-needs', 'Needs panel contained');
    await clearControls();
    const needs = await page.locator('.connected-source-needs label').evaluateAll(elements =>
      elements.map(element => ({
        label: element.querySelector('span').textContent,
        value: element.querySelector('progress').value,
        x: element.querySelector('progress').getBoundingClientRect().x,
      })));
    assert.equal(needs.length, 8);
    assert(needs.every(need => near(need.x, needs[0].x)), 'Eight need bars must align');
    record('Eight need bars align', needs);
    await capture('03-needs');
    await page.getByRole('button', { name: 'Close needs', exact: true }).click();

    await page.getByRole('button', { name: 'Player menu', exact: true }).click();
    await bounded('.connected-player-menu .player-menu-panel', 'Player menu contained');
    await capture('04-player-menu');
    await page.getByRole('button', { name: 'Chat', exact: true }).click();
    await page.getByRole('button', { name: /^Lot chat/ }).click();
    const log = page.getByRole('log', { name: 'Lot messages', exact: true });
    await log.getByText('Hello from the controlled lot.', { exact: true }).waitFor();
    assert.equal(await log.getByText('Hello from the controlled lot.', { exact: true }).count(), 1);
    const chat = await bounded('.connected-panel-chat', 'Compact chat contained');
    const composer = await bounded('.connected-chat-compose', 'Chat composer contained');
    assert(composer.y + composer.height <= chat.y + chat.height - 1,
      `Chat composer clipped: ${JSON.stringify({ chat, composer })}`);
    await clearControls();
    // Re-measure after opening chat: responsive controls can move with the panel.
    const currentToolbar = await box('.source-world-tools');
    assert(currentToolbar && !overlap(chat, currentToolbar), 'Compact chat overlaps property toolbar');
    await page.getByLabel('Say something to the lot', { exact: true }).fill('Retained layout QA draft');
    await capture('05-chat');
    const expandChat = page.getByRole('button', { name: 'Expand chat', exact: true });
    if (await expandChat.isVisible()) {
      await expandChat.click();
      await bounded('.connected-panel-chat', 'Expanded chat contained');
      await capture('06-chat-expanded');
    } else {
      record('Expand chat omitted when the short viewport has no larger panel', true);
    }
    await page.getByRole('button', { name: 'Close panel', exact: true }).click();
    await page.getByRole('button', { name: 'Player menu', exact: true }).click();
    await page.getByRole('button', { name: 'Chat', exact: true }).click();
    assert.equal(await page.getByLabel('Say something to the lot', { exact: true }).inputValue(),
      'Retained layout QA draft');
    record('Lot draft retained after panel close/reopen', true);
    await page.getByRole('button', { name: 'Close panel', exact: true }).click();

    await page.getByRole('button', { name: 'Needs', exact: true }).click();
    await page.getByRole('button', { name: 'Open profile', exact: true }).click();
    await page.getByRole('heading', { name: 'Profile', exact: true }).waitFor();
    await bounded('.connected-panel', 'Profile panel contained');
    await capture('07-profile');
    assert.equal(errors.length, 0, `Browser errors: ${JSON.stringify(errors)}`);
    result.passed = true;
  } catch (error) {
    record('FAILURE', error.message);
    try { await capture('failure'); } catch { /* Keep the original failure if the page closed. */ }
    throw error;
  } finally {
    results.push(result);
    await context.close();
    await saveResults();
  }
}

try {
  await new Promise((ready, reject) => {
    server.once('error', reject);
    server.listen(staticPort, '127.0.0.1', ready);
  });
  gateway = spawn(gatewayBinary, [], {
    env: {
      ...process.env,
      WONDERLAND_REPLAY_BIND: `127.0.0.1:${gatewayPort}`,
      WONDERLAND_REPLAY_BROWSER_ORIGINS: origin,
    },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  gateway.on('error', error => { gatewayError = error; });
  gateway.stderr.on('data', data => { gatewayLog += data.toString(); });
  await waitForGateway();
  browser = await chromium.launch({ headless: true });
  console.log('Connected-layout QA', { repoRoot, dist, output, origin, gatewayUrl, browser: browser.version() });
  for (const [width, height] of sizes) await checkViewport(width, height);
  console.log(`Passed ${results.length} viewport runs. Evidence: ${output}`);
} finally {
  await browser?.close();
  server.closeAllConnections();
  if (server.listening) await new Promise(ready => server.close(ready));
  gateway?.kill('SIGTERM');
  await writeFile(resolve(output, 'gateway.log'), gatewayLog);
}
