import init, { BrowserEditor, fontRecovery } from './pkg/runebender_browser.js';
const canvas = document.querySelector('#app');
const context = canvas.getContext('2d', { alpha: false });
const textInput = document.querySelector('#text-input');
let composing = false;
const loading = document.querySelector('#loading');
let editor, pending = false, stopped = false, scale = 1, lastFrame = 0, move;
let lastDown = { time: 0, x: 0, y: 0, count: 0 };
const heldKeys = new Map();
const measurements = { frames: 0, renderMs: [], width: 0, height: 0, scale: 1 };
const mods = e => (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
function fail(error) {
  stopped = true;
  loading.hidden = false;
  document.querySelector('#status').textContent = 'Runebender could not start';
  document.querySelector('#error').textContent = String(error);
  console.error(error);
}
// Fonts on disk: the File System Access API reads and writes the folder the user picks.
const folder = { handle: null, name: null, portable: false, busy: false };
const hasFolders = typeof window.showDirectoryPicker === 'function';
const recovery = fontRecovery({
  capture() {
    const name = folder.name || JSON.parse(editor.state()).source.split('/').pop();
    return { name, bytes: editor.recovery_bundle() };
  },
  restore(label, source) {
    const error = editor.open_bundle(null, bundle(unbundle(source.bytes).files));
    if (error) throw new Error(error);
    editor.restore_draft(label.master, source.featureDraft);
    // A checkpoint always opens as an independent copy, even if the original folder changed.
    Object.assign(folder, { name: label.name, handle: null, portable: true, archiveDownload: false });
  },
  warn(message) { editor.host_result('recovery', false, message); schedule(); },
});
function bundle(files) {
  let offset = 0;
  const manifest = new TextEncoder().encode(JSON.stringify(files.map(([path, bytes]) => {
    const entry = [path, offset, bytes.byteLength];
    offset += bytes.byteLength;
    return entry;
  })));
  const out = new Uint8Array(4 + manifest.length + offset);
  new DataView(out.buffer).setUint32(0, manifest.length, true);
  out.set(manifest, 4);
  let at = 4 + manifest.length;
  for (const [, bytes] of files) { out.set(bytes, at); at += bytes.byteLength; }
  return out;
}
function unbundle(bytes) {
  const length = new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, true);
  const manifest = JSON.parse(new TextDecoder().decode(bytes.subarray(4, 4 + length)));
  const data = bytes.subarray(4 + length);
  return { files: manifest.files.map(([path, offset, size]) => [path, data.subarray(offset, offset + size)]), prune: manifest.prune };
}
async function readTree(directory, prefix, out) {
  for await (const [name, handle] of directory.entries()) {
    if (skipEntry(name)) continue;
    if (handle.kind === 'file') out.push([prefix + name, new Uint8Array(await (await handle.getFile()).arrayBuffer())]);
    else await readTree(handle, `${prefix}${name}/`, out);
  }
}
// Include project sidecars and nested masters, leaving out Git internals and Finder metadata.
const skipEntry = name => name === '.git' || name === '.DS_Store';
async function writeFile(directory, path, bytes) {
  const parts = path.split('/');
  let at = directory;
  for (const part of parts.slice(0, -1)) at = await at.getDirectoryHandle(part, { create: true });
  const file = await at.getFileHandle(parts.at(-1), { create: true });
  const writable = await file.createWritable();
  try {
    await writable.write(bytes);
    await writable.close();
  } catch (error) {
    // Release the handle so a subsequent Save can retry this file.
    await writable.abort().catch(() => {});
    throw error;
  }
}
async function pruneGlifs(directory, path, written) {
  let at = directory;
  try {
    for (const part of path.split('/')) at = await at.getDirectoryHandle(part);
  } catch (error) {
    if (error.name === 'NotFoundError') return;
    throw error;
  }
  for await (const [name, handle] of at.entries()) {
    if (handle.kind === 'file' && name.endsWith('.glif') && !written.has(`${path}/${name}`)) await at.removeEntry(name);
  }
}
const handleStore = {
  open: () => new Promise((resolve, reject) => {
    const request = indexedDB.open('runebender', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('handles');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  }),
  async get() {
    try {
      const db = await this.open();
      return await new Promise((resolve, reject) => {
        const tx = db.transaction('handles');
        tx.oncomplete = tx.onabort = () => db.close();
        const request = tx.objectStore('handles').get('font');
        request.onsuccess = () => resolve(request.result ?? null);
        request.onerror = () => reject(request.error);
      });
    } catch { return null; }
  },
  async set(handle) {
    try {
      const db = await this.open();
      try {
        await new Promise((resolve, reject) => {
          const tx = db.transaction('handles', 'readwrite');
          tx.oncomplete = resolve;
          tx.onabort = () => reject(tx.error);
          if (handle) tx.objectStore('handles').put(handle, 'font');
          else tx.objectStore('handles').delete('font');
        });
      } finally { db.close(); }
    } catch { /* remembering the folder is a convenience */ }
  },
};
async function openSources(name, files, directory) {
  // A picker or directory read may finish after the user has made another edit.
  if (JSON.parse(editor.state()).modified) throw new Error('Save the current font before opening another folder');
  const ufoName = name.toLowerCase().endsWith('.ufo') && files.some(([path]) => path === 'metainfo.plist') ? name : null;
  const error = editor.open_bundle(ufoName, bundle(files));
  if (error) throw new Error(error);
  recovery.newDocument();
  folder.name = name;
  folder.portable = !directory;
  folder.archiveDownload = false;
  folder.handle = directory;
  await handleStore.set(directory);
}
async function openFolder(directory) {
  const files = [];
  await readTree(directory, '', files);
  await openSources(directory.name, files, directory);
}
async function importFolder(offerPicker) {
  const dialog = document.querySelector('#source-import');
  const input = document.querySelector('#source-folder');
  input.value = '';
  const selection = new Promise((resolve, reject) => {
    input.onchange = () => dialog.close('selected');
    input.oncancel = () => dialog.close();
    dialog.onclose = () => {
      input.onchange = input.oncancel = dialog.onclose = null;
      if (dialog.returnValue === 'selected' && input.files.length) resolve(Array.from(input.files));
      else reject(new DOMException('Folder selection cancelled', 'AbortError'));
    };
  });
  dialog.returnValue = '';
  dialog.showModal();
  if (offerPicker && navigator.userActivation?.isActive) {
    // An embedded host may deny a programmatic picker. The visible input still works.
    try { input.click(); } catch { /* choose the folder from the dialog */ }
  }
  const selected = await selection;
  input.value = '';
  const root = selected[0].webkitRelativePath.split('/')[0];
  const files = [];
  for (const file of selected) {
    const [name, ...parts] = file.webkitRelativePath.split('/');
    if (!root || name !== root || !parts.length) throw new Error('Choose one project folder');
    if (parts.some(skipEntry)) continue;
    files.push([parts.join('/'), new Uint8Array(await file.arrayBuffer())]);
  }
  await openSources(root, files, null);
}
async function chooseFolder() {
  if (!hasFolders) return importFolder(true);
  try {
    const directory = await window.showDirectoryPicker({ id: 'runebender-font', mode: 'read' });
    await openFolder(directory);
  } catch (error) {
    if (error.name === 'NotAllowedError' || error.name === 'SecurityError') return importFolder(false);
    throw error;
  }
}
async function saveFolder(intoNewFolder) {
  let directory = folder.handle;
  if (intoNewFolder || !directory) {
    directory = await window.showDirectoryPicker({ id: 'runebender-font', mode: 'readwrite' });
    intoNewFolder = true;
  }
  const bytes = editor.save_bundle(intoNewFolder);
  if (!bytes.length) return false;
  const { files, prune } = unbundle(bytes);
  const written = new Set();
  for (const [path, data] of files) { await writeFile(directory, path, data); written.add(path); }
  for (const path of prune) await pruneGlifs(directory, path, written);
  folder.handle = directory;
  folder.name = directory.name;
  folder.archiveDownload = false;
  await handleStore.set(directory);
  return true;
}
function downloadSources(permissionDenied) {
  const bytes = editor.source_zip();
  if (!bytes.length) return;
  const name = folder.name || JSON.parse(editor.state()).source.split('/').pop() || 'Runebender';
  const filename = name + '.zip';
  const url = URL.createObjectURL(new Blob([bytes], { type: 'application/zip' }));
  const link = document.createElement('a');
  link.download = filename;
  link.href = url;
  try {
    document.body.append(link);
    link.click();
  } finally {
    link.remove();
    // The browser may start reading the blob after the click handler returns.
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
  }
  recovery.flush();
  folder.archiveDownload = true;
  // A copy of a writable project leaves any edits pending for its source folder.
  const kind = permissionDenied ? 'download-denied' : folder.handle ? 'download-copy' : 'download';
  editor.host_result(kind, true, filename);
}
async function saveSources(intoNewFolder) {
  if (folder.portable || !hasFolders) {
    downloadSources(false);
    return;
  }
  try {
    if (await saveFolder(intoNewFolder)) editor.host_result('save', true, '');
  } catch (error) {
    if (!['NotAllowedError', 'SecurityError', 'NoModificationAllowedError'].includes(error.name)) throw error;
    downloadSources(true);
  }
}
async function hostRequest(request) {
  if (folder.busy) return;
  folder.busy = true;
  const kind = request === 'open' ? 'open' : 'save';
  try {
    if (request === 'open') {
      await chooseFolder();
      editor.host_result('open', true, '');
    } else if (request === 'download') downloadSources(false);
    else await saveSources(request === 'save-as');
  } catch (error) {
    editor.host_result(kind, false, error?.name === 'AbortError' ? '' : String(error?.message ?? error));
  } finally {
    folder.busy = false;
    schedule();
  }
}
canvas.addEventListener('dragover', e => e.preventDefault());
canvas.addEventListener('drop', async e => {
  e.preventDefault();
  const item = e.dataTransfer.items[0];
  if (!editor || !item?.getAsFileSystemHandle || folder.busy) return;
  folder.busy = true;
  try {
    if (JSON.parse(editor.state()).modified) throw new Error('Save the current font before opening another folder');
    const handle = await item.getAsFileSystemHandle();
    if (handle?.kind === 'directory') { await openFolder(handle); editor.host_result('open', true, ''); }
    else editor.host_result('open', false, 'drop a UFO folder, or a folder with a designspace');
  } catch (error) {
    editor.host_result('open', false, String(error?.message ?? error));
  } finally {
    folder.busy = false;
    schedule();
  }
});
function feedback() {
  const state = JSON.parse(editor.feedback());
  recovery.observe(state, folder.archiveDownload);
  canvas.style.cursor = state.cursor;
  if (state.request) hostRequest(state.request);
  if (state.clipboard != null && navigator.clipboard) {
    navigator.clipboard.writeText(state.clipboard).catch(() => {});
  }
  if (state.ime) {
    textInput.style.left = `${state.imePosition[0]}px`;
    textInput.style.top = `${state.imePosition[1]}px`;
  }
  if (state.ime && document.activeElement === canvas) {
    textInput.focus({ preventScroll: true });
    editor.text(3, '');
  } else if (!state.ime && document.activeElement === textInput) {
    canvas.focus({ preventScroll: true });
  }
  return state;
}
function draw(now) {
  pending = false;
  if (stopped || !editor) return;
  try {
    flushMove();
    const state = feedback();
    if (state.dirty || state.animate) {
      const start = performance.now();
      const bytes = editor.frame(lastFrame ? now - lastFrame : 16);
      context.putImageData(new ImageData(new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, bytes.byteLength), canvas.width, canvas.height), 0, 0);
      lastFrame = now;
      measurements.frames++;
      measurements.renderMs.push(performance.now() - start);
      if (measurements.renderMs.length > 120) measurements.renderMs.shift();
    }
    const next = feedback();
    if (next.dirty || next.animate) schedule();
  } catch (error) { fail(error); }
}
function schedule() {
  if (editor && !pending && !stopped && !document.hidden) {
    pending = true;
    requestAnimationFrame(draw);
  }
}
function resize() {
  const rect = canvas.getBoundingClientRect();
  const nextScale = window.devicePixelRatio || 1;
  const width = Math.max(1, Math.round(rect.width * nextScale));
  const height = Math.max(1, Math.round(rect.height * nextScale));
  if (canvas.width === width && canvas.height === height && scale === nextScale) return;
  scale = nextScale;
  canvas.width = width;
  canvas.height = height;
  Object.assign(measurements, { width, height, scale });
  editor?.resize(width, height, scale);
  schedule();
}
function sample(e) {
  const rect = canvas.getBoundingClientRect();
  return { x: (e.clientX - rect.left) * canvas.width / rect.width,
    y: (e.clientY - rect.top) * canvas.height / rect.height,
    button: e.button, buttons: e.buttons, modifiers: mods(e) };
}
function pointer(kind, e, count = 0, dx = 0, dy = 0) {
  editor.pointer(kind, e.x, e.y, e.button, e.buttons, count, e.modifiers, dx, dy);
}
function flushMove() {
  if (move) { pointer(0, move); move = undefined; }
}
canvas.addEventListener('pointerdown', e => {
  if (!editor) return;
  e.preventDefault();
  flushMove();
  canvas.focus({ preventScroll: true });
  canvas.setPointerCapture(e.pointerId);
  const at = sample(e);
  const count = e.timeStamp - lastDown.time < 450 && Math.hypot(at.x - lastDown.x, at.y - lastDown.y) < 6 * scale
    ? lastDown.count % 3 + 1 : 1;
  lastDown = { time: e.timeStamp, x: at.x, y: at.y, count };
  pointer(1, at, count);
  schedule();
});
canvas.addEventListener('pointerup', e => {
  if (!editor) return;
  e.preventDefault();
  flushMove();
  pointer(2, sample(e), lastDown.count);
  if (canvas.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId);
  schedule();
});
canvas.addEventListener('pointermove', e => { if (editor) { move = sample(e); schedule(); } });
canvas.addEventListener('pointercancel', e => {
  if (!editor) return;
  flushMove();
  pointer(2, { ...sample(e), buttons: 0 }, 1);
  schedule();
});
canvas.addEventListener('contextmenu', e => e.preventDefault());
canvas.addEventListener('wheel', e => {
  if (!editor) return;
  e.preventDefault();
  flushMove();
  const unit = e.deltaMode === 1 ? 20 : e.deltaMode === 2 ? canvas.clientHeight : 1;
  // Wheel distances are logical browser pixels; pointer positions are physical.
  pointer(3, sample(e), 0, -e.deltaX * unit, -e.deltaY * unit);
  schedule();
}, { passive: false });
for (const target of [canvas, textInput]) for (const name of ['keydown', 'keyup']) target.addEventListener(name, e => {
  if (!editor || e.key === 'F5' || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r')) return;
  // Let the browser produce its paste event; cancelling Cmd/Ctrl-V suppresses it.
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'v') return;
  if (e.isComposing || composing || e.key === 'Process' || e.key === 'Dead') return;
  e.preventDefault();
  if (name === 'keydown') heldKeys.set(e.code, e.key); else heldKeys.delete(e.code);
  editor.key(name === 'keydown', e.key, e.code, mods(e), e.repeat);
  schedule();
});
function blur() {
  if (!editor) return;
  flushMove();
  for (const [code, key] of heldKeys) editor.key(false, key, code, 0, false);
  heldKeys.clear();
  editor.pointer(2, -1, -1, 0, 0, 1, 0, 0, 0);
  editor.focus(false);
  schedule();
}
for (const target of [canvas, textInput]) target.addEventListener('blur', e => { if (e.relatedTarget !== canvas && e.relatedTarget !== textInput) blur(); });
for (const target of [canvas, textInput]) target.addEventListener('focus', () => { if (editor) { editor.focus(true); schedule(); } });
textInput.addEventListener('compositionstart', () => { composing = true; });
textInput.addEventListener('compositionupdate', e => { editor.text(1, e.data); schedule(); });
textInput.addEventListener('compositionend', e => {
  editor.text(2, e.data); composing = false; textInput.value = ''; schedule();
});
for (const target of [canvas, textInput]) target.addEventListener('paste', e => {
  if (!editor) return;
  e.preventDefault();
  editor.text(0, e.clipboardData.getData('text/plain'));
  schedule();
});
window.addEventListener('blur', blur);
window.addEventListener('resize', resize);
document.addEventListener('visibilitychange', () => {
  if (document.hidden) recovery.flush();
  else { lastFrame = 0; resize(); schedule(); }
});
new ResizeObserver(resize).observe(canvas);
function watchResolution() {
  const query = matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
  query.addEventListener('change', () => { resize(); watchResolution(); }, { once: true });
}
watchResolution();
// Some hosts change devicePixelRatio without a resize or media-query event.
// This checks one number; it never repaints an unchanged or hidden canvas.
setInterval(() => {
  if (!document.hidden && (window.devicePixelRatio || 1) !== scale) resize();
}, 250);
try {
  await init();
  resize();
  editor = new BrowserEditor(canvas.width, canvas.height, scale);
  let recovered = false;
  try {
    recovered = await recovery.offer();
  } catch (error) {
    editor.host_result('recovery', false, String(error?.message ?? error));
    recovered = folder.portable;
  }
  // Reopen a remembered folder only when the user hasn't recovered an independent draft.
  const remembered = hasFolders && !recovered ? await handleStore.get() : null;
  if (remembered) {
    try {
      if (await remembered.queryPermission({ mode: 'read' }) === 'granted') await openFolder(remembered);
    } catch (error) { editor.host_result('open', false, String(error?.message ?? error)); }
  }
  window.runebender = { state: () => JSON.parse(editor.state()),
    metrics: () => ({ ...measurements, renderMs: [...measurements.renderMs] }),
    // Test hooks: the same open and save paths without a folder picker.
    openFiles: (ufoName, files) => {
      const error = editor.open_bundle(ufoName, bundle(files));
      if (!error) recovery.newDocument();
      schedule();
      return error;
    },
    saveFiles: () => { const bytes = editor.save_bundle(false); return bytes.length ? unbundle(bytes) : null; },
    saved: () => { editor.host_result('save', true, ''); schedule(); } };
  draw(performance.now());
  loading.hidden = true;
  canvas.focus({ preventScroll: true });
} catch (error) { fail(error); }
