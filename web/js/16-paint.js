/* ---------------------------------------------------------- the door ---
   The garage the brand's renders are set in (garage-mini-reference.png on
   D:\brand-assets), done as a street-car shop door: black steel, scratched
   and grimy, sprayed with paint in the brand's colours, sticker-bombed with
   the crew's stickers, a gold coachline through the crest. It rolls up
   when you click it.
   - The door is in index.html, so it covers the app from the first paint;
     this decorates it, then opens and closes it. Double-click the LSP mark
     in the sidebar to bring it down again.
   - The paint is fixed artwork: it does not move or fade. The splats and
     scratches are drawn fresh at each start, so no two doors are the same.
   - Reduced motion: the door fades instead of rolling. */

function openDoor() {
  const d = $('#door');
  if (!d || d.classList.contains('up')) return;
  d.classList.add('up');
  d.setAttribute('aria-hidden', 'true');
  const gone = () => { d.hidden = true; d.classList.remove('up'); };
  d.addEventListener('animationend', gone, { once: true });
  // ⚠ and a timer, in case the animation never runs (a hidden window)
  setTimeout(gone, 1600);
  termsCheck();
}

/* The terms (TERMS.md), once per terms version: agreed in config.json's
   terms_agreed ("terms 1", from TERMS.md's "Version 1, ..." line), so the
   box comes back only when that line changes, not on an app update. It
   can't be closed without agreeing. */
let termsOpen = false;

/* TERMS.md as paragraphs that wrap to the box: a line underlined with ===
   or --- is a heading, lines starting "a)" stay a list, the rest of a block
   is one paragraph. */
function termsHtml(text) {
  return text.replace(/\r/g, '').split(/\n\s*\n/).map(b => {
    const ls = b.split('\n').filter(l => l.trim());
    if (!ls.length) return '';
    if (ls.length === 2 && /^[=-]{3,}$/.test(ls[1].trim())) return (ls[1][0] === '=' ? '<h3>' : '<h4>') + esc(ls[0]) + (ls[1][0] === '=' ? '</h3>' : '</h4>');
    if (/^[a-z]\)/.test(ls[0])) {
      const items = [];
      for (const l of ls) if (/^[a-z]\)/.test(l)) items.push(l); else items[items.length - 1] += ' ' + l.trim();
      return '<ul>' + items.map(i => '<li>' + esc(i) + '</li>').join('') + '</ul>';
    }
    return '<p>' + esc(ls.map(l => l.trim()).join(' ')) + '</p>';
  }).join('');
}
async function termsCheck() {
  if (termsOpen) return;
  // can't tell (no terms or no answer): ask
  const termsVer = async () => {
    try {
      const r = await fetch('/legal/TERMS.md', { cache: 'no-store' });
      const m = r.ok && /^Version\s+(\S+?),/m.exec(await r.text());
      return m ? 'terms ' + m[1] : '';
    } catch (e) { return ''; }
  };
  const [v, cfg] = await Promise.all([termsVer(), api('config')]);
  if (v && cfg && cfg.terms_agreed === v) return;
  termsOpen = true;
  const veil = el('div', 'pk-veil terms-veil');
  const box = el('div', 'pk-box terms-box');
  box.setAttribute('role', 'dialog');
  box.setAttribute('aria-modal', 'true');
  box.setAttribute('aria-labelledby', 'terms-h');
  box.innerHTML = '<span class="eyebrow">Before you start</span>'
    + '<h2 id="terms-h">Terms of use</h2>'
    + '<p class="terms-quote">I confirm I own or have permission to use and modify the cars I convert. '
    + 'LSP Converter doesn\'t supply any car content and isn\'t responsible for what I convert.</p>';
  const read = el('button', 'linkish', 'Read the full terms');
  const full = el('div', 'terms-full');
  full.hidden = true;
  read.onclick = async () => {
    if (!full.hidden) { full.hidden = true; read.textContent = 'Read the full terms'; return; }
    full.hidden = false;
    read.textContent = 'Hide the full terms';
    full.textContent = 'Loading…';
    try {
      const r = await fetch('/legal/TERMS.md', { cache: 'no-store' });
      if (!r.ok) throw new Error(String(r.status));
      full.innerHTML = termsHtml(await r.text());
    } catch (e) {
      full.textContent = 'The terms are missing from this copy of the app (TERMS.md, next to README.md in the source code).';
    }
  };
  const lab = el('label', 'terms-tick');
  const tick = document.createElement('input');
  tick.type = 'checkbox';
  lab.append(tick, document.createTextNode(' I agree'));
  const go = el('button', 'sm primary', 'Continue');
  go.disabled = true;
  tick.onchange = () => { go.disabled = !tick.checked; };
  go.onclick = async () => {
    go.disabled = true;
    // api() never throws: the saved settings come back, or {error}
    const r = v ? await api('config', { terms_agreed: v }) : null;
    if (!r || r.terms_agreed !== v) toast('Couldn\'t save that you agreed - it will ask again next time', true);
    termsOpen = false;
    veil.remove();
  };
  const row = el('div', 'row terms-row');
  row.append(lab, go);
  box.append(read, full, row);
  veil.append(box);
  document.body.append(veil);
  tick.focus();
}
function closeDoor() {
  const d = $('#door');
  if (!d || !d.hidden) return;
  d.hidden = false;
  d.removeAttribute('aria-hidden');
  d.classList.add('down');
  d.addEventListener('animationend', () => d.classList.remove('down'), { once: true });
  d.focus();
}

