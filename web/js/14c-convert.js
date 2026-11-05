/* LSP Converter UI - Convert. One of the ordered classic scripts listed in
   index.html; they share one global scope (see 01-core.js).

   Pick an Assetto Corsa (AC1) car or track from a picture grid, press
   Convert, and it lands in the game's mods folder ready to try. The
   converter (aceconv, bundled) runs in the background at low priority; its
   own checks run on the result before anything is installed. The technical
   bits (paths, the EVO id, the converter's output) are folded away until
   wanted. Cars can also be ticked and converted one after another (the
   convert queue), each checked the same way. */
/* ------------------------------------------------------------- convert --- */
let cvTimer = null, cvPick = '', cvFilter = '', cvShow = 'all', cvSetupOpen = null, cvMode = 'car';
const cvPicks = { car: '', track: '' };
// the cars ticked for the queue (AC1 folders)
const cvSel = new Set();

/* An AC1 mod archive dropped on the Convert page: unpacked into the app's own
   convert_sources folder (not installed anywhere) and picked, ready to
   convert. Everything else dropped here still goes to the normal install. */
function cvIsArchive(f) { return /\.(zip|7z|rar|tar|tgz|tar\.gz)$/i.test(f.name || ''); }
async function cvDropImport(files) {
  const id = dropId();
  const prog = dropProgress();
  const total = files.reduce((n, f) => n + (f.size || 0), 0);
  let done = 0;
  prog.show('Copying ' + files.map(f => f.name).join(', ') + '…');
  try {
    for (const f of files) {
      const r = await putFile('/api/drop/part?id=' + encodeURIComponent(id) + '&name=' + encodeURIComponent(f.name), f,
        loaded => prog.set(total ? (done + loaded) / total : 0, `${f.name} - ${mb(done + loaded)} of ${mb(total)}`));
      if (!r.ok) { toast(r.error || 'Upload failed', true); return; }
      done += f.size || 0;
    }
    prog.busy('Unpacking…', 'looking for Assetto Corsa mods');
    const r = await apiLong('convert/import', { id });
    if (r.error) toast(r.error, !r.ok);
    if (!r.ok) return;
    const car = (r.cars || [])[0], trk = (r.tracks || [])[0];
    const n = (r.cars || []).length + (r.tracks || []).length;
    // a car pack wins over a track in the same drop; the rest are listed too
    cvMode = car ? 'car' : 'track';
    cvPicks[cvMode] = car || trk;
    cvShow = 'all';
    cvFilter = n === 1 ? (car || trk) : '';
    toast(n === 1 ? 'Ready to convert: ' + (car || trk) : n + ' mods added - ' + (car || trk) + ' is picked');
    convertPage();
  } finally {
    prog.hide();
  }
}

async function cvAddFolder(st) {
  const r = await api('pick_folder', { start: (st.extra_dirs || '').split(';').pop() || '' });
  if (!r.ok) { toast(r.error || 'Could not open the folder picker', true); return; }
  if (!r.path) return;
  const have = (st.extra_dirs || '').split(';').map(x => x.trim()).filter(Boolean);
  if (!have.some(x => x.toLowerCase() === r.path.toLowerCase())) have.push(r.path);
  const s = await api('convert/settings', { extra_dirs: have.join(';') });
  if (!s.ok) { toast(s.error || 'Could not save', true); return; }
  toast('Added ' + r.path);
  convertPage();
}

/* The page's look is in app.css (the Convert block): the library on the
   left, the picked car and its Convert button in a pane on the right. */

/* "01 Pick a car": numbered, because the steps come in order */
const cvHead = (n, text) => '<div class="cv-head"><h2><span class="num">' + n + '</span>' + text + '</h2></div>';

function cvAgo(t) {
  if (!t) return '';
  const s = Date.now() / 1000 - t;
  if (s < 90) return 'just now';
  if (s < 5400) return Math.round(s / 60) + ' min ago';
  if (s < 129600) return Math.round(s / 3600) + ' h ago';
  return Math.round(s / 86400) + ' days ago';
}

/* The converter's check (check-car / check-track), in words. */
function cvCheckBox(c) {
  const box = el('div', 'cv-check');
  if (!c) return box;
  const lines = c.lines || [];
  const errs = lines.filter(l => /^\s+ERROR /.test(l)).map(l => l.replace(/^\s+ERROR /, ''));
  const warns = lines.filter(l => /^\s+warn /.test(l)).map(l => l.replace(/^\s+warn\s+/, ''));
  box.innerHTML = 'Check: <b class="' + (c.errors ? 'bad' : 'ok') + '">' + c.errors + ' error' + (c.errors === 1 ? '' : 's') + '</b>, '
    + c.warnings + ' warning' + (c.warnings === 1 ? '' : 's');
  const items = errs.map(e => '<li class="bad">' + esc(e) + '</li>').concat(warns.map(w => '<li>' + esc(w) + '</li>'));
  if (items.length) box.append(el('ul', '', items.join('')));
  return box;
}

