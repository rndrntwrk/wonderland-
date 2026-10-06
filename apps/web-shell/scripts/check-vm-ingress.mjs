/**
 * Adversarial socket scheduling against the built application and controlled
 * native gateway. Original payloads are unchanged; the first two VM callbacks
 * deliberately run in the same task, before reactive effects can consume them.
 *
 * Requires a release web bundle, built controlled_replay, Node and Playwright.
 * Uses only fresh contexts and owned loopback fixtures, never production accounts.
 *
 * node apps/web-shell/scripts/check-vm-ingress.mjs
 * WONDERLAND_QA_DIST, WONDERLAND_QA_GATEWAY, WONDERLAND_QA_OUTPUT,
 * PLAYWRIGHT_MODULE and WONDERLAND_QA_CHROMIUM override local paths.
 */
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { resolve, extname, sep, isAbsolute } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const dist = resolve(process.env.WONDERLAND_QA_DIST || `${root}/apps/web-shell/dist`);
const output = resolve(process.env.WONDERLAND_QA_OUTPUT || `${tmpdir()}/wonderland-vm-ingress`);
const binary = resolve(process.env.WONDERLAND_QA_GATEWAY || `${root}/target/debug/examples/controlled_replay`);
const specifier = process.env.PLAYWRIGHT_MODULE || 'playwright';
const { chromium } = await import(isAbsolute(specifier) ? pathToFileURL(specifier).href : specifier);
const gatewayPort = 18992;
const gatewayUrl = `http://127.0.0.1:${gatewayPort}`;
const mime = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
  '.wasm': 'application/wasm', '.css': 'text/css', '.json': 'application/json',
  '.png': 'image/png', '.svg': 'image/svg+xml', '.webp': 'image/webp', '.woff2': 'font/woff2' };
const delay = ms => new Promise(done => setTimeout(done, ms));
const results = { fixture: 'controlled_replay; unchanged source-wire payloads; deferred callback scheduler',
  passed: false, checks: [], pageErrors: [] };
