// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Index the small labels separately so opening the editor never reads every stored font.
function openDrafts() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('runebender-recovery', 1);
    request.onupgradeneeded = () => {
      request.result.createObjectStore('labels');
      request.result.createObjectStore('sources');
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function read(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

// IDB commits both stores together. An interrupted or quota-denied write leaves the last
// complete checkpoint readable, including when a closed tab's draft moves to a new session.
function write(db, edit) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(['labels', 'sources'], 'readwrite');
    tx.oncomplete = resolve;
    tx.onabort = () => reject(tx.error || new Error('Storage transaction aborted'));
    try {
      edit(tx.objectStore('labels'), tx.objectStore('sources'));
    } catch (error) {
      tx.abort();
      reject(error);
    }
  });
}

export function fontRecovery({ capture, restore, warn }) {
  const id = Array.from(crypto.getRandomValues(new Uint32Array(4)), n => n.toString(16).padStart(8, '0')).join('');
  const lock = key => 'runebender-draft-' + key;
  let database, observed, timer, deadline, retry = false, hasRecord = false, generation = 0;
  let lockProtected = false;
  const open = () => database ??= openDrafts().catch(error => { database = undefined; throw error; });
  const report = error => warn(String(error?.message || error));

  function cancel() {
    clearTimeout(timer);
    timer = undefined;
    deadline = undefined;
  }
  function clear() {
    cancel();
    observed = undefined;
    retry = false;
    generation++;
    if (hasRecord) {
      hasRecord = false;
      open().then(db => write(db, (labels, sources) => {
        labels.delete(id);
        sources.delete(id);
      })).catch(report);
    }
  }
  function flush() {
    if (timer === undefined && !retry) return;
    cancel();
    retry = false;
    const current = generation, snapshot = observed;
    const { revision, featureDraft, master } = snapshot;
    function failed(error) {
      if (current !== generation || observed !== snapshot) return;
      retry = true;
      report(error);
    }
    try {
      const { name, bytes } = capture();
      const label = { id, name, revision, master, savedAt: Date.now(), lockProtected };
      hasRecord = true;
      open().then(db => {
        if (current !== generation) return;
        return write(db, (labels, sources) => {
          labels.put(label, id);
          sources.put({ bytes, featureDraft }, id);
        });
      }).catch(failed);
    } catch (error) { failed(error); }
  }
  function observe(state, keepDownload) {
    if (state.revision == null) {
      if (observed && !keepDownload) clear();
      return;
    }
    if (observed && observed.revision === state.revision && observed.featureDraft === state.featureDraft &&
        observed.master === state.master) return;
    observed = { revision: state.revision, featureDraft: state.featureDraft, master: state.master };
    // During continuous input, checkpoint at least once every eight seconds.
    deadline ??= performance.now() + 8000;
    clearTimeout(timer);
    timer = setTimeout(flush, Math.max(0, Math.min(900, deadline - performance.now())));
  }

  async function offer() {
    let db;
    try { db = await open(); } catch { return false; } // Report storage failure on the first edit.
    let locks = navigator.locks;
    let active = new Set();
    if (locks) {
      // The browser releases this lock on navigation, tab close, or process death.
      try {
        await new Promise((resolve, reject) => {
          locks.request(lock(id), () => { lockProtected = true; resolve(); return new Promise(() => {}); }).catch(reject);
        });
        active = new Set((await locks.query()).held.map(entry => entry.name));
      } catch { locks = null; }
    }
    const all = await read(db.transaction('labels').objectStore('labels').getAll());
    const choices = all.filter(item => !active.has(lock(item.id))).sort((a, b) => b.savedAt - a.savedAt);
    if (!choices.length) return false;
    const dialog = document.querySelector('#recovery');
    const select = document.querySelector('#drafts');
    select.replaceChildren(...choices.map(item => new Option(
      item.name + ' · ' + new Date(item.savedAt).toLocaleString(), item.id)));
    // A checkpoint written without a lifetime lock may still belong to a running editor.
    const canRemove = () => Boolean(locks && choices.find(item => item.id === select.value)?.lockProtected);
    const discard = dialog.querySelector('[value="discard"]');
    const showActions = () => {
      discard.disabled = !canRemove();
      discard.title = discard.disabled ? 'Another tab may own this checkpoint. You can recover a copy.' : '';
    };
    select.onchange = showActions;
    showActions();
    const decision = new Promise(resolve => dialog.addEventListener('close', resolve, { once: true }));
    dialog.showModal();
    await decision;
    const action = dialog.returnValue;
    if (action !== 'restore' && action !== 'discard') return false;
    const old = select.value;
    async function use() {
      if (action === 'discard') {
        if (!canRemove()) return false;
        await write(db, (labels, sources) => { labels.delete(old); sources.delete(old); });
        return false;
      }
      const tx = db.transaction(['labels', 'sources']);
      const [label, source] = await Promise.all([
        read(tx.objectStore('labels').get(old)), read(tx.objectStore('sources').get(old)),
      ]);
      if (!label || !source) throw new Error('This checkpoint was already handled in another tab');
      restore(label, source);
      hasRecord = true;
      await write(db, (labels, sources) => {
        labels.put({ ...label, id, lockProtected }, id);
        sources.put(source, id);
        // Without Web Locks the other tab may still own this record. Restore a copy.
        if (canRemove()) {
          labels.delete(old);
          sources.delete(old);
        }
      });
      return true;
    }
    if (!locks) return use();
    return locks.request(lock(old), { ifAvailable: true }, held => {
      if (!held) throw new Error('This checkpoint is in use in another tab');
      return use();
    });
  }
  return { observe, flush, newDocument: clear, offer };
}