const rnd = (a, b) => a + Math.random() * (b - a);

/* One splat: a blob of overlapping drops, droplets flung out around it and
   drips that ran down. SVG units: the blob is 100 across; the drips may run
   past the bottom (overflow is visible). */
function splatSvg(dripMax) {
  let core = '', fling = '', drips = '';
  for (let i = 0; i < 7; i++)
    core += `<circle cx="${rnd(-11, 11).toFixed(1)}" cy="${rnd(-9, 9).toFixed(1)}" r="${rnd(7, 15).toFixed(1)}"/>`;
  for (let i = 0; i < 22; i++) {
    const a = rnd(0, Math.PI * 2), d = rnd(18, 50);
    fling += `<circle cx="${(Math.cos(a) * d).toFixed(1)}" cy="${(Math.sin(a) * d).toFixed(1)}" r="${(rnd(.5, 4.4) * (1.3 - d / 60)).toFixed(1)}"/>`;
  }
  // ⚠ the blob turns, the drips do not: paint runs down
  for (let i = 0, n = 1 + (Math.random() * 4 | 0); i < n; i++) {
    const x = rnd(-13, 13), len = rnd(8, dripMax || 30), w = rnd(2.2, 3.6);
    drips += `<rect x="${(x - w / 2).toFixed(1)}" y="4" width="${w.toFixed(1)}" height="${(6 + len).toFixed(1)}" rx="${(w / 2).toFixed(1)}"/>`
      + `<circle cx="${x.toFixed(1)}" cy="${(10 + len).toFixed(1)}" r="${(w * .8).toFixed(1)}"/>`;
  }
  return `<svg viewBox="-50 -50 100 100" aria-hidden="true"><g fill="currentColor">`
    + `<g transform="rotate(${rnd(0, 360).toFixed(0)})">${core}${fling}</g>${drips}</g></svg>`;
}

