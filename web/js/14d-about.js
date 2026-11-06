/* LSP Converter UI - about. One of the ordered classic scripts listed in
   index.html; they share one global scope (see 01-core.js). */
/* --------------------------------------------------------------- about --- */
// Version, licence, credits and the third-party notices. MIT, BSD and Apache
// need their notices to travel with the app (LEGAL-REVIEW.md); the files are
// served from the exe's own bundle as /legal/... (native/src/web.rs).
const ABOUT_DOCS = [
  ['TERMS.md', 'Terms of use', 'What you agree to when you use LSP Converter'],
  ['THIRD-PARTY-NOTICES.txt', 'Third-party notices', 'Every open-source library inside the app, with its licence'],
  ['LICENSE.txt', 'Licence', 'GNU General Public License, version 3'],
  ['CREDITS.md', 'Credits', 'ACECM, its authors, and what came from where'],
];
let aboutDoc = '';

async function aboutPage() {
  const p = $('#page');
  p.innerHTML = '';
  const v = await api('version');
  // the cover: the carbon crest (yellow gold, since the page has yellow on it)
  // and the two-line headline
  const hero = el('div', 'card about-hero');
  hero.innerHTML = '<img src="/img/crest.png" alt="La Shush Performance crest">'
    + '<div><span class="eyebrow">La Shush Performance</span>'
    + '<h2 class="display">Your car.<span class="l2">In EVO.</span></h2>'
    + '<div class="row wrap" style="gap:6px 16px">'
    + '<span class="mono">LSP Converter ' + esc(v.version ? 'v' + v.version : '') + '</span>'
    + '</div></div>';
  p.append(hero);
  const card = el('div', 'card');
  card.innerHTML = '<h2>What it is</h2>'
    + '<p>Turns Assetto Corsa car mods into Assetto Corsa EVO mods, on your own PC, from your own installs of both games. '
    + 'Made by La Shush Performance, and free.</p>'
    + '<p>It\'s a modified version of ACECM (Assetto Corsa EVO Content Manager) by WhoaThatCombo and the ACECM contributors. '
    + 'ACECM\'s converter is where it started.</p>'
    + '<p>Since then La Shush Performance has reworked how it converts: written conversion rules that apply to every car, '
    + 'a long list of handling and sound behaviours corrected, and new behaviours of our own, '
    + 'each one checked by comparing MoTeC telemetry and by driving the cars on track.</p>'
    + '<p>Copyright &copy; 2026 La Shush Performance. Based on ACECM, copyright &copy; WhoaThatCombo and the ACECM contributors.</p>'
    + '<p>LSP Converter is free software: you can pass it on and change it under the GNU General Public License, '
    + 'version 3 or later. It comes with no warranty. The libraries inside it keep their own licences, listed below.</p>'
    + '<p class="tiny dim">The La Shush Performance name, the LSP badge, crest and monogram are La Shush Performance\'s trademarks, '
    + 'not covered by the GPL. LSP Converter isn\'t affiliated with Kunos Simulazioni or 505 Games; Assetto Corsa and '
    + 'Assetto Corsa EVO are their trademarks.</p>';
  // the corresponding source of this exact version (GPL-3.0, section 6)
  if (v.source) {
    const srow = el('div', 'row wrap');
    srow.style.gap = '10px';
    const surl = extLink(v.source);
    surl.classList.add('mono');
    const scopy = el('button', 'sm', 'Copy link');
    scopy.onclick = () => {
      navigator.clipboard.writeText(v.source).then(() => { scopy.textContent = 'Copied'; }, () => { scopy.textContent = 'Select it and copy'; });
    };
    srow.append(surl, scopy);
    card.append(el('p', '', 'The source code of this version, with its build scripts:'), srow);
  }
  p.append(card);

  // what it will and won't convert (LEGAL-REVIEW.md: the public app's limits)
  const rules = el('div', 'card');
  rules.innerHTML = '<h2>What it converts</h2>'
    + '<p>Community car mods for Assetto Corsa that you already have. Only convert mods you have the right to use, '
    + 'and keep what you convert for yourself: the model, textures and sound still belong to the mod\'s author, '
    + 'and parts of every converted car come from your own copy of Assetto Corsa EVO.</p>'
    + '<p>It won\'t convert:</p>'
    + '<ul class="dev-list">'
    + '<li><b>Tracks.</b> Cars only.</li>'
    + '<li><b>Mods protected with CSP encryption.</b> Ask the author for an unprotected version.</li>'
    + '<li><b>Kunos\'s own Assetto Corsa cars</b>, from the base game or the DLC.</li>'
    + '</ul>'
    + '<p class="tiny dim">It never changes the game\'s own files and never touches online play. '
    + 'Converted cars go into your EVO mods folder and stay on your PC.</p>';
  p.append(rules);

  // the testing behind it (13-stats.js, brought up to date before a build)
  if (typeof LSP_STATS !== 'undefined' && LSP_STATS.length) {
    const work = el('div', 'card');
    work.innerHTML = '<h2>The work behind it</h2>'
      + '<p>Every rule in the converter is tested on real cars in EVO, logged with EVO\'s own MoTeC telemetry '
      + 'and compared with Kunos\'s stock cars, until the numbers and the feel line up.</p>';
    const stats = el('div', 'stats');
    stats.style.margin = '10px 0 8px';
    for (const [n, label] of LSP_STATS) stats.append(el('div', 'stat', '<b>' + esc(n) + '</b><span>' + esc(label) + '</span>'));
    work.append(stats, el('div', 'tiny dim', 'As of ' + esc(LSP_STATS_DATE) + '.'));
    p.append(work);
  }

  const contact = el('div', 'card');
  const mail = 'contact@lashushperformance.com';
  contact.innerHTML = '<h2>Contact</h2>'
    + '<p>Found a bug, or want a feature? Tell us on our Discord or by email, with the car and what happened. '
    + 'The Diagnostics page has the log we\'ll need.</p>';
  const disc = el('p', '', 'Our Discord: ');
  disc.style.margin = '0 0 12px';
  const dl = extLink(LSP_DISCORD);
  dl.classList.add('mono');
  disc.append(dl);
  contact.append(disc);
  const crow = el('div', 'row wrap');
  crow.style.gap = '10px';
  const addr = extLink('mailto:' + mail);
  addr.classList.add('mono');
  const copy = el('button', 'sm', 'Copy address');
  copy.onclick = () => {
    navigator.clipboard.writeText(mail).then(() => { copy.textContent = 'Copied'; }, () => { copy.textContent = 'Select it and copy'; });
  };
  crow.append(addr, copy);
  const site = el('p', '', 'Our website: ');
  site.style.marginTop = '12px';
  site.append(extLink(LSP_SITE));
  contact.append(crow, site);
  p.append(contact);

  const docs = el('div', 'card');
  docs.innerHTML = '<h2>Terms, licences and credits</h2>';
  const row = el('div', 'row wrap');
  row.style.margin = '6px 0 10px';
  const info = el('div', 'tiny dim');
  const pre = el('pre', 'log');
  pre.style.maxHeight = '62vh';
  pre.style.whiteSpace = 'pre-wrap';
  pre.hidden = true;
  const btns = {};
  async function show(name) {
    aboutDoc = name;
    for (const k in btns) btns[k].classList.toggle('primary', k === name);
    const [, , what] = ABOUT_DOCS.find(d => d[0] === name);
    info.textContent = what;
    pre.hidden = false;
    pre.textContent = 'Loading…';
    try {
      const r = await fetch('/legal/' + name, { cache: 'no-store' });
      if (!r.ok) throw new Error(String(r.status));
      // the web and email addresses in them open (01-core.js linkify)
      const text = await r.text();
      pre.textContent = '';
      pre.append(linkify(text));
      pre.scrollTop = 0;
    } catch (e) {
      pre.textContent = name + ' is missing from this copy of the app. It\'s in the source code, next to README.md.';
    }
  }
  for (const [name, label] of ABOUT_DOCS) {
    const b = el('button', 'sm', esc(label));
    b.onclick = () => show(name);
    btns[name] = b;
    row.append(b);
  }
  docs.append(row, info, pre);
  p.append(docs);
  if (aboutDoc) show(aboutDoc);
}
