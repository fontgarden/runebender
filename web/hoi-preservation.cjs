// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Exercise the actual browser package boundary with the small HOI Shapes fixture.
// Folder writes use Chromium's temporary origin-private file system.
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.RUNEBENDER_PLAYWRIGHT || 'playwright');
const root = process.env.RUNEBENDER_HOI_SOURCE || path.join(__dirname, '../assets/font-sources/variable-fonts/HOIShapes');

function readFiles(directory, files = []) {
  for (const entry of fs.readdirSync(directory).sort()) {
    const full = path.join(directory, entry);
    if (fs.statSync(full).isDirectory()) readFiles(full, files);
    else if (!/\.(md|png)$/.test(entry)) {
      files.push([path.relative(root, full).split(path.sep).join('/'), [...fs.readFileSync(full)]]);
    }
  }
  return files;
}

(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.RUNEBENDER_CHROME });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    // Replace only the picker. The host writes, prunes and reopens real directory handles.
    await page.addInitScript(() => {
      window.showDirectoryPicker = async () => window.testFontDirectory;
    });
    await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4326/');
    await page.waitForFunction(() => window.runebender);
    const files = readFiles(root);
    const master = 'HOIShapes-Regular.ufo';
    const emptyKerning = '<?xml version="1.0"?><plist version="1.0"><dict><!-- deliberately empty --></dict></plist>';
    files.push([`${master}/features.fea`, [...Buffer.from('languagesystem DFLT dflt;\n')]]);
    files.push([`${master}/kerning.plist`, [...Buffer.from(emptyKerning)]]);
    const error = await page.evaluate(files => window.runebender.openFiles(
      null, files.map(([name, bytes]) => [name, new Uint8Array(bytes)])), files);
    assert.equal(error, '', 'HOI Shapes opens in the actual browser host');
    const state = await page.evaluate(() => window.runebender.state());
    assert.equal(state.source, 'HOIShapes.designspace');
    assert.equal(state.glyph_count, 6);
    assert.equal(state.modified, false);
    const save = await page.evaluate(() => {
      const bundle = window.runebender.saveFiles();
      return bundle && bundle.files.map(([name, bytes]) => [name, Array.from(bytes)]);
    });
    assert.ok(save, 'the browser produces a save package');
    const written = new Map(save);
    let count = 0;
    for (const [name, bytes] of files) {
      if (!name.endsWith('.glif')) continue;
      assert.deepEqual(written.get(name), bytes, `original bytes retained: ${name}`);
      count++;
    }
    assert.equal(count, 18, 'all six glyphs in all three masters were checked');
    // Saves are incremental: an unchanged Designspace stays in the existing source package.
    const source = new Map(files).get('HOIShapes.designspace');
    const authoring = await page.evaluate(xmls => {
      function value(node) {
        if (node.tagName === 'dict') {
          const result = {};
          const children = [...node.children];
          for (let i = 0; i < children.length; i += 2) result[children[i].textContent] = value(children[i + 1]);
          return result;
        }
        if (node.tagName === 'array') return [...node.children].map(value);
        if (node.tagName === 'true' || node.tagName === 'false') return node.tagName === 'true';
        if (node.tagName === 'integer' || node.tagName === 'real') return Number(node.textContent);
        return node.textContent;
      }
      return xmls.map(xml => {
        const document = new DOMParser().parseFromString(xml, 'text/xml');
        const lib = document.querySelector('lib > dict');
        return lib ? value(lib)['com.runebender.hoiTrajectories'] ?? null : null;
      });
    }, [source, written.get('HOIShapes.designspace') ?? source].map(bytes => Buffer.from(bytes).toString('utf8')));
    assert.deepEqual(authoring[1], authoring[0], 'scoped HOI records survive the browser save boundary');

    const fileCommand = async (keys, note) => {
      await page.locator('canvas').focus();
      await page.keyboard.press(keys);
      await page.waitForFunction(note => {
        const state = window.runebender.state();
        return state.note.startsWith(note) && (!note.startsWith('Saved') || !state.modified);
      }, note);
    };
    await page.evaluate(async () => {
      const root = await navigator.storage.getDirectory();
      window.testFontDirectory = await root.getDirectoryHandle('family-copy', { create: true });
      const ufo = await window.testFontDirectory.getDirectoryHandle('HOIShapes-Regular.ufo', { create: true });
      const glyphs = await ufo.getDirectoryHandle('glyphs', { create: true });
      window.externalGlyph = await glyphs.getFileHandle('external.glif', { create: true });
      const stream = await window.externalGlyph.createWritable();
      await stream.write('outside work');
      await stream.close();
    });
    await fileCommand('Control+Shift+s', 'Saved HOIShapes.designspace');
    assert.equal(await page.evaluate(async () => (await window.externalGlyph.getFile()).text()), 'outside work',
      'Save As leaves unrelated glyphs in an existing destination');
    const copiedNames = await page.evaluate(async () => {
      const names = [];
      for await (const name of window.testFontDirectory.keys()) names.push(name);
      return names.sort();
    });
    assert.deepEqual(copiedNames, [...new Set(files.map(([path]) => path.split('/')[0]))].sort(),
      'Save As writes the designspace and every master into the destination');
    await fileCommand('Control+o', 'Opened HOIShapes.designspace');
    assert.equal((await page.evaluate(() => window.runebender.state())).glyph_count, state.glyph_count,
      'the saved family reopens from its own directory');

    await page.mouse.click(1068, 451); // Features disclosure
    await page.mouse.click(1044, 480); // shared features.fea field
    await page.waitForFunction(() => document.activeElement.id === 'text-input');
    await page.keyboard.press('Control+a');
    await page.keyboard.press('Backspace');
    await page.mouse.click(1244, 739); // Apply shared features
    await page.waitForFunction(() => window.runebender.state().modified);
    await fileCommand('Control+s', 'Saved HOIShapes.designspace');
    const optionalTables = await page.evaluate(async master => {
      const ufo = await window.testFontDirectory.getDirectoryHandle(master);
      const names = [];
      for await (const name of ufo.keys()) names.push(name);
      const kerning = await ufo.getFileHandle('kerning.plist');
      return { featuresPresent: names.includes('features.fea'), kerning: await (await kerning.getFile()).text() };
    }, master);
    assert.equal(optionalTables.featuresPresent, false, 'clearing all feature code removes the owned file on Save');
    assert.equal(optionalTables.kerning, emptyKerning, 'ordinary Save retains an intentionally empty original table byte for byte');

    // Open the UFO itself. A failed copy must leave ordinary Save rooted at that UFO.
    const openBox = async () => {
      await page.mouse.dblclick(425, 150, { delay: 100 });
      await page.waitForFunction(() => {
        const state = window.runebender.state();
        return state.mode === 'editor' && state.glyph === 'box';
      });
      return page.evaluate(() => window.runebender.state().points);
    };
    const nudgeBox = async () => {
      await page.mouse.click(650, 350);
      await page.keyboard.press('Control+a');
      await page.keyboard.press('ArrowRight');
      await page.waitForFunction(() => window.runebender.state().modified);
      return page.evaluate(() => window.runebender.state().points);
    };
    await page.evaluate(async master => {
      window.testFontDirectory = await window.testFontDirectory.getDirectoryHandle(master);
    }, master);
    await fileCommand('Control+o', `Opened ${master}`);
    await page.evaluate(async () => {
      const glyphs = await window.testFontDirectory.getDirectoryHandle('glyphs');
      window.newExternalGlyph = await glyphs.getFileHandle('new-external.glif', { create: true });
      const stream = await window.newExternalGlyph.createWritable();
      await stream.write('new outside work');
      await stream.close();
    });
    await page.evaluate(async master => {
      const root = await navigator.storage.getDirectory();
      const blocked = await root.getDirectoryHandle('blocked', { create: true });
      await blocked.getFileHandle(master, { create: true }); // A file cannot hold UFO contents.
      window.testFontDirectory = blocked;
    }, master);
    await fileCommand('Control+Shift+s', 'Save failed');
    const original = await openBox();
    const firstEdit = await nudgeBox();
    assert.notDeepEqual(firstEdit, original);
    await fileCommand('Control+s', `Saved ${master}`);
    assert.equal(await page.evaluate(async () => (await window.newExternalGlyph.getFile()).text()), 'new outside work',
      'ordinary Save preserves a glyph added externally after Open');
    await page.evaluate(async master => {
      const root = await navigator.storage.getDirectory();
      const copy = await root.getDirectoryHandle('family-copy');
      window.testFontDirectory = await copy.getDirectoryHandle(master);
    }, master);
    await fileCommand('Control+o', `Opened ${master}`);
    assert.deepEqual(await openBox(), firstEdit, 'ordinary Save kept writing into the original UFO');
    await page.evaluate(async () => {
      const root = await navigator.storage.getDirectory();
      window.testFontDirectory = await root.getDirectoryHandle('single-copy', { create: true });
    });
    await fileCommand('Control+Shift+s', `Saved ${master}`);
    const pendingEdit = await nudgeBox();
    await page.evaluate(() => {
      const create = FileSystemFileHandle.prototype.createWritable;
      FileSystemFileHandle.prototype.createWritable = async function (...args) {
        if (this.name === 'box.glif') {
          FileSystemFileHandle.prototype.createWritable = create;
          await new Promise(resolve => { window.finishFirstWrite = resolve; });
        }
        return create.apply(this, args);
      };
    });
    await page.keyboard.press('Control+s');
    await page.waitForFunction(() => window.finishFirstWrite);
    const secondEdit = await nudgeBox();
    assert.notDeepEqual(secondEdit, pendingEdit);
    await page.keyboard.press('Control+s');
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.evaluate(() => window.finishFirstWrite());
    await page.waitForFunction(() => !window.runebender.state().modified);
    await fileCommand('Control+o', `Opened ${master}`);
    assert.deepEqual(await openBox(), secondEdit, 'the new copy contains the subsequent edit');

    const sameFolderEdit = await nudgeBox();
    await fileCommand('Control+Shift+s', `Saved ${master}`);
    await fileCommand('Control+o', `Opened ${master}`);
    assert.deepEqual(await openBox(), sameFolderEdit, 'Save As into the current folder can update owned files');

    // A denied write offers an editable copy while keeping the original dirty for retry.
    const deniedEdit = await nudgeBox();
    await page.evaluate(() => {
      const write = FileSystemFileHandle.prototype.createWritable;
      FileSystemFileHandle.prototype.createWritable = async function () {
        FileSystemFileHandle.prototype.createWritable = write;
        throw new DOMException('write access revoked', 'NotAllowedError');
      };
    });
    const [backup] = await Promise.all([
      page.waitForEvent('download'),
      fileCommand('Control+s', 'Folder write denied'),
    ]);
    assert.match(backup.suggestedFilename(), /\.zip$/);
    assert.equal((await page.evaluate(() => window.runebender.state())).modified, true);
    const backupBox = execFileSync('python3', ['-c',
      'import sys, zipfile; print(zipfile.ZipFile(sys.argv[1]).read(sys.argv[2]).decode(), end="")',
      await backup.path(), master + '/glyphs/box.glif'], { encoding: 'utf8' });
    await fileCommand('Control+s', `Saved ${master}`);
    const retriedBox = await page.evaluate(async master => {
      const ufo = await window.testFontDirectory.getDirectoryHandle(master);
      const glyphs = await ufo.getDirectoryHandle('glyphs');
      const file = await glyphs.getFileHandle('box.glif');
      return (await file.getFile()).text();
    }, master);
    assert.equal(backupBox, retriedBox, 'the downloaded copy contains the pending edit');
    await fileCommand('Control+o', `Opened ${master}`);
    assert.deepEqual(await openBox(), deniedEdit, 'the original received the retried edit');

    // File → Download Editable Sources is also available while the writable UFO is clean.
    await page.mouse.click(112, 15);
    const [explicitCopy] = await Promise.all([
      page.waitForEvent('download'),
      page.mouse.click(146, 191),
    ]);
    assert.match(explicitCopy.suggestedFilename(), /\.zip$/);
    assert.equal((await page.evaluate(() => window.runebender.state())).modified, false);

    await nudgeBox();
    await page.evaluate(async master => {
      const ufo = await window.testFontDirectory.getDirectoryHandle(master);
      const glyphs = await ufo.getDirectoryHandle('glyphs');
      window.changedGlyph = await glyphs.getFileHandle('box.glif');
      const stream = await window.changedGlyph.createWritable();
      await stream.write('externally changed box');
      await stream.close();
    }, master);
    await fileCommand('Control+s', 'Save failed');
    assert.match((await page.evaluate(() => window.runebender.state())).note, /box\.glif changed outside/);
    assert.equal(await page.evaluate(async () => (await window.changedGlyph.getFile()).text()), 'externally changed box');
    assert.equal((await page.evaluate(() => window.runebender.state())).modified, true);

    assert.deepEqual(errors, [], 'no browser errors');
    console.log(`HOI Shapes browser preservation passed: ${count} original GLIFs unchanged`);
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exit(1); });
