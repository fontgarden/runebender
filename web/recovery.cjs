// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const path = require('node:path');
const { chromium } = require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');

const fixture = path.join(__dirname, '../assets/font-sources/variable-fonts/HOIShapes');
const url = process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4326/';

async function openBox(page) {
  await page.mouse.dblclick(425, 150, { delay: 100 });
  await page.waitForFunction(() => {
    const state = window.runebender.state();
    return state.mode === 'editor' && state.glyph === 'box';
  });
  return page.evaluate(() => window.runebender.state().points);
}

// Observe a committed checkpoint through a separate transaction, before closing the editing tab.
async function checkpointAfter(revision) {
  const database = await new Promise((resolve, reject) => {
    const request = indexedDB.open('runebender-recovery');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  try {
    const transaction = database.transaction(['labels', 'sources']);
    const stored = store => new Promise((resolve, reject) => {
      const request = transaction.objectStore(store).openCursor();
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const [label, source] = await Promise.all([stored('labels'), stored('sources')]);
    if (label?.value.name !== 'HOIShapes' || label.value.revision <= revision || source?.key !== label.key) return null;
    const digest = await crypto.subtle.digest('SHA-256', source.value.bytes);
    return { revision: label.value.revision, digest: Array.from(new Uint8Array(digest)), featureDraft: source.value.featureDraft };
  } finally {
    database.close();
  }
}

async function waitForCheckpoint(page, revision, featureDraft) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const checkpoint = await page.evaluate(checkpointAfter, revision);
    if (checkpoint && (featureDraft === undefined || checkpoint.featureDraft === featureDraft)) return checkpoint;
    await page.waitForTimeout(150);
  }
  throw new Error(`No complete font checkpoint after revision ${revision}`);
}

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.RUNEBENDER_CHROME });
  try {
    const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
    await context.addInitScript(() => { window.showDirectoryPicker = undefined; });
    const errors = [];
    context.on('page', page => page.on('pageerror', error => errors.push(error.message)));

    const original = await context.newPage();
    await original.goto(url);
    await original.waitForFunction(() => window.runebender);
    await original.locator('canvas').focus();
    const [chooser] = await Promise.all([
      original.waitForEvent('filechooser'),
      original.keyboard.press('Control+o'),
    ]);
    await chooser.setFiles(fixture);
    await original.waitForFunction(() => window.runebender.state().source === 'HOIShapes.designspace');
    const before = await openBox(original);
    await original.mouse.click(650, 350);
    await original.keyboard.press('Control+a');
    await original.keyboard.press('ArrowRight');
    await original.waitForFunction(() => window.runebender.state().modified);
    const firstEdit = await original.evaluate(() => window.runebender.state().points);
    assert.notDeepEqual(firstEdit, before);
    const first = await waitForCheckpoint(original, -1);

    // Fail the source write after staging its new label, inside the real IndexedDB transaction.
    await original.evaluate(() => {
      const put = IDBObjectStore.prototype.put;
      IDBObjectStore.prototype.put = function (...args) {
        if (this.name === 'sources') {
          IDBObjectStore.prototype.put = put;
          throw new DOMException('Test quota exhausted', 'QuotaExceededError');
        }
        return put.apply(this, args);
      };
    });
    await original.keyboard.press('ArrowRight');
    await original.waitForFunction(() => window.runebender.state().note.includes('Test quota exhausted'));
    const edited = await original.evaluate(() => window.runebender.state().points);
    assert.notDeepEqual(edited, firstEdit);
    assert.deepEqual(await original.evaluate(checkpointAfter, -1), first, 'a failed write preserves the previous label and bytes');
    // A visibility transition retries the outstanding checkpoint without another edit or Save.
    await original.evaluate(() => {
      Object.defineProperty(document, 'hidden', { value: true, configurable: true });
      try { document.dispatchEvent(new Event('visibilitychange')); } finally { delete document.hidden; }
    });
    const retry = await waitForCheckpoint(original, first.revision);
    assert.notDeepEqual(retry.digest, first.digest, 'the retry stored the newer font');

    // Supply a real directory handle through the drop event's DataTransferItem boundary.
    await original.evaluate(async () => {
      const directory = await navigator.storage.getDirectory();
      const event = new Event('drop', { bubbles: true, cancelable: true });
      Object.defineProperty(event, 'dataTransfer', {
        value: { items: [{ getAsFileSystemHandle: async () => directory }] },
      });
      document.querySelector('canvas').dispatchEvent(event);
    });
    await original.waitForFunction(() => window.runebender.state().note.includes('Save the current font'));
    assert.deepEqual(await original.evaluate(() => window.runebender.state().points), edited);

    const second = await context.newPage();
    await second.goto(url);
    await second.waitForFunction(() => window.runebender || document.querySelector('#recovery').open);
    assert.equal(await second.locator('#recovery').isVisible(), false, 'an active tab owns its own draft');
    await second.close();
    const lockless = await context.newPage();
    await lockless.addInitScript(() => Object.defineProperty(navigator, 'locks', { value: undefined }));
    await lockless.goto(url);
    const locklessDialog = lockless.locator('#recovery');
    await locklessDialog.waitFor({ state: 'visible' });
    assert.equal(await locklessDialog.getByRole('button', { name: 'Discard selected checkpoint' }).isDisabled(), true);
    await lockless.evaluate(() => document.querySelector('#recovery').close('discard'));
    await lockless.waitForFunction(() => window.runebender);
    assert.deepEqual(await original.evaluate(checkpointAfter, -1), retry, 'a tab without locks cannot discard another tab’s checkpoint');
    await lockless.close();
    await original.close();

    const reopened = await context.newPage();
    await reopened.goto(url);
    const dialog = reopened.locator('#recovery');
    await dialog.waitFor({ state: 'visible' });
    assert.match(await dialog.locator('option').innerText(), /HOIShapes/);
    await dialog.getByRole('button', { name: 'Recover', exact: true }).click();
    await reopened.waitForFunction(() => window.runebender);
    const recovered = await reopened.evaluate(() => window.runebender.state());
    assert.equal(recovered.source, 'HOIShapes.designspace');
    assert.equal(recovered.modified, true, 'recovery keeps unsaved edits marked dirty');
    assert.deepEqual(await openBox(reopened), edited, 'the closed tab’s unsaved outline was restored');
    const [download] = await Promise.all([
      reopened.waitForEvent('download'),
      reopened.keyboard.press('Control+s'),
    ]);
    assert.equal(download.suggestedFilename(), 'HOIShapes.zip');
    assert.equal(await download.failure(), null);

    // Switch from Regular to Bold in View → Next Master, then type without applying.
    await reopened.keyboard.press('Escape');
    await reopened.waitForFunction(() => window.runebender.state().mode === 'overview');
    await reopened.mouse.click(392, 16);
    await reopened.mouse.click(429, 146);
    await reopened.mouse.click(1068, 451); // Features disclosure
    await reopened.mouse.click(1044, 480); // empty features.fea field
    await reopened.waitForFunction(() => document.activeElement.id === 'text-input');
    const draft = 'languagesystem DFLT dflt;';
    await reopened.keyboard.type(draft);
    await waitForCheckpoint(reopened, -1, draft);
    await reopened.close();

    const features = await context.newPage();
    await features.goto(url);
    await features.getByRole('button', { name: 'Recover', exact: true }).click();
    await features.waitForFunction(() => window.runebender);
    await features.locator('canvas').focus();
    await features.keyboard.press('Control+s');
    await features.waitForFunction(() => window.runebender.state().note.includes('Apply or Revert feature edits'));
    await features.mouse.click(1068, 451);
    await features.mouse.click(1244, 739); // Apply shared features
    const [applied] = await Promise.all([features.waitForEvent('download'), features.keyboard.press('Control+s')]);
    const featureFiles = JSON.parse(execFileSync('python3', ['-c',
      'import json,sys,zipfile; z=zipfile.ZipFile(sys.argv[1]); print(json.dumps({p:z.read(p).decode() for p in z.namelist() if p.endswith("/features.fea")}))',
      await applied.path()], { encoding: 'utf8' }));
    assert.deepEqual(featureFiles, { 'HOIShapes-Regular.ufo/features.fea': draft },
      'Apply writes the recovered shared text to the default source');
    assert.deepEqual((await openBox(features))[0], [690, 280], 'the active drawing master is still Bold after recovery');
    assert.deepEqual(errors, []);
    console.log('Recovery passed: active tabs excluded, closed tab restored its outline, master, and unapplied feature text.');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
