import test from 'node:test';
import assert from 'node:assert/strict';
import * as audio from './source-audio.mjs';

// Only the document's focus primitives are controlled. The production helper
// chooses the opener and owns capture/return across the real dialog lifecycle.
function documentFixture() {
  const document = { activeElement: null, triggers: [] };
  function element({ tabIndex = 0, disabled = false, insideDialog = false } = {}) {
    return { tabIndex, disabled, insideDialog, isConnected: true,
      focus() { if (this.isConnected && !this.disabled && this.tabIndex >= 0) document.activeElement = this; },
      getClientRects: () => [{}] };
  }
  document.body = element({ tabIndex: -1 });
  document.documentElement = element({ tabIndex: -1 });
  document.activeElement = document.body;
  document.querySelectorAll = selector => {
    assert.equal(selector, '.source-audio-control .settings-audio');
    return document.triggers.filter(trigger => trigger.isConnected);
  };
  const dialog = { ownerDocument: document, open: false, isConnected: true, contains: candidate => candidate?.insideDialog === true };
  const opener = element(), close = element({ insideDialog: true });
  document.triggers.push(opener);
  return { document, dialog, opener, close, element };
}

test('async dialog opening restores the temporarily disabled Sound settings trigger instead of body', () => {
  const { document, dialog, opener, close } = documentFixture();
  // The reactive opener is disabled before import resolves, which drops native
  // activeElement to body. It becomes enabled after showModal has focused Close.
  opener.disabled = true;
  const focus = audio.createSourceAudioFocusReturn(dialog);
  focus.capture();
  dialog.open = true; document.activeElement = close; opener.disabled = false;
  dialog.open = false;
  focus.restore();
  assert.equal(document.activeElement, opener);
});

test('close resolves a replacement Sound settings button when the captured node was removed', () => {
  const { document, dialog, opener, close, element } = documentFixture();
  document.activeElement = opener;
  const focus = audio.createSourceAudioFocusReturn(dialog); focus.capture();
  dialog.open = true; document.activeElement = close; opener.isConnected = false;
  const replacement = element(); document.triggers.push(replacement);
  dialog.open = false; focus.restore();
  assert.equal(document.activeElement, replacement);
});

test('repeated open preserves the external opener and stale disposal cannot steal focus', () => {
  const { document, dialog, opener, close, element } = documentFixture();
  document.activeElement = opener;
  const focus = audio.createSourceAudioFocusReturn(dialog); focus.capture();
  dialog.open = true; document.activeElement = close; focus.capture();
  focus.restore(); assert.equal(document.activeElement, close);
  dialog.open = false; focus.restore(); assert.equal(document.activeElement, opener);
  const nextView = element(); document.activeElement = nextView; dialog.isConnected = false;
  focus.restore(); assert.equal(document.activeElement, nextView);
});
