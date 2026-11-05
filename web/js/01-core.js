/* LSP Converter - UI (forked from ACECM's)

   Ordered classic scripts, loaded by index.html in number order, sharing ONE
   global scope:

     01-core        helpers, api(), toast, the confirm box, uploads, drag-drop
     08-settings    Settings
     14-logs        Diagnostics > Logs
     14c-convert    Convert
     14d-about      About
     14e-drift      Drift, the little game
     15-nav         pages, sections, router, themes - and boot (keep LAST)

   ⚠ Not ES modules, on purpose: pages call each other by bare name and the
   router rebinds them (keepPlace), which module scope would break. A
   top-level `const`/`let` is visible to every later file, so names must stay
   unique across all of them - a duplicate is a load-time SyntaxError. */
const $ = (s, r) => (r || document).querySelector(s);
const el = (t, c, h) => { const e = document.createElement(t);
  if (c) e.className = c; if (h != null) e.innerHTML = h; return e; };
const esc = s => String(s ?? '').replace(/[&<>"]/g,
  c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

async function api(path, body) {
  const opt = body ? { method: 'POST', body: JSON.stringify(body) } : {};
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), 12000);
  try {
    const r = await fetch('/api/' + path, { ...opt, signal: ctrl.signal });
    const j = await r.json().catch(() => ({ error: 'bad response' }));
    if (j && j.error) toast(j.error, true);
    return j;
  } catch (e) {
    const msg = (e && e.name === 'AbortError')
      ? (path + ' timed out') : String(e && e.message || e);
    return { error: msg };
  } finally {
    clearTimeout(t);
  }
}

// api() for the slow ones (unpacking a dropped mod): 30 minutes, not 12 s
async function apiLong(path, body, ms) {
  const opt = body ? { method: 'POST', body: JSON.stringify(body) } : {};
  const ctrl = new AbortController();
  const t = setTimeout(() => ctrl.abort(), ms || 30 * 60 * 1000);
  try {
    const r = await fetch('/api/' + path, { ...opt, signal: ctrl.signal });
    const j = await r.json().catch(() => ({ error: 'bad response' }));
    if (j && j.error && !j.need_confirm) toast(j.error, true);
    return j;
  } catch (e) {
    const msg = (e && e.name === 'AbortError')
      ? (path + ' timed out') : String(e && e.message || e);
    return { error: msg };
  } finally {
    clearTimeout(t);
  }
}

let toastT;
function toast(msg, bad) {
  const t = $('#toast');
  if (!t) return;
  t.textContent = msg;
  t.style.borderLeftColor = bad ? 'var(--red)' : 'var(--accent)';
  t.classList.add('show');
  clearTimeout(toastT);
  toastT = setTimeout(() => t.classList.remove('show'), 3200);
}

/* ⚠ Our OWN confirmation, because window.confirm() is not reliable here.
   The embedded webview returns FALSE from confirm() without ever showing a
   dialog, so every `if (!await ask(...)) return;` silently did nothing. This
   renders a real element, so it cannot be suppressed by the host.
   Returns a promise; call it as `if (!await ask(msg)) return;`. */
function ask(msg, okLabel) {
  return new Promise(resolve => {
    const veil = el('div', 'pk-veil ask-veil');
    const box = el('div', 'pk-box ask-box');
    const body = el('div', 'ask-msg');
    // the messages carry newlines and bullets - keep them readable
    body.textContent = String(msg == null ? '' : msg);
    const row = el('div', 'row ask-row');
    const no = el('button', 'sm', 'Cancel');
    const yes = el('button', 'sm primary', okLabel || 'OK');
    let done = false;
    const close = (val) => {
      if (done) return;
      done = true;
      removeEventListener('keydown', onKey, true);
      veil.remove();
      resolve(val);
    };
    const onKey = (e) => {
      if (e.key === 'Escape') { e.preventDefault(); close(false); }
      else if (e.key === 'Enter') { e.preventDefault(); close(true); }
    };
    no.onclick = () => close(false);
    yes.onclick = () => close(true);
    veil.onclick = (e) => { if (e.target === veil) close(false); };
    addEventListener('keydown', onKey, true);
    row.append(no, yes);
    box.append(body, row);
    veil.append(box);
    document.body.append(veil);
    yes.focus();
  });
}

