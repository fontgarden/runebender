// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Real browser input, with only the folder picker replaced by the package hook.
// Coordinates target the shared Gray widget tree at 1280 x 800, DPR 1.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');
const root = path.join(__dirname, '../assets/font-sources/variable-fonts/HOIShapes');
function readFiles(directory, files = []) {
  for (const entry of fs.readdirSync(directory).sort()) {
    const full = path.join(directory, entry);
    if (fs.statSync(full).isDirectory()) readFiles(full, files);
    else if (!/\.(md|png)$/.test(entry)) files.push([
      path.relative(root, full).split(path.sep).join('/'), [...fs.readFileSync(full)],
    ]);
  }
  return files;
}

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.RUNEBENDER_CHROME });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4326/');
    await page.waitForFunction(() => window.runebender);
    const files = readFiles(root);
    const state = () => page.evaluate(() => window.runebender.state());
    const pause = () => page.waitForTimeout(150);
    async function open(bundle) {
      assert.equal(await page.evaluate(files => window.runebender.openFiles(
        null, files.map(([name, bytes]) => [name, new Uint8Array(bytes)])), bundle), '');
      await pause();
      await page.mouse.dblclick(425, 150, { delay: 100 });
      await pause();
      assert.equal((await state()).glyph, 'box');
      await page.keyboard.press('i');
      await pause();
      assert.equal((await state()).tool, 'Interpolation');
    }
    await open(files);
    const original = await state();
    assert.equal(original.interpolation.interval, 1);
    await page.mouse.move(400, 442);
    await page.mouse.down();
    await page.mouse.move(400, 385, { steps: 8 });
    assert.deepEqual((await state()).interpolation.records, [], 'pointer draft is not committed');
    assert.deepEqual((await state()).points, original.points, 'masters stay fixed while dragging');
    await page.mouse.up();
    await pause();
    const bent = (await state()).interpolation.records;
    assert.equal(bent.length, 1, 'the selected path was bent');
    await page.keyboard.press('Meta+z');
    await pause();
    assert.deepEqual((await state()).interpolation.records, [], 'one Undo removes the whole drag');
    await page.keyboard.press('Meta+Shift+z');
    await pause();
    assert.deepEqual((await state()).interpolation.records, bent, 'Redo restores the curve');

    // The fitted Regular/Bold interval spans x=390..830, with 32px canvas padding.
    const zoom = 690 / 440;
    const [cx, cy] = bent[0].controls[0];
    const control = [295 + (cx - 390) * zoom, 442 - (cy - 280) * zoom];
    await page.mouse.move(...control); await page.mouse.down();
    await page.mouse.move(control[0] + 25, control[1] - 20, { steps: 4 });
    await page.keyboard.press('Escape'); await page.mouse.up();
    await pause();
    assert.deepEqual((await state()).interpolation.records, bent, 'Escape discards the control drag');
    await page.mouse.move(...control); await page.mouse.down();
    await page.mouse.move(control[0] + 20, control[1] - 10, { steps: 4 });
    await page.mouse.up(); await pause();
    const editedControl = (await state()).interpolation.records[0];
    assert.notDeepEqual(editedControl.controls[0], bent[0].controls[0], 'the picked cubic control moves');
    assert.deepEqual(editedControl.controls[1], bent[0].controls[1], 'the other cubic control stays fixed');
    await page.keyboard.press('Meta+z'); await pause();
    assert.deepEqual((await state()).interpolation.records, bent);
    await page.keyboard.down('Shift'); await page.mouse.click(400, 223); await page.keyboard.up('Shift');
    await pause();
    assert.equal((await state()).interpolation.selected.length, 2, 'Shift adds a path');
    await page.keyboard.down('Shift'); await page.mouse.click(400, 223); await page.keyboard.up('Shift');
    await pause();
    assert.equal((await state()).interpolation.selected.length, 1, 'Shift removes a selected path');


    await page.mouse.click(1125, 421); // Ease In.
    await pause();
    assert.equal((await state()).interpolation.records[0].easing, 'ease-in');
    await page.mouse.click(1220, 261); // Scrub the exact preview.
    await pause();
    assert.ok((await state()).interpolation.progress > 0.5);
    await page.mouse.click(1205, 288); // Compare Export.
    await page.waitForFunction(() => window.runebender.state().interpolation.comparison === 'Export overlay ready');
    await page.mouse.click(1140, 508); // Select all paths.
    await pause();
    assert.equal((await state()).interpolation.selected.length, 4);
    await page.mouse.click(400, 223); // Refocus an already-selected path.
    await page.keyboard.press('ArrowUp');
    await pause();
    const authored = (await state()).interpolation.records;
    assert.equal(authored.length, 4, 'nudge applies to the selected group');
    assert.deepEqual((await state()).points, original.points);
    if (process.env.RUNEBENDER_HOI_PROOF_DIR) {
      fs.mkdirSync(process.env.RUNEBENDER_HOI_PROOF_DIR, { recursive: true });
      await page.mouse.move(1278, 798);
      await page.waitForTimeout(1100);
      await page.screenshot({ path: path.join(process.env.RUNEBENDER_HOI_PROOF_DIR, 'editor.png') });
    }
    // Clean preview uses the same coordinates and cannot edit hidden paths.
    const selected = (await state()).interpolation.selected;
    await page.mouse.click(1205, 233);
    await pause();
    assert.equal((await state()).interpolation.preview_only, true);
    await page.mouse.click(1220, 261);
    await page.mouse.move(400, 223); await page.mouse.down();
    await page.mouse.move(420, 200, { steps: 4 }); await page.mouse.up();
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('Backspace');
    await pause();
    assert.deepEqual((await state()).interpolation.records, authored);
    assert.deepEqual((await state()).interpolation.selected, selected);
    assert.deepEqual((await state()).points, original.points);
    await page.mouse.click(1075, 233);
    await pause();
    assert.equal((await state()).interpolation.preview_only, false);
    assert.deepEqual((await state()).interpolation.selected, selected);
    await page.mouse.click(1140, 479); // Reset Selected.
    await pause();
    assert.deepEqual((await state()).interpolation.records, []);
    await page.keyboard.press('Meta+z');
    await pause();
    assert.deepEqual((await state()).interpolation.records, authored);
    await page.mouse.click(1140, 130); // Neighboring Light/Regular interval.
    await pause();
    assert.equal((await state()).interpolation.interval, 0);
    assert.deepEqual((await state()).interpolation.selected, []);
    await page.mouse.click(1140, 508);
    await page.mouse.click(1200, 421); // Ease Out in the other interval.
    await pause();
    assert.equal((await state()).interpolation.records.length, 8);
    assert.deepEqual((await state()).interpolation.records.slice(0, 4), authored);
    await page.keyboard.press('Meta+z');
    await pause();
    assert.deepEqual((await state()).interpolation.records, authored);
    const saved = await page.evaluate(() => window.runebender.saveFiles().files
      .map(([name, bytes]) => [name, Array.from(bytes)]));
    const written = new Map(saved);
    for (const [name, bytes] of files.filter(([name]) => name.endsWith('.glif'))) {
      assert.deepEqual(written.get(name), bytes, `source bytes unchanged: ${name}`);
    }
    const merged = new Map([...files, ...saved]);
    await open([...merged]);
    assert.deepEqual((await state()).interpolation.records, authored, 'saved trajectories reopen');
    assert.deepEqual((await state()).points, original.points);
    assert.deepEqual(errors, []);
    console.log('HOI browser editing passed: drag, control cancel, easing, scrub, export overlay, group nudge, read-only preview, reset, interval scope, undo/redo, save/reopen; all 18 GLIFs unchanged.');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exit(1); });
