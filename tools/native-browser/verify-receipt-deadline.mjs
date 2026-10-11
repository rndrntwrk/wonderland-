// Actual release application + real Rust authority. No fake browser timers,
// injected UI/CSS, altered WASM, or fabricated server state. The fixture omits
// exactly one correlated receipt while accepted state and later ticks continue.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { chromium } from 'playwright';
import { createFixture } from './peer.mjs';

const root = resolve(process.env.WONDERLAND_NATIVE_QA_OUTPUT ?? '/tmp/wonderland-native-browser-evidence');
const output = resolve(root, 'receipt-deadline');
const dist = resolve(process.env.WONDERLAND_NATIVE_QA_DIST ?? 'apps/web-shell/dist');
await mkdir(output, {recursive: true});
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const digest = value => createHash('sha256').update(value).digest('hex');
let fixture, browser, page, gateway;
let gatewayLog = '';
const result = {schema: 1, passed: false, checks: [], screenshots: [], errors: [], warnings: []};
function record(name, evidence) { result.checks.push({name, evidence}); console.log(name, JSON.stringify(evidence)); }
async function capture(name) {
  const path = resolve(output, name + '.png');
  await page.screenshot({path, animations: 'disabled'});
  result.screenshots.push({file: name + '.png', sha256: digest(await readFile(path))});
}
async function waitFor(predicate, message, limit = 3000) {
  const end = performance.now() + limit;
  while (!(await predicate())) {
    if (performance.now() >= end) throw Error(message);
    await delay(50);
  }
}
async function enter() {
  await page.goto(fixture.origin);
  await page.getByLabel('Account name', {exact: true}).fill('controlled-player');
  await page.getByLabel('Password', {exact: true}).fill('test-only');
  await page.getByRole('button', {name: 'Sign in', exact: true}).click();
  await page.getByRole('heading', {name: 'Choose your Sim', exact: true}).waitFor();
  await page.getByRole('button', {name: 'Play as Controlled Alice', exact: true}).click();
  await page.getByRole('heading', {name: 'Controlled City', exact: true}).waitFor();
  await page.getByRole('button', {name: 'Select Controlled Source Lot', exact: true}).click();
  await page.getByRole('button', {name: 'Visit', exact: true}).click();
  await page.locator('.native-lot[data-native-live="true"]').waitFor();
  await page.locator('.native-lot canvas').waitFor();
}
async function menu() {
  if (!await page.locator('.native-source-action').first().isVisible()) {
    await page.getByRole('button', {name: 'Your Sim', exact: true}).click();
  }
  await page.locator('.native-source-action').first().waitFor();
}
async function tick() {
  const text = await page.locator('#native-tick').textContent();
  const value = /Tick (\d+)/.exec(text)?.[1];
  assert.ok(value, 'The actual player must expose its accepted tick');
  return BigInt(value);
}

