// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Exercise the portable path through the real directory input and browser download.
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { chromium, firefox, webkit } = require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');

const fixture = path.join(__dirname, '../tests/fixtures/core');
const masters = ['Core-Regular.ufo', 'Core-Bold.ufo'];
const descriptor = 'Core.designspace';
const attachment = 'références/notes.bin';

function createFamily(directory) {
  fs.mkdirSync(path.join(directory, 'masters'), { recursive: true });
  for (const master of masters) {
    fs.cpSync(path.join(fixture, master), path.join(directory, 'masters', master), { recursive: true });
  }
  // This descriptor retains its own formatting and relative source references on Save.
  const xml = fs.readFileSync(path.join(fixture, descriptor), 'utf8');
  fs.writeFileSync(path.join(directory, descriptor), xml.replaceAll('filename="Core-', 'filename="masters/Core-'));
  const regular = path.join(directory, 'masters', masters[0]);
  const info = path.join(regular, 'fontinfo.plist');
  const annotated = fs.readFileSync(info, 'utf8').replaceAll('\t', '  ')
    .replace('<dict>', '<dict>\n  <!-- Preserve the designer’s original metadata formatting. -->');
  fs.writeFileSync(info, annotated);
  fs.writeFileSync(path.join(regular, 'glyphs', 'layerinfo.plist'),
    '<?xml version="1.0"?>\n<plist version="1.0"><dict><!-- Empty by choice. --></dict></plist>\n');
  fs.mkdirSync(path.dirname(path.join(directory, attachment)), { recursive: true });
  fs.writeFileSync(path.join(directory, attachment), Buffer.from([0x00, 0xff, 0x38, 0x80]));
}

async function openFolder(page, directory) {
  await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4326/');
  await page.waitForFunction(() => window.runebender);
  await page.locator('canvas').focus();
  await page.keyboard.press('Control+o');
  const input = page.locator('input[type=file][webkitdirectory]');
  await input.waitFor({ state: 'visible' });
  await input.setInputFiles(directory);
  await page.waitForFunction(name => {
    const { source, note } = window.runebender.state();
    return source === name || note.startsWith('Open failed');
  }, descriptor);
  const state = await page.evaluate(() => window.runebender.state());
  assert.equal(state.source, descriptor, state.note);
  assert.equal(state.modified, false);
}

async function openGlyph(page) {
  await page.mouse.dblclick(425, 150, { delay: 100 });
  await page.waitForFunction(() => window.runebender.state().mode === 'editor');
  const { glyph, points } = await page.evaluate(() => window.runebender.state());
  assert.ok(points.length > 0, 'the selected glyph has an outline');
  return { glyph, points };
}

function verifySource(original, extracted, editedGlyph) {
  const paths = directory => fs.readdirSync(directory, { recursive: true })
    .filter(relative => fs.statSync(path.join(directory, relative)).isFile()).sort();
  const originalPaths = paths(original);
  assert.deepEqual(paths(extracted), originalPaths, 'the editable ZIP contains the complete source tree');
  const editedDirectory = path.join('masters', masters[0], 'glyphs');
  let changed = 0;
  for (const relative of originalPaths) {
    const before = fs.readFileSync(path.join(original, relative));
    if (before.equals(fs.readFileSync(path.join(extracted, relative)))) continue;
    assert.equal(path.dirname(relative), editedDirectory, `unexpected change in ${relative}`);
    assert.ok(relative.endsWith('.glif') && before.toString('utf8').includes(`<glyph name="${editedGlyph}"`),
      `unexpected change in ${relative}`);
    changed++;
  }
  assert.equal(changed, 1, 'only the edited GLIF changed across the entire source tree');
  return originalPaths.length - changed;
}

async function roundTrip(engine, original, scratch) {
  const browser = await engine.launch({
    headless: true,
    executablePath: engine === chromium ? process.env.RUNEBENDER_CHROME : undefined,
  });
  const pageErrors = [];
  const newPage = async () => {
    const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    if (engine === chromium) {
      // Permission denial routes this browser through the same visible import control.
      await context.addInitScript(() => {
        window.showDirectoryPicker = async () => {
          throw new DOMException('Folder access denied', 'NotAllowedError');
        };
      });
    }
    const page = await context.newPage();
    page.on('pageerror', error => pageErrors.push(error.message));
    // Keep the native chooser pending until the test supplies the selected directory.
    page.on('filechooser', () => {});
    return page;
  };
  try {
    const page = await newPage();
    await openFolder(page, original);
    const before = await openGlyph(page);
    await page.mouse.click(650, 350);
    await page.keyboard.press('Control+a');
    await page.keyboard.press('ArrowRight');
    await page.waitForFunction(() => window.runebender.state().modified);
    const edited = await page.evaluate(() => window.runebender.state().points);
    assert.notDeepEqual(edited, before.points, 'the keyboard nudge moved the outline');

    const [download] = await Promise.all([
      page.waitForEvent('download').catch(async error => {
        const note = await page.evaluate(() => window.runebender.state().note);
        throw new Error(`${engine.name()} did not download the project: ${note}`, { cause: error });
      }),
      page.keyboard.press('Control+s'),
    ]);
    assert.match(download.suggestedFilename(), /\.zip$/i);
    const zip = path.join(scratch, engine.name() + '.zip');
    await download.saveAs(zip);
    await page.context().close();

    const extracted = path.join(scratch, engine.name());
    execFileSync('python3', ['-m', 'zipfile', '-e', zip, extracted]);
    const preserved = verifySource(original, extracted, before.glyph);
    const reopened = await newPage();
    await openFolder(reopened, extracted);
    assert.deepEqual(await openGlyph(reopened), { glyph: before.glyph, points: edited },
      'the edited outline survives opening the downloaded source in a fresh browser session');
    assert.deepEqual(pageErrors, []);
    console.log(`${engine.name()} portable source passed: fresh reopen with the edited glyph and ${preserved} byte-identical files`);
  } finally {
    await browser.close();
  }
}

(async () => {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'runebender-portable-'));
  try {
    const family = path.join(scratch, 'Portable Family');
    createFamily(family);
    for (const engine of [firefox, webkit, chromium]) await roundTrip(engine, family, scratch);
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
