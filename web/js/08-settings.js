/* LSP Converter UI - settings. One of the ordered classic scripts listed in
   index.html; they share one global scope (see 01-core.js). */
/* ------------------------------------------------------------- settings -- */
/* Only what the converter uses. ACECM's settings (disk compression, game
   launch, HUD and pit tweaks, the installer, the lobby proxy, server paths
   and ports) are gone: none of that is part of our app (LEGAL-REVIEW.md).
   The converter's own options - AC1 folder, work folder, threads - live on
   the Convert page. */
const SETTINGS_FOLDERS = [
  // [setting, detected as, label, what it is for]
  ['game_dir', 'game_dir', 'Assetto Corsa EVO install',
   'The converter reads the game\'s own car files from here to build on.'],
  ['client_mods_dir', 'client_mods', 'EVO mods folder',
   'Converted cars and tracks are installed here, where the game finds them.'],
];
const DETECTED_LABEL = {
  game_dir: 'Assetto Corsa EVO install',
  ace_dir: 'EVO\'s Saved Games folder',
  client_mods: 'EVO mods folder',
};

async function settingsPage() {
  const cfg = await api('config');
  const p = $('#page');
  p.innerHTML = '';

  // ---- folders ------------------------------------------------------------
  const c = el('div', 'card');
  c.innerHTML = '<h2>Folders</h2>'
    + '<div class="tiny dim" style="margin-bottom:10px">Found automatically '
    + 'from Steam and Windows. Only fill one in if it was found wrong; leave '
    + 'it blank to use what\'s detected. The AC1 folder and the converter\'s '
    + 'work folder are on the Convert page.</div>';
  const g = el('div', 'grid g2');
  const draft = {};
  const inputs = {};
  SETTINGS_FOLDERS.forEach(([key, , label, why]) => {
    const l = el('label', 'f', `<span>${esc(label)}</span>`);
    const i = el('input');
    i.value = cfg[key] || '';
    i.oninput = () => { draft[key] = i.value.trim(); };
    inputs[key] = i;
    l.append(i, el('div', 'tiny dim', esc(why)));
    g.append(l);
  });
  c.append(g);
  const save = el('button', 'primary', 'Save');
  save.onclick = async () => {
    if (!Object.keys(draft).length) { toast('Nothing changed'); return; }
    const r = await api('config', draft);
    if (r && r.error) { toast(r.error, true); return; }
    toast('Saved');
    showPaths(true);
  };
  c.append(save);
  p.append(c);

  // ---- detected locations -----------------------------------------------
  const dc = el('div', 'card');
  dc.innerHTML = '<h2>Detected locations</h2>'
    + '<div class="tiny dim">Found from Steam&rsquo;s own library list and '
    + 'Windows known folders, so extra drives and a moved '
    + '&ldquo;Saved Games&rdquo; are handled. Anything you type above wins.</div>';
  const drow = el('div', 'row wrap');
  drow.style.margin = '10px 0';
  const redo = el('button', 'sm primary', 'Look again');
  const dnote = el('span', 'tiny dim');
  drow.append(redo, dnote);
  const dlist = el('div');
  dc.append(drow, dlist);
  p.append(dc);

  async function showPaths(refresh) {
    dnote.textContent = refresh ? 'looking…' : '';
    const r = await api('detect' + (refresh ? '?refresh=1' : ''));
    dnote.textContent = `${r.took_ms} ms · Steam: ${r.steam || 'not found'}`;
    dlist.innerHTML = '';
    Object.entries(r.paths || {}).forEach(([k, v]) => {
      dlist.append(el('div', 'chk',
        `<span class="name">${esc(DETECTED_LABEL[k] || k)}`
        + `<div class="tiny dim">${esc(v.path || 'not found')}</div></span>`
        + `<span class="pill ${v.exists ? 'on' : (v.pending ? '' : 'off')}">`
        + `<i class="dot"></i>${v.exists ? v.source
             : (v.pending ? 'made on first use' : 'missing')}</span>`));
    });
    // show what's detected in the empty boxes, so blank reads as "this one"
    SETTINGS_FOLDERS.forEach(([key, det]) => {
      const d = (r.paths || {})[det];
      inputs[key].placeholder = d && d.path ? d.path : 'not found - type the folder';
    });
  }
  redo.onclick = () => showPaths(true);
  showPaths(false);
}