/* ---- sidebar status: the app's version */
async function paintSideStat() {
  const box = $('#sidestat');
  if (!box) return;
  const v = await api('version');
  box.innerHTML = v.version ? `<div class="ver">v${esc(v.version)}</div>` : '';
}

/* ---- dropped files ------------------------------------------------------
   An AC1 mod archive dropped anywhere in the window is uploaded into a drop
   folder of its own (/api/drop/part), then unpacked and picked on the
   Convert page (cvDropImport, 14c-convert.js). */
function dropId() {
  if (crypto.randomUUID) return crypto.randomUUID();
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, c => {
    const r = Math.random() * 16 | 0;
    return (c === 'x' ? r : (r & 0x3 | 0x8)).toString(16);
  });
}

/* A dropped mod can be a gigabyte. One toast and then nothing for minutes is
   indistinguishable from the app having hung, so the upload shows a bar.
   ⚠ fetch() cannot report UPLOAD progress; only XMLHttpRequest exposes
   upload.onprogress. That is the whole reason putFile is not a fetch call. */
function dropProgress() {
  let box = $('#dropprog');
  if (!box) {
    box = el('div');
    box.id = 'dropprog';
    box.innerHTML = '<div class="dp-what"></div>'
      + '<div class="dp-track"><div class="dp-bar"></div></div>'
      + '<div class="dp-detail tiny dim"></div>';
    document.body.append(box);
  }
  const bar = box.querySelector('.dp-bar');
  return {
    show(what) {
      box.classList.add('on');
      box.querySelector('.dp-what').textContent = what;
      bar.style.width = '0%';
      bar.classList.remove('indet');
      box.querySelector('.dp-detail').textContent = '';
    },
    set(frac, detail) {
      bar.classList.remove('indet');
      bar.style.width = Math.max(0, Math.min(1, frac)) * 100 + '%';
      if (detail != null) box.querySelector('.dp-detail').textContent = detail;
    },
    // the server side gives no byte-level feedback, so say so honestly with a
    // moving bar rather than a percentage we would be inventing
    busy(what, detail) {
      box.querySelector('.dp-what').textContent = what;
      bar.style.width = '100%';
      bar.classList.add('indet');
      if (detail != null) box.querySelector('.dp-detail').textContent = detail;
    },
    hide() { box.classList.remove('on'); },
  };
}

function mb(n) { return (n / 1048576).toFixed(1) + ' MB'; }

function putFile(url, file, onProgress) {
  return new Promise(resolve => {
    const xhr = new XMLHttpRequest();
    xhr.open('POST', url);
    xhr.setRequestHeader('Content-Type', 'application/octet-stream');
    xhr.upload.onprogress = e => {
      if (e.lengthComputable) onProgress(e.loaded, e.total);
    };
    xhr.onload = () => {
      let j;
      try { j = JSON.parse(xhr.responseText); }
      catch (e) { j = { error: 'upload failed (' + xhr.status + ')' }; }
      resolve(j);
    };
    xhr.onerror = () => resolve({ error: 'upload failed - connection lost' });
    xhr.onabort = () => resolve({ error: 'upload cancelled' });
    xhr.send(file);
  });
}

// every file in a drop, folders walked (one level of 40 entries at most)
function collectDropped(dt) {
  return new Promise(resolve => {
    const items = dt && dt.items ? [...dt.items] : [];
    const files = [];
    let pending = 0;
    const finish = () => { if (!pending) resolve(files); };
    const walk = (entry) => {
      if (!entry) return;
      if (entry.isFile) {
        pending++;
        entry.file(f => { files.push(f); pending--; finish(); },
                   () => { pending--; finish(); });
      } else if (entry.isDirectory) {
        const reader = entry.createReader();
        pending++;
        reader.readEntries(ents => {
          pending--;
          if ((ents || []).length > 40) files.tooMany = true;
          else (ents || []).forEach(walk);
          finish();
        }, () => { pending--; finish(); });
      }
    };
    const entries = items
      .map(i => i.webkitGetAsEntry && i.webkitGetAsEntry())
      .filter(Boolean);
    if (entries.length) {
      entries.forEach(walk);
      finish();
    } else {
      resolve([...(dt.files || [])]);
    }
  });
}