// Scratches: short keyed marks, mostly across the slats, and a few long scuffs
function scratchSvg() {
  let s = '';
  const line = (n, lo, hi, op, wd) => {
    for (let i = 0; i < n; i++) {
      const x = rnd(0, 1600), y = rnd(0, 1000), len = rnd(lo, hi), a = rnd(-.35, .35) + (Math.random() < .2 ? rnd(-1.2, 1.2) : 0);
      s += `<path d="M${x.toFixed(0)} ${y.toFixed(0)}l${(Math.cos(a) * len).toFixed(0)} ${(Math.sin(a) * len).toFixed(0)}" `
        + `stroke-opacity="${rnd(op[0], op[1]).toFixed(2)}" stroke-width="${rnd(wd[0], wd[1]).toFixed(1)}"/>`;
    }
  };
  line(90, 8, 70, [.06, .2], [.5, 1.2]);
  line(6, 180, 520, [.04, .09], [.6, 1.6]);
  return `<svg viewBox="0 0 1600 1000" preserveAspectRatio="xMidYMid slice" aria-hidden="true">`
    + `<g stroke="#EEE9DF" stroke-linecap="round" fill="none">${s}</g></svg>`;
}

/* The paint: splats around the edges of the door, kept off the crest and
   the line in the middle, and off the stickers (the paint went on first).
   Mostly Signal Yellow, some LSP Gold, a little Bone, and the guideline's
   dark charcoal paint (#1D1C1B, its cover splatter's other colour): tonal on
   the black steel, there in the light. The brand's colours, nothing else -
   never Quiet Line Blue, which is only ever The Quiet Line's. */
function sprayDoor(art) {
  const inks = ['var(--signal)', 'var(--signal)', 'var(--signal)', 'var(--lsp-gold)', 'var(--lsp-gold)', '#EEE9DF',
    '#1D1C1B', '#1D1C1B'];
  const W = art.clientWidth, H = art.clientHeight;
  if (!W || !H) return;
  // ⚠ a sticker may not have loaded yet: its height is about its width
  const keepOff = [...art.querySelectorAll('.door-sticker,.door-tag')]
    .map(s => s.getBoundingClientRect())
    .filter(r => r.width)
    .map(r => ({ l: r.left - 12, t: r.top - 12, r: r.right + 12, b: r.top + Math.max(r.height, r.width * 1.1) + 12 }));
  const spots = [];
  for (let tries = 0; spots.length < 32 && tries < 800; tries++) {
    const size = Math.random() < .25 ? rnd(150, 240) : rnd(50, 130);
    const x = rnd(-.02, 1.02) * W, y = rnd(.04, .92) * H, px = x / W * 100, py = y / H * 100;
    // the middle stays clear: the crest and the headline, and the coachline
    // above and below them
    if (px > 24 && px < 76 && py > 12 && py < 82) continue;
    if (Math.abs(x - W / 2) < size * .42 + 24) continue;
    // the blob and its flung drops, and the drips running down under it
    const box = { l: x - size * .42, t: y - size * .42, r: x + size * .42, b: y + size * (py < 60 ? 1 : .7) };
    if (keepOff.some(k => box.l < k.r && box.r > k.l && box.t < k.b && box.b > k.t)) continue;
    if (spots.some(p => Math.hypot(p.x - x, (p.y - y) * .7) < Math.max(size, p.size) * .45)) continue;
    spots.push({ x, y, size, py });
  }
  for (const { x, y, size, py } of spots) {
    const s = el('div', 'door-splat door-worn', splatSvg(py < 60 ? 60 : 30));
    s.style.cssText = `left:calc(${(x / W * 100).toFixed(2)}% - ${size / 2}px);top:calc(${(y / H * 100).toFixed(2)}% - ${size / 2}px);`
      + `width:${size.toFixed(0)}px;height:${size.toFixed(0)}px;color:${inks[Math.random() * inks.length | 0]}`;
    art.append(s);
  }
}

{
  const d = $('#door');
  const art = d && d.querySelector('.door-art');
  if (art) {
    art.insertAdjacentHTML('afterbegin', '<div class="door-scratch">' + scratchSvg() + '</div>');
    sprayDoor(art);
  }
  if (d) {
    d.onclick = openDoor;
    d.onkeydown = e => {
      if (e.key !== 'Enter' && e.key !== ' ' && e.key !== 'Escape') return;
      e.preventDefault();
      openDoor();
    };
    d.focus();
  }
  const b = $('#brand');
  if (b) b.ondblclick = closeDoor;
}