try {
  const sourceHost = await readFile('apps/web-shell/public/native-socket.mjs');
  assert.deepEqual(await readFile(resolve(dist, 'native-socket.mjs')), sourceHost, 'The served socket must equal tracked source');
  result.socket_sha256 = digest(sourceHost);
  gateway = spawn(resolve('target/debug/examples/controlled_replay'), [], {
    env: {...process.env, WONDERLAND_REPLAY_BIND: '127.0.0.1:19187', WONDERLAND_REPLAY_BROWSER_ORIGINS: 'http://127.0.0.1:19188'},
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  let spawnError;
  gateway.on('error', error => { spawnError = error; });
  gateway.stderr.on('data', bytes => { if (gatewayLog.length < 64000) gatewayLog += bytes; });
  await waitFor(async () => {
    if (spawnError) throw spawnError;
    if (gateway.exitCode !== null) throw Error('Controlled gateway exited');
    try { return (await fetch('http://127.0.0.1:19187/health', {signal: AbortSignal.timeout(1000)})).ok; }
    catch { return false; }
  }, 'Controlled gateway unavailable', 10000);
  fixture = await createFixture({dist, gateway: 'http://127.0.0.1:19187', port: 19188,
    runtimeExecutable: resolve('target/debug/examples/native_browser_peer'), actionDelayTicks: 3});
  browser = await chromium.launch({headless: true, args: ['--no-sandbox', '--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader']});
  const context = await browser.newContext({viewport: {width: 390, height: 844}, isMobile: true, hasTouch: true, reducedMotion: 'reduce'});
  page = await context.newPage();
  page.on('pageerror', error => result.errors.push({type: 'pageerror', message: error.message}));
  page.on('console', message => {
    if (message.type() === 'error') result.errors.push({type: 'console', message: message.text()});
    else if (message.type() === 'warning') result.warnings.push(message.text());
  });
  await enter();
  assert.equal(await page.title(), 'Wonderland');
  assert.equal(await page.locator('vite-error-overlay,nextjs-portal').count(), 0);
  record('Native player entry', {url: page.url(), title: await page.title(), chromium: browser.version(), viewport: page.viewportSize()});

  await menu();
  await page.locator('.native-source-action').first().click();
  await page.getByText('Accepted by the server', {exact: true}).waitFor();
  await page.locator('.native-action-feedback').filter({hasText: /completed/}).waitFor();
  assert.equal(fixture.stats.actions, 1);
  record('Validated receipt permits a subsequent action', {actions: fixture.stats.actions, accepted: fixture.stats.accepted});

  fixture.loseNextReceipt();
  const start = performance.now();
  await page.locator('.native-source-action').first().click();
  await waitFor(() => fixture.stats.silentReceipts === 1, 'The controlled receipt was not withheld');
  await page.getByText('Sent · awaiting server acceptance', {exact: true}).waitFor();
  await waitFor(async () => await page.locator('.native-action-history li').count() === 2, 'The second real action did not complete');
  const before = await tick();
  await delay(4000);
  const during = await tick();
  assert.ok(during > before, 'Accepted ticks continue despite the missing receipt');
  assert.equal(fixture.active(), 1, 'The server must not close the socket to create this test');
  assert.equal(await page.locator('.native-lot').getAttribute('data-native-live'), 'true');
  await page.getByText('Sent · awaiting server acceptance', {exact: true}).waitFor();
  record('Actual action completion and later ticks do not stand in for a receipt', {
    tickBefore: before.toString(), tickDuring: during.toString(), actions: fixture.stats.actions,
    terminalOutcomes: await page.locator('.native-action-history li').count(), serverForcedDisconnects: fixture.stats.unknownDrops,
  });
  await capture('01-pending-with-live-ticks');

  const notice = 'The server did not confirm this action in time. Its result is unknown. Reconnect to continue; it will not be retried.';
  await page.getByText(notice, {exact: true}).waitFor({timeout: 25000});
  const elapsed = performance.now() - start;
  assert.ok(elapsed >= 14000 && elapsed < 30000, 'Use the unchanged 15s production deadline, allowing scheduling overhead');
  assert.equal(await page.locator('.native-lot').getAttribute('data-native-live'), 'false');
  assert.equal(fixture.stats.actions, 2, 'The timed-out command is not retried');
  assert.equal(fixture.stats.unknownDrops, 0);
  await waitFor(() => fixture.active() === 0, 'The timed-out socket was not disposed');
  record('Independent action deadline exits the indefinitely pending state', {
    elapsedMilliseconds: Math.round(elapsed), actionsSent: 2, silentReceipts: fixture.stats.silentReceipts,
    serverForcedDisconnects: 0, activeNativeSockets: fixture.active(), notice,
  });
  await capture('02-receipt-timeout');

  await page.getByRole('button', {name: 'Reconnect', exact: true}).click();
  await page.locator('.native-lot[data-native-live="true"]').waitFor();
  await menu();
  await page.getByText('Previous action result unknown · not retried', {exact: true}).waitFor();
  assert.equal(await page.locator('.native-action-history li').count(), 2, 'Recovery does not replay historical completions');
  assert.ok(await page.locator('.native-source-action').evaluateAll(buttons => buttons.every(button => button.disabled)));
  assert.equal(fixture.stats.actions, 2);
  record('Reconnect retains unknown result and history without replay', {actionsSent: 2, historyEntries: 2, checkpoints: fixture.stats.checkpoints});
  await capture('03-recovered-unknown-result');

  await page.getByRole('button', {name: 'Dismiss unknown result without retrying', exact: true}).click();
  assert.equal(fixture.stats.actions, 2);
  await page.locator('.native-source-action').first().click();
  await page.getByText('Accepted by the server', {exact: true}).waitFor();
  await waitFor(async () => await page.locator('.native-action-history li').count() === 3, 'New action did not complete');
  const settledTick = await tick();
  await delay(16000);
  assert.equal(await page.locator('.native-lot').getAttribute('data-native-live'), 'true', 'A validated receipt clears its action timer');
  assert.ok(await tick() > settledTick);
  assert.equal(fixture.stats.actions, 3);
  assert.equal(fixture.active(), 1);
  record('A separately selected and confirmed action does not falsely time out', {actionsSent: 3, liveAfterDeadline: true});
  await capture('04-confirmed-after-deadline');

  fixture.loseNextReceipt();
  await page.locator('.native-source-action').first().click();
  await waitFor(() => fixture.stats.silentReceipts === 2, 'Final controlled receipt was not withheld');
  await page.getByRole('button', {name: 'Return to city', exact: true}).click();
  await page.getByRole('heading', {name: 'Controlled City', exact: true}).waitFor();
  await waitFor(() => fixture.active() === 0, 'Leaving the lot did not dispose its pending socket');
  await delay(16000);
  assert.equal(await page.locator('.native-lot').count(), 0);
  assert.equal(await page.getByText(notice, {exact: true}).count(), 0);
  assert.equal(fixture.stats.actions, 4);
  assert.deepEqual(result.errors, []);
  record('Route exit disposes pending deadlines without late UI mutation', {activeNativeSockets: 0, pageErrors: 0, consoleErrors: 0, actionsSent: 4});
  result.passed = true;
  result.stats = {...fixture.stats};
} catch (error) {
  result.failure = error.stack;
  result.stats = fixture ? {...fixture.stats} : null;
  console.error(error);
  process.exitCode = 1;
  if (page) await capture('failure').catch(() => {});
} finally {
  await writeFile(resolve(output, 'results.json'), JSON.stringify(result, null, 2) + '\n');
  await writeFile(resolve(output, 'gateway.log'), gatewayLog);
  await browser?.close();
  await fixture?.close();
  gateway?.kill();
}