let browser;
let gateway;
let gatewayError;
let gatewayLog = '';
await mkdir(output, { recursive: true });
await readFile(resolve(dist, 'index.html'));
const server = createServer(async (req, res) => {
  try {
    const pathname = decodeURIComponent(new URL(req.url, 'http://127.0.0.1').pathname);
    const path = resolve(dist, pathname === '/' ? 'index.html' : `.${pathname}`);
    if (!path.startsWith(`${dist}${sep}`)) return res.writeHead(403).end();
    const data = await readFile(path);
    res.writeHead(200, { 'Content-Type': mime[extname(path)] || 'application/octet-stream',
      'Cache-Control': 'no-store' }).end(data);
  } catch { res.writeHead(404).end(); }
});
try {
  await new Promise((done, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', done); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  gateway = spawn(binary, [], { env: { ...process.env, WONDERLAND_REPLAY_BIND: `127.0.0.1:${gatewayPort}`,
    WONDERLAND_REPLAY_BROWSER_ORIGINS: origin }, stdio: ['ignore', 'pipe', 'pipe'] });
  gateway.on('error', error => { gatewayError = error; });
  gateway.stdout.on('data', data => { gatewayLog += data.toString(); });
  gateway.stderr.on('data', data => { gatewayLog += data.toString(); });
  let ready = false;
  for (let i = 0; i < 100; i++) {
    if (gatewayError) throw gatewayError;
    if (gateway.exitCode !== null) throw Error(`Controlled gateway exited: ${gatewayLog}`);
    try {
      ready = (await fetch(`${gatewayUrl}/health`, { signal: AbortSignal.timeout(1000) })).ok;
    } catch { /* owned fixture is starting */ }
    if (ready) break;
    await delay(50);
  }
  assert(ready, `Controlled gateway unavailable: ${gatewayLog}`);
  browser = await chromium.launch({ headless: true,
    ...(process.env.WONDERLAND_QA_CHROMIUM ? { executablePath: process.env.WONDERLAND_QA_CHROMIUM } : {}),
    args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  for (const [width, height] of [[1440, 1000], [390, 844]]) {
    const context = await browser.newContext({ viewport: { width, height }, reducedMotion: 'reduce',
      isMobile: width < 600, hasTouch: width < 600 });
    const page = await context.newPage();
    page.on('pageerror', error => results.pageErrors.push(error.message));
    await context.addInitScript(() => {
      const Native = window.WebSocket;
      const descriptor = Object.getOwnPropertyDescriptor(Native.prototype, 'onmessage');
      const state = { batches: [], injected: 0, inject: null };
      Object.defineProperty(window, '__vmIngressTest', { value: state });
      window.WebSocket = class ScheduledSocket extends Native {
        set onmessage(callback) {
          this.testCallback = callback;
          const pending = [];
          let initial = true;
          descriptor.set.call(this, event => {
            let envelope;
            try { envelope = JSON.parse(event.data); } catch { return callback?.call(this, event); }
            if (envelope.event?.type !== 'vm_frame') return callback?.call(this, event);
            state.inject = count => {
              if (!Number.isInteger(count) || count < 1 || count > 1024) throw Error('Invalid test count');
              for (let i = 0; i < count; i++) callback?.call(this, new MessageEvent('message', { data: event.data }));
              state.injected += count;
            };
            if (!initial) return callback?.call(this, event);
            pending.push(event);
            if (pending.length === 2) {
              initial = false;
              const batch = pending.splice(0);
              state.batches.push(batch.map(item => JSON.parse(item.data).event.data.length));
              for (const item of batch) callback?.call(this, item);
            }
          });
        }
        get onmessage() { return this.testCallback ?? null; }
      };
    });
    await page.route('**/wonderland-config.json', route => route.fulfill({ contentType: 'application/json',
      body: JSON.stringify({ version: 1, mode: 'connected', gateway_url: gatewayUrl }) }));
    await page.goto(origin);
    await page.getByLabel('Account name', { exact: true }).fill('controlled-player');
    await page.getByLabel('Password', { exact: true }).fill('test-only');
    await page.getByRole('button', { name: 'Sign in', exact: true }).click();
    await page.getByRole('heading', { name: 'Choose your Sim', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Play as Controlled Alice', exact: true }).click();
    await page.getByRole('heading', { name: 'Controlled City', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Select Controlled Source Lot', exact: true }).click();
    await page.getByRole('button', { name: 'Visit', exact: true }).click();
    await page.locator('.connected-world canvas[data-renderer="source-software-3d"]').waitFor();
    const batches = await page.evaluate(() => window.__vmIngressTest.batches);
    assert.equal(batches.length, 1);
    assert.equal(batches[0].length, 2);
    await page.screenshot({ path: resolve(output, `burst-world-${width}x${height}.png`) });
    results.checks.push({ width, height, name: 'Two VM callbacks before render retain the source world', batches });
    await page.evaluate(() => window.__vmIngressTest.inject(257));
    await page.getByRole('alert').filter({ hasText: 'World updates could not be kept in order' }).waitFor();
    await page.getByRole('button', { name: 'Reconnect', exact: true }).waitFor();
    assert.equal(await page.locator('.connected-world canvas[data-renderer="source-software-3d"]').count(), 0);
    await page.screenshot({ path: resolve(output, `overflow-recovery-${width}x${height}.png`) });
    results.checks.push({ width, height, name: 'Overflow disconnects and removes the stale playable view',
      injected: await page.evaluate(() => window.__vmIngressTest.injected) });
    await context.close();
  }
  assert.deepEqual(results.pageErrors, []);
  results.passed = true;
  console.log(JSON.stringify(results, null, 2));
} finally {
  await writeFile(resolve(output, 'results.json'), `${JSON.stringify(results, null, 2)}\n`);
  await writeFile(resolve(output, 'gateway.log'), gatewayLog);
  if (browser) await browser.close();
  if (gateway && gateway.exitCode === null) gateway.kill('SIGTERM');
  server.closeAllConnections();
  await new Promise(done => server.close(done));
}