function bindDropAnywhere() {
  const veil = $('#dropveil');
  if (!veil || veil.dataset.bound) return;
  veil.dataset.bound = '1';
  let depth = 0;
  const hasFiles = (e) => {
    const t = e.dataTransfer;
    return !!(t && [...(t.types || [])].includes('Files'));
  };
  document.addEventListener('dragenter', e => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    depth++;
    veil.classList.add('on');
  });
  document.addEventListener('dragover', e => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = 'copy';
  });
  document.addEventListener('dragleave', e => {
    if (!hasFiles(e)) return;
    depth = Math.max(0, depth - 1);
    if (!depth) veil.classList.remove('on');
  });
  document.addEventListener('drop', async e => {
    if (!hasFiles(e)) return;
    e.preventDefault();
    e.stopPropagation();
    depth = 0;
    veil.classList.remove('on');
    const files = await collectDropped(e.dataTransfer);
    if (!files.length || files.tooMany || !files.every(cvIsArchive)) {
      toast('Drop an Assetto Corsa mod archive (.zip, .7z, .rar or .tar) to convert it', true);
      return;
    }
    if (_page !== 'convert') go('convert');
    await cvDropImport(files);
  });
}

/* ---- links ------------------------------------------------------------------
   ⚠ The window is WebView2: a web or email link clicked in it would load in
   the app's own window, or nowhere. Every https / mailto link on any page
   goes to this PC's browser or mail app instead (/api/open, procs.rs). */
const LSP_SITE = 'https://lashushperformance.com';
async function openLink(url) {
  const j = await api('open', { url });
  if (j && j.ok) return;
  const bare = url.replace(/^mailto:/, '');
  navigator.clipboard.writeText(bare).then(
    () => toast('Couldn\'t open ' + bare + '. It\'s copied: paste it into your browser or email', true),
    () => toast('Couldn\'t open ' + bare, true));
}
// a link out of the app: the address (no https:// or mailto:) unless told otherwise
function extLink(url, text) {
  const a = document.createElement('a');
  a.className = 'ext';
  a.href = url;
  a.textContent = text || url.replace(/^(mailto:|https:\/\/)/, '');
  a.title = url.replace(/^mailto:/, '');
  return a;
}
// text with its web addresses and email addresses as links (the licence
// texts on About); http:// ones open as https://
function linkify(text) {
  const frag = document.createDocumentFragment();
  const re = /\bhttps?:\/\/[^\s<>"'`]+|[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}/g;
  let at = 0;
  for (const m of text.matchAll(re)) {
    let s = m[0];
    // a sentence's full stop or a closing bracket isn't part of the address
    const cut = s.match(/[.,;:!?)\]}>]+$/);
    if (cut) s = s.slice(0, -cut[0].length);
    if (!s) continue;
    frag.append(text.slice(at, m.index));
    const web = /^https?:\/\//.test(s);
    frag.append(extLink(web ? s.replace(/^http:/, 'https:') : 'mailto:' + s, s));
    at = m.index + s.length;
  }
  frag.append(text.slice(at));
  return frag;
}
function _linkClick(e) {
  if (e.button > 1) return;
  const a = e.target.closest && e.target.closest('a[href]');
  if (!a) return;
  const href = a.getAttribute('href') || '';
  if (!/^(https:\/\/|mailto:)/i.test(href)) return;
  e.preventDefault();
  openLink(href);
}
document.addEventListener('click', _linkClick);
// a middle click would open a second app window
document.addEventListener('auxclick', _linkClick);

/* ---- the sidebar's gap, between Convert and More ---------------------------
   Empty on purpose, but for two quiet things: what the converter is doing
   (only while a car converts, or until you've seen how it went), and at the
   bottom a note that this is an early version, with our address. */
