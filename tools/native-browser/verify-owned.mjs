// Actual account-owned browser journey. The caller supplies the local durable
// runner and a qualified original-content action, never a pre-spawned actor.
import assert from 'node:assert/strict';
import {mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {observeNativeCanvas} from './canvas-observation.mjs';

const decimal = value => typeof value === 'string' && /^[1-9][0-9]*$/.test(value);

async function identity(page) {
  const lot = page.locator('.native-lot[data-native-live="true"]');
  await lot.waitFor({timeout: 30000});
  const avatar = await lot.getAttribute('data-owned-avatar-id');
  const principal = await lot.getAttribute('data-owned-principal');
  assert.ok(decimal(avatar), 'Decoded bootstrap must expose an owned avatar ID');
  assert.ok(decimal(principal), 'Decoded bootstrap must expose the admitted principal');
  assert.match(await lot.getAttribute('data-native-state-hash') ?? '', /^[0-9a-f]{64}$/,
    'Identity must be accompanied by the actual accepted replica hash');
  return {avatar, principal};
}

/**
 * exerciseObject must click a separately rendered original object through the
 * real UI and return {before, after}. observeObject reads that same visible
 * state in another browser. Neither hook may submit supervisor commands.
 * restartHost must terminate/recover the real process from durable authority;
 * recreating a fixture from an in-memory saved copy is not recovery evidence.
 */
export async function verifyOwnedJourney({browser, demo, output,
  exerciseObject, observeObject}) {
  assert.equal(new URL(demo.origin).hostname, '127.0.0.1');
  assert.equal(typeof exerciseObject, 'function');
  assert.equal(typeof observeObject, 'function');
  const accounts = await demo.setupAccounts();
  assert.equal(accounts.length, 2);
  assert.notEqual(accounts[0].username, accounts[1].username);
  assert.notEqual(accounts[0].avatar_id, accounts[1].avatar_id);
  for (const account of accounts) assert.ok(decimal(account.avatar_id));
  await mkdir(output, {recursive: true});
  const contexts = [], pages = [], identities = [], errors = [];
  try {
    for (const [index, account] of accounts.entries()) {
      const context = await browser.newContext({viewport: index === 0
        ? {width: 1440, height: 1000} : {width: 390, height: 844}});
      contexts.push(context);
      const page = await context.newPage();
      pages.push(page);
      page.on('pageerror', error => errors.push(error.message));
      await observeNativeCanvas(page);
      await page.goto(demo.origin);
      await page.getByLabel('Account name', {exact: true}).fill(account.username);
      await page.getByLabel('Password', {exact: true}).fill(account.password);
      await page.getByRole('button', {name: 'Sign in', exact: true}).click();
      await page.getByRole('button', {name: `Play as ${account.name}`, exact: true}).click();
      await page.getByRole('button', {name: `Enter ${account.lot_name}`, exact: true}).click();
      const bound = await identity(page);
      assert.equal(bound.avatar, account.avatar_id, 'Admission must bind the selected owned Sim');
      identities.push(bound);
      await page.waitForFunction(() => {
        const canvas = document.querySelector('.native-lot canvas');
        return canvas && window.__wonderlandCanvasEvidence?.read(canvas);
      });
      await page.screenshot({path: resolve(output, `owned-player-${index + 1}.png`)});
    }
    assert.notEqual(identities[0].principal, identities[1].principal,
      'Two accounts must never share the fixture principal');
    for (const page of pages) await page.waitForFunction(() =>
      Number(document.querySelector('.native-lot')?.dataset.nativeAvatarModels) >= 2);
    const changed = await exerciseObject(pages[0]);
    assert.notDeepEqual(changed.after, changed.before, 'Original object action must visibly change state');
    assert.deepEqual(await observeObject(pages[1]), changed.after,
      'The other owned player must observe the accepted object change');
    const durableBefore = await demo.inspectDurable();
    assert.equal(durableBefore.actors.length, 2);
    for (const bound of identities) assert.ok(durableBefore.actors.some(actor =>
      actor.avatar_id === bound.avatar && actor.principal === bound.principal));
    await demo.restartHost();
    for (const [index, page] of pages.entries()) {
      await page.locator('.native-lot[data-native-live="false"]').waitFor({timeout: 30000});
      await page.getByRole('button', {name: 'Reconnect', exact: true}).click();
      assert.deepEqual(await identity(page), identities[index], 'Recovery must preserve owned actor binding');
      assert.deepEqual(await observeObject(page), changed.after, 'Object change must survive process loss');
      await page.screenshot({path: resolve(output, `owned-recovered-${index + 1}.png`)});
    }
    const durableAfter = await demo.inspectDurable();
    assert.deepEqual(durableAfter.actors, durableBefore.actors, 'Recovery must not duplicate admission');
    assert.deepEqual(errors, [], 'No browser runtime exceptions');
    return {passed: true, identities, object: changed, durableBefore, durableAfter};
  } finally {
    await Promise.all(contexts.map(context => context.close()));
  }
}