async function convertPage() {
  const p = $('#page');
  p.innerHTML = '';
  if (cvTimer) { clearInterval(cvTimer); cvTimer = null; }
  const root = el('div');
  root.id = 'cv-root';
  p.append(root);

  const T = cvMode === 'track';
  const KIND = T ? 'tracks' : 'cars';
  cvPick = cvPicks[cvMode];
  // ⚠ The first list after the app starts is a cold scan of the whole AC1
  // library (~20 s on a big one). api()'s 12 s gave up on it and left the
  // page blank until Convert was clicked again; wait for it, and say so.
  const wait = el('div', 'empty', 'Reading your Assetto Corsa ' + KIND + '… the first look after the app starts takes a little while.');
  root.append(wait);
  const [info, raw] = await Promise.all([api('convert'), apiLong(T ? 'convert/tracks' : 'convert/cars', null, 180000)]);
  wait.remove();
  // ⚠ cars only, with no Tracks tab, unless the server offers tracks
  // (installing a track changes the game's content.kspkg)
  if (T && !info.tracks) { cvMode = 'car'; return convertPage(); }
  if (info.tracks) $('#sub').textContent = 'Assetto Corsa cars and tracks into EVO mods';
  const cars = { ok: raw.ok, error: raw.error, cars: T ? (raw.tracks || []) : (raw.cars || []) };
  const noun = T ? 'track' : 'car';
  const st = info.settings || {}, have = info.present || {};
  const missing = Object.entries({ ac1_root: 'Assetto Corsa (the first game)', aceconv: 'the converter',
    game_content: 'Assetto Corsa EVO' }).filter(([k]) => !have[k]).map(([, v]) => v);
  if (cvSetupOpen === null) cvSetupOpen = missing.length > 0;

  // ---- problems first, in words
  if (missing.length) {
    root.append(el('div', 'cv-banner', '<b>Not ready yet.</b> Can\'t find ' + esc(missing.join(', '))
      + '. Fix the paths under <b>Setup</b> below' + (have.game_content ? '' : ' (the EVO game folder is in Settings)') + '.'));
  }
  if (info.game_running) {
    root.append(el('div', 'cv-banner', T
      ? 'The game is running. Close it before installing a track - installing changes content.kspkg, which the game keeps open.'
      : 'The game is running. You can convert, but the car only shows up after a game restart, '
        + 'a car the game has open can\'t be replaced, and its saved cameras are only reset with the game closed.'));
  }

  // ---- 1. pick a car
  const modes = el('div', 'seg cv-modes');
  for (const [k, label] of [['car', 'Cars'], ['track', 'Tracks']]) {
    const b = el('button', 'cv-chip' + (cvMode === k ? ' on' : ''), label);
    b.onclick = () => { if (cvMode !== k) { cvPicks[cvMode] = cvPick; cvMode = k; cvFilter = ''; cvShow = 'all'; convertPage(); } };
    modes.append(b);
  }
  if (info.tracks) root.append(modes);
  // the library on the left, the picked one in a pane on the right
  const lay = el('div', 'cv-layout'), libCol = el('div', 'cv-lib'), sideCol = el('div', 'cv-side');
  lay.append(libCol, sideCol);
  root.append(lay);
  const pc = el('div', 'card');
  pc.innerHTML = cvHead('01', 'Pick a ' + noun);
  const bar = el('div', 'cv-tools');
  const filt = el('input', 'cv-search');
  filt.placeholder = 'Search by name, brand or folder…';
  filt.value = cvFilter;
  const chips = el('div', 'seg');
  const chip = (k, label) => { const b = el('button', 'cv-chip' + (cvShow === k ? ' on' : ''), label);
    b.onclick = () => { cvShow = k; chips.querySelectorAll('.cv-chip').forEach(x => x.classList.remove('on')); b.classList.add('on'); paint(); }; return b; };
  chips.append(chip('all', 'All'), chip('new', 'Not converted'), chip('done', 'Converted'));
  const count = el('span', 'tiny dim cv-count');
  const addDir = el('button', 'sm', 'Add folder…');
  addDir.title = 'Convert ' + noun + 's that are not installed in Assetto Corsa: pick a folder of downloaded mods. '
    + 'You can also drop a mod .zip anywhere on this page.';
  addDir.onclick = () => cvAddFolder(st);
  bar.append(filt, chips, addDir);
  pc.firstChild.append(count);
  // the queue's selection (cars): tick cards, or take all the shown ones
  const selRow = el('div', 'row wrap cv-selrow');
  const selAll = el('button', 'sm', 'Select all shown');
  selAll.title = 'Tick every car the search and filter show (with "Not converted": every car not converted yet)';
  const selNone = el('button', 'sm', 'Clear selection');
  const selInfo = el('span', 'tiny dim', 'Tick cars to convert several, one after another.');
  selRow.append(selAll, selNone, selInfo);
  selRow.hidden = T;
  const grid = el('div', 'cv-grid' + (T ? ' tracks' : ''));
  pc.append(bar, selRow, grid);
  libCol.append(pc);

  // ---- 2. convert
  const cc = el('div', 'card');
  cc.innerHTML = cvHead('02', 'Convert');
  const hero = el('div', 'cv-hero');
  // nothing picked yet: the crest and the line, in place of the picture
  const noneHtml = '<div class="cv-none"><img src="/img/crest.png" alt="">'
    + '<div class="display">Pick a ' + noun + '.<span class="l2">Then send it.</span></div></div>';
  const noneSub = 'Click one in the list, or drop a mod archive anywhere on this window.';
  const himg = el('div', 'cv-img', noneHtml);
  const hside = el('div', 'grow');
  const htitle = el('div', 'cv-name');
  const hsub = el('div', 'tiny dim', noneSub);
  // a running queue's cars, small: done, converting, waiting
  const qStrip = el('div', 'cv-strip');
  qStrip.hidden = true;
  const opts = el('div', 'cv-opts');
  // ticked to start with: unticked, a rebuild leaves the car out of the game
  const cbC = el('label', 'tiny', '<input type="checkbox" checked> Install in the game when conversion succeeds');
  opts.append(cbC);
  // brakes stronger than any stock car's come down to a stock strength
  // unless this is ticked (the converter's --brakes mod); cars only
  const cbB = el('label', 'tiny', '<input type="checkbox"> Keep the mod\'s own brakes, even when stronger than any stock car');
  cbB.title = 'Off: brakes far stronger than the tyres can use (past every stock EVO car) are brought down to a typical stock strength. On: the mod\'s brakes as they are.';
  cbB.hidden = T;
  opts.append(cbB);
  // a steering lock below the slider's degrees is raised to it (the
  // converter's --min-lock); off by default, cars only
  const cbL = el('label', 'tiny', '<input type="checkbox"> Raise a short steering lock to at least ');
  cbL.title = 'Off: the mod\'s own steering lock. On: a lock below this many degrees each way is raised to it, so the wheels turn further (never past a typical stock car\'s angle; past that the steering is made slower instead). Stock EVO cars run 217 to 630, most around 440.';
  const lockIn = el('input');
  lockIn.type = 'range'; lockIn.min = '270'; lockIn.max = '540'; lockIn.step = '10'; lockIn.value = '360';
  lockIn.style.cssText = 'width:120px;vertical-align:middle';
  const lockOut = el('span', '', '360°');
  lockIn.oninput = () => { lockOut.textContent = lockIn.value + '°'; };
  const lockTick = cbL.querySelector('input');
  lockIn.disabled = true;
  lockTick.onchange = () => { lockIn.disabled = !lockTick.checked; };
  cbL.append(lockIn, ' ', lockOut);
  cbL.hidden = T;
  opts.append(cbL);
  const minLock = () => lockTick.checked ? Number(lockIn.value) : null;
  // earlier conversions of the same source (the converter's legacy-id)
  const prevBox = el('div');
  const actions = el('div', 'cv-actions');
  const go_ = el('button', 'primary cv-big', 'Convert');
  go_.disabled = !cvPick;
  // the ticked cars, one after another (the convert queue)
  const goSel = el('button', 'cv-big');
  goSel.hidden = true;
  const stop = el('button', 'sm', 'Stop');
  stop.hidden = true;
  // a car's Delete: out of the game, its converted files gone, back to "Not
  // converted" (the AC1 original untouched); a track's Remove from the game
  const uninst = el('button', 'sm', T ? 'Remove from the game' : 'Delete');
  uninst.hidden = true;
  // the ticked cars that are in the game, taken out together
  const uninstSel = el('button', 'sm');
  uninstSel.hidden = true;
  // a track converted without installing: install that build, no reconvert
  const instNow = el('button', 'sm', 'Put this build in the game');
  instNow.hidden = true;
  actions.append(go_, goSel, stop, uninst, uninstSel, instNow);
  const adv = el('details', 'cv-adv');
  adv.innerHTML = '<summary>Advanced</summary>';
  const idRow = el('div', 'row wrap cv-idrow');
  const idIn = el('input');
  idIn.spellcheck = false;
  idRow.append(el('span', 'tiny dim', 'EVO ' + noun + ' id'), idIn,
    el('span', 'tiny dim', T ? '3-16 lower-case letters, digits and _. Converting again with the same id replaces the old one.'
      : 'ks_ + lower-case letters, digits and _, 13-37 characters. Converting again with the same id replaces the old one.'));
  adv.append(idRow);
  hside.append(htitle, hsub, qStrip, opts, prevBox, actions, adv);
  hero.append(himg, hside);
  // progress
  const prog = el('div', 'cv-prog');
  prog.hidden = true;
  const pbar = el('div', 'cv-bar', '<div></div>');
  const pline = el('div', 'row');
  const pword = el('span', 'grow');
  const ptime = el('span', 'tiny dim');
  pline.append(pword, ptime);
  const pcheck = el('div');
  const det = el('details', 'cv-adv');
  det.innerHTML = '<summary>Details</summary>';
  const pre = el('pre', 'log');
  det.append(pre);
  prog.append(pbar, pline, pcheck, det);
  // the convert queue: what each car did, the one converting, the ones waiting
  const qBox = el('div', 'cv-q');
  qBox.hidden = true;
  const qHead = el('div', 'row');
  const qStop = el('div');
  const qList = el('ol');
  qBox.append(qHead, qStop, qList);
  let qKey = '';
  // track details: map, facts, layouts, the model files and where they are
  const tdet = el('div', 'cv-tdet');
  tdet.hidden = true;
  let tLayout = '', tSel = null, tAll = [], tSource = '', tBlocked = false, tReq = 0, pReq = 0;
  let lastId = '', delId = '', delName = '';
  let replaceIds = [];
  // the job convert/status last reported (poll), read by pick() too
  let wasRunning = false, runFolder = '';
  cc.append(hero, tdet, prog, qBox);
  sideCol.append(cc);

  // ---- setup (folded unless something is missing)
  const sc = el('details', 'card');
  sc.open = cvSetupOpen;
  sc.ontoggle = () => { cvSetupOpen = sc.open; };
  sc.innerHTML = '<summary><b>Setup</b>'
    + ' <span class="tiny ' + (missing.length ? 'bad' : 'dim') + '">' + (missing.length ? 'needs attention' : 'everything found') + '</span></summary>';
  const fields = [
    ['ac1_root', 'Assetto Corsa folder', 'Where Assetto Corsa (the first game) is installed', have.ac1_root],
    ['extra_dirs', 'Extra mod folders', 'Folders with AC1 mods that are not installed in Assetto Corsa (downloads, unzipped packs). Separate several with ;', true],
    ['out_dir', 'Work folder', 'Where converted ' + (info.tracks ? 'cars and tracks are' : 'cars are') + ' built (several hundred MB each). The converter also keeps what it builds once from your EVO install here (_aceconv).', have.out_dir],
    ['threads', 'Threads', '0 = automatic: half of your cores, fewer when memory is short', true],
    ['memory_gb', 'Memory limit (GB)', '0 = automatic: what is free, less 2 GB for everything else', true],
  ];
  const inputs = {};
  const sg = el('div', 'cv-setup');
  // found / not found: tool states, green and red
  const tick = ok => el('div', ok ? 'yes' : 'no', ok ? '✓' : '✗');
  for (const [k, label, hint, ok] of fields) {
    const i = el('input');
    i.value = st[k] === undefined ? '' : st[k];
    i.spellcheck = false;
    i.title = hint;
    inputs[k] = i;
    sg.append(el('div', 'tiny', esc(label)), i, tick(ok));
  }
  const row3 = (label, text, ok) => {
    sg.append(el('div', 'tiny', esc(label)), el('div', 'tiny dim', text), tick(ok));
  };
  {
    const i = el('input');
    i.value = info.game_dir || '';
    i.placeholder = 'not found - e.g. C:\\Program Files (x86)\\Steam\\steamapps\\common\\Assetto Corsa EVO';
    i.spellcheck = false;
    i.title = 'The Assetto Corsa EVO install folder (the one holding content.kspkg). Same setting as in Settings.';
    inputs.game_dir = i;
    sg.append(el('div', 'tiny', 'EVO game folder'), i, tick(have.game_content));
  }
  row3('Converter', esc(have.aceconv ? ('aceconv' + (info.converter_rev ? ' ' + info.converter_rev : '') + ' · ' + (info.threads_why || '')) : 'missing from this build'), have.aceconv);
  row3('From your game', have.donor ? 'the donor car and templates are built'
    : 'built on the first conversion from your EVO install (a minute or two, once)', true);
  const srow = el('div', 'row wrap');
  const save = el('button', 'sm', 'Save');
  save.onclick = async () => {
    const body = {};
    for (const k in inputs) body[k] = inputs[k].value;
    const r = await api('convert/settings', body);
    if (r.ok) { toast('Saved'); convertPage(); } else toast(r.error || 'could not save', true);
  };
  srow.append(save, el('span', 'tiny dim', 'Cars go to ' + esc(info.client_mods || '?')));
  sc.append(sg, srow);
  libCol.append(sc);

  // ---- the grid
  const all = cars.cars || [];
  const srcOf = (c, small) => '/api/convert/' + (T ? 'track_preview' : 'preview') + '?car=' + encodeURIComponent(c.folder) + (small ? '&thumb=1' : '');
  const imgOf = c => 'url("' + srcOf(c, false) + '")';
  function tagOf(c) {
    if (c.protected) return '<span class="cv-tag bad">Protected</span>';
    if (c.kunos) return '<span class="cv-tag">Kunos car</span>';
    const l = c.last;
    // in the game from the old app (or an older build): a sash across the picture
    const sash = c.old ? '<span class="cv-sash">Update available</span>' : '';
    // "In game" (Quiet Line Blue) only when the mods folder really holds it:
    // cars are checked on disk by the server; tracks go by the install record
    const inGame = T ? !!(l && (l.installed || []).length) : !!(c.in_game || c.old);
    const ig = inGame ? '<span class="cv-tag ig">In game</span>' : '';
    const tags = first => (first || ig) ? '<span class="cv-tags">' + first + ig + '</span>' : '';
    if (!l) return sash + tags('');
    if (!l.ok) return sash + tags('<span class="cv-tag bad">Failed ' + esc(cvAgo(l.at)) + '</span>');
    return sash + tags('<span class="cv-tag ok">Converted</span>');
  }
  function line2(c) {
    if (T) {
      const lay = (c.layouts || []).length > 1 ? (c.layouts.length + ' layouts') : '';
      return [c.country, c.length, c.pitboxes ? c.pitboxes + ' boxes' : '', lay].filter(Boolean).map(esc).join(' · ') || esc(c.folder);
    }
    return [c.year, c.power, c.class].filter(Boolean).map(esc).join(' · ') || esc(c.folder);
  }
  function paint() {
    const q = filt.value.trim().toLowerCase();
    cvFilter = filt.value;
    grid.innerHTML = '';
    // Kunos's own cars (public app only) can't be converted: left out of the
    // list, but shown, marked, when a search finds them, so they don't seem missing
    const kunos = all.filter(c => c.kunos).length;
    const mods = all.length - kunos;
    const shown = all.filter(c => {
      if (cvShow === 'new' && c.last && c.last.ok) return false;
      if (cvShow === 'done' && !(c.last && c.last.ok)) return false;
      if (c.kunos && !q) return false;
      return !q || [c.folder, c.name, c.brand, c.year, c.class].join(' ').toLowerCase().includes(q);
    });
    const total = q ? all.length : mods;
    count.textContent = cars.ok ? (shown.length === total ? total + ' ' + noun + 's' : shown.length + ' of ' + total + ' ' + noun + 's')
      + (kunos && !q ? ' · ' + kunos + ' Kunos cars left out' : '') : (cars.error || '');
    count.title = kunos ? 'Kunos\'s own Assetto Corsa cars belong to Kunos, so they can\'t be converted. Search for one to see it.' : '';
    lastShown = shown;
    for (const c of shown.slice(0, 300)) {
      const blocked = c.protected || c.kunos;
      const card = el('div', 'cv-card' + (c.folder === cvPick ? ' on' : '') + (blocked ? ' locked' : '') + (cvSel.has(c.folder) ? ' sel' : ''));
      card.title = c.protected ? (T ? 'Protected by its author - click to see its files' : 'Protected by its author - it can\'t be converted')
        : c.kunos ? 'One of Kunos\'s own Assetto Corsa cars - it belongs to Kunos, so it can\'t be converted'
        : c.folder + (c.from ? '\nfrom ' + c.from : '')
          + (c.old ? '\nIn the game from an older version (' + c.old.join(', ') + '). Convert it again to update it - the old copy is replaced.' : '');
      card.dataset.folder = c.folder;
      // small cached thumbnails, loaded only when scrolled into view
      const im = el('div', 'cv-img', tagOf(c));
      const pic = document.createElement('img');
      pic.loading = 'lazy';
      pic.decoding = 'async';
      pic.alt = '';
      pic.src = srcOf(c, true);
      pic.onerror = () => pic.remove();
      im.prepend(pic);
      // ticked for the queue (cars only; a click on the box doesn't pick the car)
      if (!T && !blocked) {
        const tick = document.createElement('input');
        tick.type = 'checkbox';
        tick.className = 'cv-sel';
        tick.title = 'Convert this car in the queue';
        tick.checked = cvSel.has(c.folder);
        tick.onclick = e => e.stopPropagation();
        tick.ondblclick = e => e.stopPropagation();
        tick.onchange = () => { if (tick.checked) cvSel.add(c.folder); else cvSel.delete(c.folder); card.classList.toggle('sel', tick.checked); selPaint(); };
        im.append(tick);
      }
      card.append(im, el('div', 'cv-meta', '<b>' + esc(c.name || c.folder) + '</b><span>'
        + (T ? '' : esc(c.brand || '') + (c.brand ? ' · ' : '')) + line2(c) + '</span>'));
      // A click picks the car - but once some are ticked, or a queue is
      // running, a click ticks or unticks it, like the box: the pane is
      // showing those cars then, not one. A protected track can still be
      // looked at (and pointed at another copy).
      card.onclick = () => {
        if (!T && !blocked && (selCars().length || queueActive())) {
          const tick = card.querySelector('.cv-sel');
          tick.checked = !tick.checked;
          tick.onchange();
          return;
        }
        if (T || !blocked) pick(c);
      };
      card.ondblclick = () => { if (!blocked && heroMode === 'one') { pick(c); go_.click(); } };
      grid.append(card);
    }
    if (shown.length > 300) grid.append(el('div', 'tiny dim', 'Showing 300 - search to narrow it down.'));
    if (!shown.length) grid.append(el('div', 'tiny dim', 'No ' + noun + 's match.'));
    selPaint();
  }
  // the queue's selection: only cars still in the list count
  let lastShown = [];
  const selCars = () => all.filter(c => cvSel.has(c.folder) && !c.protected && !c.kunos);
  function selPaint() {
    const n = selCars().length;
    selInfo.textContent = n ? n + ' car' + (n === 1 ? '' : 's') + ' ticked' : 'Tick cars to convert several, one after another.';
    selNone.hidden = !n;
    goSel.hidden = T || !n;
    goSel.textContent = 'Convert ' + n + ' ticked';
    heroPaint();
  }

  /* The pane on the right shows one of three things: the picked car; the
     ticked cars, as a mosaic of their pictures; or, while a queue runs, the
     car it is converting, with every car in the queue in a strip below. The
     queue wins over the ticks (more can be ticked to add to it), the ticks
     over the picked car. */
  let heroMode = 'one', heroKey = '', lastQ = {};
  const queueActive = () => !T && (!!lastQ.current || (lastQ.waiting || []).length > 0);
  const carOf = f => all.find(x => x.folder === f) || { folder: f };
  const nameOf = f => carOf(f).name || f;
  const thumb = f => 'url("' + srcOf(carOf(f), true) + '")';
  // the ticked cars whose last conversion is in the game, with its EVO id
  // the ticked cars with a conversion to delete (converted, or in the game), with its EVO id
  const convertedTicked = () => selCars().filter(c => c.last && (c.last.ok || (c.last.installed || []).length) && (c.last.id || c.id));
  function heroPaint() {
    const mode = queueActive() ? 'queue' : !T && selCars().length ? 'sel' : 'one';
    const was = heroMode;
    heroMode = mode;
    go_.hidden = mode !== 'one';
    adv.hidden = mode !== 'one';
    qStrip.hidden = mode !== 'queue';
    goSel.classList.toggle('primary', mode === 'sel');
    const n = T ? 0 : selCars().length;
    goSel.textContent = mode === 'queue' ? 'Add ' + n + ' ticked to the queue' : 'Convert ' + n + ' ticked';
    // ticked cars that are in the game can be taken out together (not
    // while a queue runs: it may be installing one of them)
    const done = mode === 'sel' ? convertedTicked() : [];
    uninstSel.hidden = !done.length;
    uninstSel.textContent = 'Delete ' + done.length + ' ticked';
    if (mode !== 'one') { uninst.hidden = true; instNow.hidden = true; prevBox.innerHTML = ''; replaceIds = []; }
    if (mode === 'queue') showQueue(lastQ);
    else if (mode === 'sel') showTicked(selCars());
    else if (was !== 'one') {
      heroKey = '';
      const c = all.find(x => x.folder === cvPick && !x.kunos);
      if (c) showOne(c); else showNone();
    }
  }
  function showNone() {
    himg.style.backgroundImage = '';
    himg.innerHTML = noneHtml;
    htitle.textContent = '';
    hsub.textContent = noneSub;
    go_.textContent = 'Convert';
    go_.disabled = true;
  }
  // up to nine small pictures in the big one's place, the last saying how many more
  function showTicked(list) {
    const key = 'sel|' + list.map(c => c.folder).join('|');
    if (key === heroKey) return;
    heroKey = key;
    const n = list.length, cols = n === 1 ? 1 : n <= 4 ? 2 : 3, max = cols * cols;
    const shown = n > max ? list.slice(0, max - 1) : list;
    const box = el('div', 'cv-mosaic');
    box.style.gridTemplateColumns = 'repeat(' + cols + ',1fr)';
    box.style.gridTemplateRows = 'repeat(' + Math.ceil((shown.length + (n > max ? 1 : 0)) / cols) + ',1fr)';
    for (const c of shown) { const t = el('i'); t.style.backgroundImage = thumb(c.folder); t.title = c.name || c.folder; box.append(t); }
    if (n > max) box.append(el('i', 'more', '+' + (n - shown.length)));
    himg.style.backgroundImage = '';
    himg.innerHTML = '';
    himg.append(box);
    htitle.textContent = n + ' car' + (n === 1 ? '' : 's') + ' ticked';
    const names = list.map(c => c.name || c.folder);
    hsub.textContent = (n <= 3 ? names.join(', ') : names.slice(0, 3).join(', ') + ' and ' + (n - 3) + ' more')
      + '. Converted one after another, each checked first.';
  }
  function showQueue(q) {
    const done = q.done || [], wait = q.waiting || [], cur = q.current || runFolder;
    const key = 'queue|' + JSON.stringify([cur, done.map(d => [d.car, d.ok]), wait]);
    if (key === heroKey) return;
    heroKey = key;
    himg.innerHTML = '';
    himg.style.backgroundImage = cur ? imgOf(carOf(cur)) : '';
    htitle.textContent = cur ? nameOf(cur) : 'The queue';
    const total = done.length + wait.length + (cur ? 1 : 0);
    hsub.textContent = 'Car ' + Math.min(total, done.length + 1) + ' of ' + total + ' in the queue';
    qStrip.innerHTML = '';
    const cars = done.map(d => [d.car, d.ok ? 'ok' : 'bad']).concat(cur ? [[cur, 'cur']] : [], wait.map(w => [w, 'wait']));
    const word = { ok: 'converted', bad: 'failed', cur: 'converting', wait: 'waiting' };
    for (const [f, st] of cars.slice(0, 24)) {
      const t = el('i', st);
      t.style.backgroundImage = thumb(f);
      t.title = nameOf(f) + ' - ' + word[st];
      qStrip.append(t);
    }
    if (cars.length > 24) qStrip.append(el('span', 'tiny dim', '+' + (cars.length - 24)));
  }
  selAll.onclick = () => {
    for (const c of lastShown) if (!c.protected && !c.kunos) cvSel.add(c.folder);
    paint();
  };
  selNone.onclick = () => { cvSel.clear(); paint(); };
  /* Earlier conversions of this source under another id: offer to replace
     them (a name-only match is offered unticked - it may be someone else's
     mod of the same car). */
  function showEarlier(list, newId) {
    prevBox.innerHTML = '';
    replaceIds = [];
    const other = (list || []).filter(e => e.id !== newId);
    if (!other.length) return;
    const box = el('div', 'cv-note', '<b>Converted before</b> under another id. Replacing takes the old one out of the game'
      + (T ? ' (its rows and folder)' : ' (kept in the work folder\'s _replaced) and resets its saved cameras') + '.');
    for (const e of other) {
      const lab = el('label', 'tiny');
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = e.strength >= 2;
      const sync = () => { replaceIds = [...prevBox.querySelectorAll('input:checked')].map(x => x.dataset.id); };
      cb.dataset.id = e.id;
      cb.onchange = sync;
      lab.append(cb, document.createTextNode(' Replace ' + e.id + ' - ' + (e.strength >= 2 ? 'same source folder' : 'same name only') + ' (' + e.why + ')'));
      box.append(lab);
    }
    prevBox.append(box);
    replaceIds = other.filter(e => e.strength >= 2).map(e => e.id);
  }
  async function pick(c) {
    cvPick = c.folder;
    cvPicks[cvMode] = c.folder;
    grid.querySelectorAll('.cv-card').forEach(x => x.classList.toggle('on', x.dataset.folder === c.folder));
    // the ticked cars or the queue have the pane: the pick shows when they're done
    if (heroMode === 'one') await showOne(c);
  }
  async function showOne(c) {
    idIn.value = c.id || '';
    himg.innerHTML = '';
    himg.style.backgroundImage = imgOf(c);
    htitle.textContent = c.name || c.folder;
    const l = c.last;
    hsub.innerHTML = esc((T ? [c.country, c.length] : [c.brand, c.year, c.power]).filter(Boolean).join(' · '))
      + (l && l.ok ? '<br>Last converted ' + esc(cvAgo(l.at)) + ' in ' + l.secs + ' s'
        + (l.check ? ' · check: ' + l.check.errors + ' errors, ' + l.check.warnings + ' warnings' : '') : '');
    go_.textContent = c.old && !T ? 'Update' : l && l.ok ? 'Convert again' : 'Convert';
    lastId = c.id || '';
    // what Delete removes: the last conversion's id
    delId = (l && l.id) || c.id || '';
    delName = c.name || c.folder;
    // another car can be looked at while one converts, but nothing is
    // started, installed or removed until that run ends
    go_.disabled = wasRunning;
    go_.title = wasRunning ? 'Wait for ' + runName() + ' to finish, or stop it' : '';
    uninst.hidden = wasRunning || !(l && (T ? (l.installed || []).length : l.ok || (l.installed || []).length));
    instNow.hidden = wasRunning || !(T && l && l.ok && !(l.installed || []).length);
    prevBox.innerHTML = '';
    replaceIds = [];
    if (T) {
      if (c.folder !== tdet.dataset.folder) { tLayout = ''; tSource = ''; tSel = null; }
      tdet.dataset.folder = c.folder;
      trackDetails(c);
    } else {
      const req = ++pReq;
      const e = await api('convert/earlier?kind=cars&folder=' + encodeURIComponent(c.folder));
      if (req === pReq && cvPick === c.folder && e && e.ok) showEarlier(e.earlier, c.id);
    }
  }
  const mb = n => n >= 1048576 ? (n / 1048576).toFixed(0) + ' MB' : (n / 1024).toFixed(0) + ' KB';
  async function trackDetails(c) {
    const req = ++tReq;
    tBlocked = !!c.protected;
    go_.disabled = tBlocked;
    const q = 'convert/track_info?track=' + encodeURIComponent(c.folder) + (tSource ? '&source=' + encodeURIComponent(tSource) : '');
    const d = await api(q);
    if (req !== tReq || cvPick !== c.folder) return;
    tdet.hidden = false;
    tdet.innerHTML = '';
    const left = el('div'), right = el('div');
    if (!d.ok) {
      left.append(el('div', 'cv-banner', esc(d.error || 'Could not read the track')));
      if (tSource) {
        const back = el('button', 'sm', 'Use the installed track instead');
        back.onclick = () => { tSource = ''; trackDetails(c); };
        left.append(back);
      }
      tdet.append(left, right);
      tBlocked = true;
      go_.disabled = true;
      return;
    }
    showEarlier(d.earlier, c.id);
    const lays = d.layouts || [];
    if (!lays.some(x => x.layout === tLayout)) tLayout = (lays[0] || {}).layout || '';
    const cur = lays.find(x => x.layout === tLayout) || lays[0] || { ui: {} };
    const u = cur.ui || {};
    // layout picker: the tick chooses what is converted, the name previews its map and facts
    tAll = lays.map(x => x.layout);
    if (!tSel) tSel = new Set(tAll);
    if (lays.length > 1) {
      const row = el('div', 'cv-lays');
      row.append(el('span', 'tiny dim', 'Layouts to convert (click a name to preview it)'));
      for (const x of lays) {
        const item = el('span', 'cv-lay' + (tSel.has(x.layout) ? '' : ' off'));
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.checked = tSel.has(x.layout);
        cb.title = 'Convert this layout';
        cb.onchange = () => {
          if (cb.checked) tSel.add(x.layout);
          else if (tSel.size > 1) tSel.delete(x.layout);
          else cb.checked = true;
          item.classList.toggle('off', !cb.checked);
        };
        const b = el('button', 'cv-chip' + (x.layout === tLayout ? ' on' : ''), esc((x.ui || {}).name || x.layout));
        b.title = x.layout;
        b.onclick = () => { tLayout = x.layout; trackDetails(c); };
        item.append(cb, b);
        row.append(item);
      }
      left.append(row);
    }
    const imgQ = '?car=' + encodeURIComponent(c.folder) + (tLayout ? '&layout=' + encodeURIComponent(tLayout) : '');
    himg.style.backgroundImage = 'url("/api/convert/track_preview' + imgQ + '")';
    if (u.name) htitle.textContent = u.name;
    const facts = el('div', 'cv-facts');
    const fact = (k, v) => { if (v) facts.append(el('div', '', esc(k)), el('div', '', esc(v))); };
    fact('Author', u.author);
    fact('Where', [u.city, u.country].filter(Boolean).join(', '));
    fact('Length', u.length);
    fact('Width', u.width);
    fact('Pit boxes', u.pitboxes);
    fact('Direction', u.run);
    fact('Version', u.version);
    fact('Tags', u.tags);
    fact('Layouts', lays.length > 1 ? lays.length + '' : '');
    fact('Model files', (d.files || []).length + ' · ' + mb(d.kn5_bytes || 0));
    fact('CSP config', d.csp_config ? 'yes (lights, materials)' : '');
    fact('Skin', d.skin_used ? d.skin_used + ' (fills in placeholder textures)' : '');
    fact('Source', d.dir);
    left.append(facts);
    if (u.description) left.append(el('div', 'cv-desc', esc(String(u.description).replace(/<br\s*\/?>/gi, '\n').replace(/<[^>]+>/g, ''))));
    // protection, in words
    const locked = d.protected_files || 0;
    tBlocked = locked > 0;
    if (locked) {
      left.append(el('div', 'cv-banner', '<b>Protected by its author.</b> ' + locked + ' of ' + d.files.length
        + ' model files are encrypted (CSP), so this copy can\'t be converted. If the author gives you an unencrypted copy, '
        + 'paste its folder under <b>Convert from</b> - or swap the files in the track folder and pick the track again.'));
    }
    // the model files, each one click from Explorer
    const fl = el('div', 'cv-files');
    for (const f of d.files || []) {
      const r = el('div');
      r.append(el('span', 'n', esc(f.name)), el('span', 'tiny dim', mb(f.size)),
        el('span', f.protected ? 'lock' : 'tiny dim', f.protected ? 'encrypted' : 'ok'));
      const o = el('button', 'sm open', 'Show');
      o.title = 'Show this file in Explorer';
      o.onclick = () => api('convert/track_open', { track: c.folder, file: f.name, source: tSource });
      r.append(o);
      fl.append(r);
    }
    left.append(fl);
    // where to convert from
    const src = el('div', 'cv-src');
    const sIn = el('input');
    sIn.placeholder = 'Convert from another copy: paste its folder (empty = the installed track)';
    sIn.value = tSource;
    sIn.spellcheck = false;
    const sUse = el('button', 'sm', 'Use');
    sUse.onclick = () => { tSource = sIn.value.trim().replace(/^"|"$/g, ''); trackDetails(c); };
    sIn.onkeydown = e => { if (e.key === 'Enter') sUse.click(); };
    const sOpen = el('button', 'sm', 'Open folder');
    sOpen.title = 'Open the track folder in Explorer';
    sOpen.onclick = () => api('convert/track_open', { track: c.folder, source: tSource });
    const sRe = el('button', 'sm', 'Re-check');
    sRe.title = 'Read the files again (after you changed them)';
    sRe.onclick = () => trackDetails(c);
    src.append(el('span', 'tiny dim', 'Convert from'), sIn, sUse, sOpen, sRe);
    left.append(src);
    // the track map
    const map = el('div', 'cv-map');
    if (cur.outline) map.style.backgroundImage = 'url("/api/convert/track_preview' + imgQ + '&outline=1")';
    else map.innerHTML = '<div class="tiny dim" style="padding:10px">No track map</div>';
    right.append(map);
    tdet.append(left, right);
    go_.disabled = tBlocked || wasRunning;
    go_.title = tBlocked ? 'This copy is encrypted by its author' : wasRunning ? 'Wait for ' + runName() + ' to finish, or stop it' : '';
  }
  let filtT = null;
  filt.oninput = () => { clearTimeout(filtT); filtT = setTimeout(paint, 150); };
  paint();
  const again = all.find(c => c.folder === cvPick && !c.kunos);
  if (again) pick(again);

  // ---- actions
  go_.onclick = async () => {
    if (!cvPick) { toast('Pick a ' + noun + ' first', true); return; }
    const r = T
      ? await api('convert/track_start', { track: cvPick, id: idIn.value.trim(), install: cbC.querySelector('input').checked,
          source: tSource, replace: replaceIds, layouts: tSel && tSel.size < tAll.length ? [...tSel] : [] })
      : await api('convert/start', { car: cvPick, id: idIn.value.trim(), replace: replaceIds,
          install_client: cbC.querySelector('input').checked, keep_brakes: cbB.querySelector('input').checked, min_lock: minLock() });
    if (r.ok) poll(); else toast(r.error || 'could not start', true);
  };
  stop.onclick = () => api('convert/cancel', {});
  goSel.onclick = async () => {
    const list = selCars();
    if (!list.length) return;
    const install = cbC.querySelector('input').checked;
    // time by each car's last run, else a typical one
    const secs = list.reduce((s, c) => s + ((c.last && c.last.ok && c.last.secs) || 60), 0);
    const took = secs < 5400 ? Math.max(1, Math.round(secs / 60)) + ' min' : (secs / 3600).toFixed(1) + ' hours';
    if (list.length > 1 && !await ask('Convert ' + list.length + ' cars, one after another?\n\nAbout ' + took + '. Each car is checked before '
      + (install ? 'it goes in the game, and once it is in, its copy in the work folder is removed to save space.'
                 : 'it is kept in the work folder (not installed: tick "Install in the game" for that).')
      + ' A car that fails is skipped. Stop ends the queue.', 'Convert ' + list.length)) return;
    const r = await api('convert/queue', { cars: list.map(c => c.folder), install_client: install, keep_brakes: cbB.querySelector('input').checked, min_lock: minLock() });
    if (!r.ok) { toast(r.error || 'could not start', true); return; }
    toast(r.added === list.length ? list.length + ' car' + (list.length === 1 ? '' : 's') + ' queued' : r.added + ' queued (the rest already were)');
    cvSel.clear();
    paint();
    poll();
  };
  instNow.onclick = async () => {
    if (!lastId || !await ask('Put ' + lastId + ' in the game?\n\nThis adds it to the game\'s track list (content.kspkg, backed up first). Assetto Corsa EVO must be closed.', 'Install')) return;
    instNow.disabled = true;
    const r = await api('convert/track_install', { id: lastId, replace: replaceIds });
    instNow.disabled = false;
    if (r.ok) { toast('Installed ' + lastId); convertPage(); } else toast(r.error || 'could not install', true);
  };
  const delWhat = n => n > 1
    ? 'They come out of the game and their converted files are deleted, so they show as not converted again. The original Assetto Corsa cars aren\'t touched.'
    : 'It comes out of the game and its converted files are deleted, so it shows as not converted again. The original Assetto Corsa car isn\'t touched.';
  uninst.onclick = async () => {
    if (T) {
      if (!lastId || !await ask('Take ' + lastId + ' out of the game?\n\nThe converted files in the work folder are kept.', 'Remove')) return;
      const r = await api('convert/track_uninstall', { id: lastId });
      if (r.ok) { toast('Removed'); convertPage(); } else toast(r.error || 'could not remove', true);
      return;
    }
    if (!delId || !await ask('Delete ' + delName + '?\n\n' + delWhat(1), 'Delete')) return;
    const r = await api('convert/uninstall', { id: delId, forget: true });
    if (r.ok) { toast('Deleted ' + delName); convertPage(); } else toast(r.error || 'could not delete', true);
  };
  uninstSel.onclick = async () => {
    const list = convertedTicked();
    if (!list.length) return;
    const names = list.map(c => c.name || c.folder);
    if (!await ask('Delete ' + (list.length === 1 ? names[0] : list.length + ' cars') + '?\n\n'
      + (list.length > 1 ? names.join(', ') + '\n\n' : '') + delWhat(list.length), 'Delete')) return;
    uninstSel.disabled = true;
    const failed = [];
    for (const c of list) {
      const r = await api('convert/uninstall', { id: c.last.id || c.id, forget: true });
      if (r.ok) cvSel.delete(c.folder); else failed.push((c.name || c.folder) + ': ' + (r.error || 'could not delete'));
    }
    uninstSel.disabled = false;
    if (failed.length) toast(failed.join(' · '), true);
    else toast('Deleted ' + list.length + ' car' + (list.length === 1 ? '' : 's'));
    convertPage();
  };

  // no stage output from the converter: progress is by time, against the
  // last run of the source being converted (or a typical one) - not the
  // picked one, which can change mid-run
  const est = () => { const c = all.find(x => x.folder === runFolder); return (c && c.last && c.last.ok && c.last.secs) || (T ? 600 : 90); };
  function runName() { const c = all.find(x => x.folder === runFolder); return (c && c.name) || runFolder || 'the ' + noun; }
  /* The queue's list: done (with what happened), converting, waiting. Only
     repainted when it changes, so a scrolled list stays put. */
  function paintQueue(q) {
    const done = q.done || [], wait = q.waiting || [], cur = q.current;
    const key = JSON.stringify([done.length, cur, wait, q.stopped, q.free_gb]);
    if (key === qKey) return;
    qKey = key;
    qBox.hidden = T || (!done.length && !wait.length && !cur && !q.stopped);
    if (qBox.hidden) return;
    const nm = f => { const c = all.find(x => x.folder === f); return (c && c.name) || f; };
    const ok = done.filter(d => d.ok).length, bad = done.length - ok, total = done.length + wait.length + (cur ? 1 : 0);
    qHead.innerHTML = '';
    qHead.append(el('span', 'grow', '<b>Queue</b> · ' + done.length + ' of ' + total + ' done'
      + (ok ? ' · <span class="ok">' + ok + ' converted</span>' : '') + (bad ? ' · <span class="bad">' + bad + ' failed</span>' : '')
      + (wait.length ? ' · ' + wait.length + ' waiting' : '')
      + (q.free_gb != null ? ' · <span class="dim">' + q.free_gb + ' GB free for the work folder</span>' : '')));
    if (!cur && !wait.length) {
      const clr = el('button', 'sm', 'Clear the list');
      clr.onclick = async () => { await api('convert/queue_clear', {}); poll(); };
      qHead.append(clr);
    }
    qStop.innerHTML = q.stopped ? '<span class="bad">' + esc(q.stopped) + '</span>' : '';
    qList.innerHTML = '';
    for (const d of done) {
      const what = d.ok ? (d.installed ? 'in the game' : 'converted, not installed') + (d.warnings ? ' · ' + d.warnings + ' warning' + (d.warnings === 1 ? '' : 's') : '')
        : (d.error || 'failed') + (d.errors ? ' (' + d.errors + ' check error' + (d.errors === 1 ? '' : 's') + ')' : '');
      qList.append(el('li', d.ok ? '' : 'bad', (d.ok ? '<span class="ok">✓</span> ' : '✗ ') + esc(nm(d.car)) + ' <span class="dim">- ' + esc(what) + '</span>'));
    }
    if (cur) qList.append(el('li', '', '<b>' + esc(nm(cur)) + '</b> <span class="dim">- converting…</span>'));
    for (const w of wait) qList.append(el('li', 'dim', esc(nm(w))));
  }
  async function poll() {
    if (!document.getElementById('cv-root')) { clearInterval(cvTimer); cvTimer = null; return; }
    const s = await api('convert/status');
    if (!s.ok) return;
    const running = s.state === 'running';
    const lines = s.lines || [];
    const q = s.queue || {};
    // between two queued cars the job is briefly not running, the next one
    // already taken off the waiting list (current): keep polling
    const queued = (q.waiting || []).length > 0 || !!q.current;
    paintQueue(q);
    runFolder = String(s.car || '').replace(/^track:/, '');
    lastQ = q;
    heroPaint();
    go_.disabled = running || !cvPick || (T && tBlocked);
    stop.hidden = !running;
    stop.textContent = running && queued ? 'Stop the queue' : 'Stop';
    const atBottom = pre.scrollTop + pre.clientHeight >= pre.scrollHeight - 40;
    pre.textContent = lines.join(String.fromCharCode(10));
    if (atBottom) pre.scrollTop = pre.scrollHeight;
    const res = s.result || {};
    // the last job belongs to the other tab (a track while showing cars):
    // its result is not this page's
    const mine = String(s.car || '').startsWith('track:') === T;
    prog.hidden = s.state === 'idle' || (!running && !mine);
    if (!running && !mine) {
      wasRunning = running;
      if (queued && !cvTimer) cvTimer = setInterval(poll, 800);
      return;
    }
    const fill = pbar.firstChild;
    pcheck.innerHTML = '';
    if (running) {
      const prep = lines.some(l => l.includes('building the donor car')) && !lines.some(l => l.startsWith('note:'));
      const snd = lines.some(l => l.includes('building the FMOD template')) && !lines.some(l => l.startsWith('note:'));
      const checking = lines.some(l => l.startsWith('LSP Converter: checking'));
      fill.style.width = (checking ? 98 : Math.min(95, s.secs / est() * 100)).toFixed(0) + '%';
      // a yellow bar is a fill, so it stays yellow on Ivory too (app.css)
      fill.style.background = 'var(--signal, var(--accent))';
      pword.innerHTML = esc(checking ? 'Checking the result' : snd ? 'Preparing the sound template from your game (first time only)'
        : prep ? 'Preparing the donor car from your game (first time only)' : 'Converting ' + runName()) + '…'
        + (heroMode === 'one' && runFolder && runFolder !== cvPick ? ' <span class="tiny dim">(you\'re looking at another ' + noun + ')</span>' : '');
      ptime.textContent = s.secs.toFixed(0) + ' s';
      uninst.hidden = true;
      instNow.hidden = true;
    } else if (s.state === 'done') {
      fill.style.width = '100%';
      const inst = [...new Set((res.installed || []).map(x => x.to || 'game'))];
      const nm = (all.find(x => x.folder === String(s.car).replace(/^track:/, '')) || {}).name || s.car;
      const warn = (res.warnings || []).slice();
      const sizeTxt = T ? (((res.installed || [])[0] || {}).files || 0) + ' files' : ((res.size || 0) / 1048576).toFixed(0) + ' MB';
      const rep = (res.replaced || []).length ? ' Replaced ' + res.replaced.length + ' earlier file' + (res.replaced.length === 1 ? '' : 's') + '.' : '';
      const cams = res.settings_reset && (res.settings_reset.removed || []).length ? ' Saved cameras / mirrors of the old version reset.' : '';
      pword.innerHTML = '<b style="color:var(--accent)">Ready.</b> ' + esc(nm) + ' took ' + res.secs + ' s (' + sizeTxt + '). '
        + (inst.length ? 'It\'s in the ' + inst.join(' and ') + ' - start (or restart) the game and look for it in the ' + noun + ' list.'
                       : 'Package created and checked; not installed. Select the install option before converting to install it automatically.')
        + esc(rep + cams)
        + (warn.length ? '<br><span style="color:var(--red)">' + esc(warn.join(' · ')) + '</span>' : '');
      pcheck.append(cvCheckBox(res.check));
      ptime.textContent = '';
      lastId = s.id || lastId;
      uninst.hidden = !inst.length;
    } else if (s.state === 'error' || s.state === 'cancelled') {
      fill.style.width = '100%';
      fill.style.background = 'var(--red)';
      const why = lines.filter(l => l.startsWith('! ')).slice(-2).map(l => l.slice(2)).join(' ');
      pword.innerHTML = s.state === 'cancelled' ? 'Stopped.' : '<b style="color:var(--red)">That didn\'t work.</b> '
        + esc(res.error || why || '') + ' <span class="tiny dim">Open Details for the full output.</span>';
      if (res.check) pcheck.append(cvCheckBox(res.check));
      ptime.textContent = '';
    }
    if (wasRunning && !running) {
      const c2 = await api(T ? 'convert/tracks' : 'convert/cars');
      if (c2.ok) { all.length = 0; all.push(...(T ? c2.tracks : c2.cars)); paint(); const c = all.find(x => x.folder === cvPick); if (c) pick(c); }
    }
    wasRunning = running;
    const busy = running || queued;
    if (busy && !cvTimer) cvTimer = setInterval(poll, 800);
    if (!busy && cvTimer) { clearInterval(cvTimer); cvTimer = null; }
  }
  poll();
}