const _sjSeen = (() => { try { return sessionStorage.getItem('lsp.sidejob.seen') || ''; } catch (e) { return ''; } })();
let _sjLast = null, _sjSeenKey = _sjSeen, _sjTimer = 0;
const _sjNames = {};
function _sjBox() {
  let box = $('#sidejob');
  const nav = $('#sections');
  if (!box && nav) {
    box = el('a', 'side-job');
    box.id = 'sidejob';
    box.hidden = true;
    box.setAttribute('aria-live', 'polite');
    box.title = 'Open Convert';
    box.onclick = () => go('convert');
    nav.after(box);
  }
  return box;
}
function paintSideJob() {
  const box = _sjBox();
  const s = _sjLast;
  if (!box) return;
  if (!s || !s.ok || s.state === 'idle') { box.hidden = true; return; }
  const q = s.queue || {};
  const waiting = (q.waiting || []).length;
  const running = s.state === 'running' || !!q.current;
  const key = [s.id, s.state, s.secs].join('|');
  // on Convert the page itself says it; a result you've seen there goes
  // (_wanted: the page go() is opening, set before it has drawn)
  if ((_wanted || _page) === 'convert') {
    if (!running && key !== _sjSeenKey) {
      _sjSeenKey = key;
      try { sessionStorage.setItem('lsp.sidejob.seen', key); } catch (e) { /* the note just comes back after a reload */ }
    }
    box.hidden = true;
    return;
  }
  if (!running && key === _sjSeenKey) { box.hidden = true; return; }
  const car = _sjNames[s.car] || String(s.car || '').replace(/^track:/, '');
  const done = q.done || [];
  let head, name = car, meta = '', cls = '';
  if (running) {
    head = 'Converting';
    meta = Math.round(s.secs || 0) + ' s' + (waiting ? ' · ' + waiting + ' waiting' : '');
    cls = 'run';
  } else if (done.length > 1) {
    const ok = done.filter(d => d.ok).length;
    head = 'Queue finished';
    name = ok + ' of ' + done.length + ' converted';
    meta = 'Open Convert for the results';
    cls = ok === done.length ? 'ok' : 'bad';
  } else if (s.state === 'done') {
    head = 'Ready';
    meta = 'Open Convert for the result';
    cls = 'ok';
  } else if (s.state === 'cancelled') {
    head = 'Stopped';
    meta = 'Open Convert';
  } else {
    head = 'Didn\'t work';
    meta = 'Open Convert for the details';
    cls = 'bad';
  }
  box.className = 'side-job ' + cls;
  box.innerHTML = '<span class="sj-head">' + esc(head) + '</span>'
    + '<span class="sj-name">' + esc(name) + '</span>'
    + '<span class="sj-meta">' + esc(meta) + '</span>'
    + (running ? '<span class="sj-bar"><i></i></span>' : '');
  box.title = car ? car + ' - open Convert' : 'Open Convert';
  box.hidden = false;
}
// its own fetch, not api(): a poll every few seconds mustn't toast errors
async function pollSideJob() {
  clearTimeout(_sjTimer);
  try {
    const r = await fetch('/api/convert/status', { cache: 'no-store' });
    _sjLast = await r.json();
  } catch (e) { _sjLast = null; }
  paintSideJob();
  // the car's own name rather than its folder: looked up once per car
  const car = _sjLast && _sjLast.ok && _sjLast.state !== 'idle' ? String(_sjLast.car || '') : '';
  if (car && !car.startsWith('track:') && !(car in _sjNames)) {
    _sjNames[car] = '';
    try {
      const c = await (await fetch('/api/convert/cars', { cache: 'no-store' })).json();
      for (const x of (c && c.cars) || []) if (x.folder && x.name) _sjNames[x.folder] = x.name;
    } catch (e) { /* the folder name will do */ }
    paintSideJob();
  }
  const busy = _sjLast && (_sjLast.state === 'running' || (_sjLast.queue && _sjLast.queue.current));
  _sjTimer = setTimeout(pollSideJob, busy ? 2000 : 5000);
}
// a page change shows or hides it at once (go() sets the hash)
addEventListener('hashchange', () => setTimeout(paintSideJob, 0));
setTimeout(pollSideJob, 600);

async function paintSideNote() {
  const fill = document.querySelector('.side .side-fill');
  if (!fill || fill.querySelector('.side-note')) return;
  const v = await api('version');
  // 1.0.0 reads "v1.0"; 1.0.3 stays "v1.0.3"
  const ver = String(v.version || '').replace(/\.0$/, '');
  const mail = 'contact@lashushperformance.com';
  const note = el('div', 'side-note', (ver ? 'This is v' + esc(ver) + ', and we\'ll keep updating it. ' : 'We\'ll keep updating LSP Converter. ')
    + 'Any requests or issues:');
  const a = extLink('mailto:' + mail);
  // the address may break after the @, never mid-word
  a.innerHTML = 'contact@<wbr>lashushperformance.com';
  note.append(' ', a);
  fill.append(note);
}
paintSideNote();
