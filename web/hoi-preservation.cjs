// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Exercise the actual browser package boundary with the small HOI Shapes fixture.
// All writes are returned in memory; no source font is modified.
const assert = require('node:assert/strict');
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
    await page.goto(process.env.RUNEBENDER_DEMO_URL || 'http://127.0.0.1:4326/');
    await page.waitForFunction(() => window.runebender);
    const files = readFiles(root);
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

    assert.deepEqual(errors, [], 'no browser errors');
    console.log(`HOI Shapes browser preservation passed: ${count} original GLIFs unchanged`);
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exit(1); });
