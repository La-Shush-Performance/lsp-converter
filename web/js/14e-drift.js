/* LSP Converter UI - drift. One of the ordered classic scripts listed in
   index.html; they share one global scope (see 01-core.js). */
/* --------------------------------------------------------------- drift --- */
// A small top-down drift game for while a conversion runs: three circuits,
// four La Shush cars on a two-axle model with saturating tyres, a chase
// camera and a minimap, best lap and best drift per car and circuit. Drawn on
// one canvas in the brand's colours, read from the CSS tokens (app.css), so
// Black (the circuit at night) and Ivory (by day) both work.
// ⚠ It only runs while its page is open: go() calls driftStop(), and the
// loop also stops itself if its canvas leaves the page. Paused = no frames.
// The best times live in localStorage, which an update wipes: a toy's
// scores aren't worth a config key.

// An oval with a figure 8 in it: the oval's round ends, radius R and 2D
// apart, its two straights, and two more straights across the middle from
// one end to the other, crossing over a hump (x, y, radius, height). A lap
// is the figure 8, the line 100 m before the crossing; the oval's straights
// are open to drive (extra), but a lap that leaves the 8 doesn't count.
// The pits and the grandstand face each other across the oval's top
// straight (stands: x, y, heading).
function driftEight(D, R) {
  const th = Math.asin(R / D), L = Math.sqrt(D * D - R * R), c = Math.cos(th), s = Math.sin(th);
  const pts = [], go = 100, step = 12;
  const line = (x, y, dx, dy, len) => { for (let t = 0; t < len; t += step) pts.push([x + dx * t, y + dy * t]); };
  const arc = (cx, a0, sweep) => {
    const n = Math.ceil(Math.abs(sweep) * R / step);
    for (let k = 0; k < n; k++) { const a = a0 + sweep * k / n; pts.push([cx + R * Math.cos(a), R * Math.sin(a)]); }
  };
  line(-go * c, -go * s, c, s, go + L);              // down the first diagonal, over the hump
  arc(D, Math.PI / 2 + th, -(Math.PI + 2 * th));     // round the right-hand end, turning left
  line(L * c, -L * s, -c, s, 2 * L);                 // back across, over the hump again
  arc(-D, Math.PI / 2 - th, Math.PI + 2 * th);       // round the left-hand end, turning right
  line(-L * c, -L * s, c, s, L - go);                // and on to the line
  const straight = y => [[-D, y], [D, y]];
  return { pts, hump: [0, 0, 9, 0.4], extra: [straight(-R), straight(R)], stands: [0, -R, 0] };
}
// Long sweepers and S-bends, and two corners taken as loops (a kink out, a
// long loop round, a kink back in), drawn as a turtle walks: radius and
// degrees (+ is right), or a length alone for a straight. The few metres it
// misses closing by are shared out along the way.
function driftRibbon() {
  const plan = [[70], [150, 22], [150, -22], [20], [45, -75], [45, 240], [45, -75],
    [130, -28], [130, 28], [85, 90],
    [100, 35], [75, -70], [100, 35], [20], [45, -75], [48, 240], [45, -75],
    [140, 22], [140, -22], [75, 90], [10]];
  const pts = [];
  let x = 0, y = 0, h = 0;
  for (const [r, deg] of plan) {
    if (deg === undefined) {
      for (let t = 0; t < r; t += 10) { pts.push([x, y]); x += Math.cos(h) * Math.min(10, r - t); y += Math.sin(h) * Math.min(10, r - t); }
      continue;
    }
    const turn = deg * Math.PI / 180, n = Math.max(2, Math.round(Math.abs(turn) * r / 10));
    const dh = turn / n, chord = 2 * r * Math.sin(Math.abs(dh) / 2);
    for (let k = 0; k < n; k++) { pts.push([x, y]); x += Math.cos(h + dh / 2) * chord; y += Math.sin(h + dh / 2) * chord; h += dh; }
  }
  return pts.map(([px, py], i) => [px - x * i / pts.length, py - y * i / pts.length]);
}
// The circuits: a closed loop through these points (metres, y down; the
// world is sized around them), the track width, and the start line at the
// first point, heading for the second. The loop may cross itself (the
// figure 8): the lap follows the car along it, not the nearest bit.
const DRIFT_TRACKS = {
  // The OG: the Tsukuba Circuit's 2000 course at its real size, clockwise:
  // its centre line from OpenStreetMap (© OpenStreetMap contributors, ODbL),
  // smoothed, the line on the main straight 160 m before the first corner.
  tsukuba: { name: 'The OG', w: 17.5, credit: 'The OG\'s line: © OpenStreetMap contributors.', pts: [
    [27, 235], [64, 75], [68, 63], [72, 54], [78, 47], [85, 41], [93, 38], [102, 36], [112, 38], [119, 41], [126, 47], [132, 55],
    [134, 63], [135, 68], [135, 74], [134, 82], [98, 194], [95, 204], [93, 214], [92, 222], [92, 232], [97, 285], [96, 295], [95, 305],
    [91, 320], [74, 359], [73, 365], [72, 370], [72, 378], [74, 383], [77, 388], [81, 392], [86, 395], [91, 397], [97, 397], [103, 396],
    [110, 393], [116, 388], [120, 382], [123, 374], [125, 361], [142, 250], [144, 242], [146, 237], [149, 232], [153, 227], [161, 222], [166, 219],
    [172, 218], [186, 216], [201, 216], [209, 218], [216, 221], [222, 226], [232, 236], [239, 240], [246, 241], [249, 241], [255, 239], [262, 235],
    [269, 229], [313, 184], [324, 171], [343, 145], [365, 114], [374, 96], [379, 85], [384, 70], [396, 24], [399, 16], [402, 11], [406, 7],
    [410, 3], [416, 1], [421, 0], [427, 0], [432, 2], [438, 7], [442, 11], [444, 14], [445, 20], [446, 25], [445, 31], [443, 39],
    [440, 46], [432, 60], [272, 282], [173, 416], [161, 430], [149, 440], [135, 448], [119, 454], [103, 457], [85, 457], [67, 455], [52, 450],
    [37, 441], [24, 431], [15, 420], [8, 406], [4, 393], [1, 379], [0, 368], [1, 354], [5, 332]] },
  // an oval with a figure 8 through the middle, and a hump to hop over
  eight: { name: 'Figure 8', w: 19, ...driftEight(150, 80) },
  // looping and flowing: two corners taken as loops, sweepers in between
  ribbon: { name: 'Ribbon', w: 18, pts: driftRibbon() },
};
// The physics every car starts from: metres, kg, watts. trac caps the drive
// force (x rear grip), steal is how much of the rear's side grip the drive
// and the engine braking take, caster and yawDamp are the drift assists (the
// wheels following the slide, slides winding down), lockFast the steering
// left at speed, guard the spin guard (how hard rotation is reined in past
// 45 degrees). front is the front's grip over the rear's: a little over 1,
// so the nose goes where it's pointed and the tail is the end that lets go.
// eb and eb2 are the engine braking off the throttle (m/s², and more per
// m/s of speed), brake the brakes at full squeeze (x grip). Tuned arcade,
// not real: quick, grippy when you turn, easy to hang the tail out.
// ⚠ Moving weight onto the nose under braking was tried (4 Nov): a lift or
// a dab of brake with the wheel turned then spun every car.
const DRIFT_BASE = { m: 1200, I: 2000, a: 1.15, b: 1.4, len: 4.4, wid: 1.9,
  mu: 1.55, cs: 8, power: 240000, drag: 0.55, roll: 14, lock: 0.62,
  trac: 1.3, steal: 0.55, caster: 0.42, yawDamp: 0.6, lockFast: 0.66, front: 1.08, guard: 10,
  eb: 1.2, eb2: 0.05, brake: 0.85 };
// The La Shush cars, from their AC1 versions: the handling follows their
// power, weight and build; the liveries are their default skins from above.
const DRIFT_CARS = {
  // Sierra RS500 TimeAttack: heavy, huge power, the wing keeps the rear planted
  bullet: { name: 'Bullet', body: 'black', stripe: 'yellow', shape: 'hatch',
    m: 1350, I: 2300, power: 260000, len: 4.5, wid: 1.95 },
  // the S14.9 drift chassis: light, and the most steering lock
  racer: { name: 'Racer', body: 'yellow', stripe: 'black', shape: 'coupe',
    power: 250000, lock: 0.72, caster: 0.39 },
  // VL Group A sedan: long wheelbase, less power, the easiest to hold
  neonstride: { name: 'Neonstride', body: 'black', stripe: 'yellow', shape: 'sedan',
    m: 1300, I: 2500, a: 1.3, b: 1.5, len: 4.85, wid: 1.85, power: 215000, caster: 0.47, yawDamp: 0.7, lockFast: 0.74 },
  // the hypercar: the most grip and the fastest, but it'll still slide
  hypo: { name: 'Hypo', body: 'yellow', stripe: 'black', shape: 'proto',
    m: 1050, I: 1700, len: 4.9, wid: 2, power: 255000, mu: 1.8, drag: 0.4, steal: 0.45, yawDamp: 0.75 },
};
const DRIFT_KEY = 'lsp.drift.best', DRIFT_PICK = 'lsp.drift.pick';
// metres between the track's points: a number of points is a distance
const DRIFT_STEP = 1.5;
let driftStop = null;

function driftTrack(def) {
  // Centripetal Catmull-Rom through the points (no loops or overshoot where
  // they bunch up, as round Tsukuba's hairpins), sampled finely...
  const P = def.pts, n = P.length, fine = [];
  const knot = (a, b) => Math.sqrt(Math.hypot(b[0] - a[0], b[1] - a[1])) || 1e-3;
  const mix = (a, b, ta, tb, t) => { const f = (t - ta) / (tb - ta); return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]; };
  for (let i = 0; i < n; i++) {
    const p0 = P[(i - 1 + n) % n], p1 = P[i], p2 = P[(i + 1) % n], p3 = P[(i + 2) % n];
    const t1 = knot(p0, p1), t2 = t1 + knot(p1, p2), t3 = t2 + knot(p2, p3);
    const k = Math.max(4, Math.ceil(Math.hypot(p2[0] - p1[0], p2[1] - p1[1]) / 0.5));
    for (let j = 0; j < k; j++) {
      const t = t1 + (t2 - t1) * j / k;
      const A1 = mix(p0, p1, 0, t1, t), A2 = mix(p1, p2, t1, t2, t), A3 = mix(p2, p3, t2, t3, t);
      fine.push(mix(mix(A1, A2, 0, t2, t), mix(A2, A3, t1, t3, t), t1, t2, t));
    }
  }
  // ...then laid out again every DRIFT_STEP metres
  let pts = [];
  for (let i = 0, at = 0; i < fine.length; i++) {
    const a = fine[i], b = fine[(i + 1) % fine.length], d = Math.hypot(b[0] - a[0], b[1] - a[1]);
    for (; at < d; at += DRIFT_STEP) pts.push([a[0] + (b[0] - a[0]) * at / d, a[1] + (b[1] - a[1]) * at / d]);
    at -= d;
  }
  // Road that isn't the lap (the oval's straights round the figure 8): open
  // lines, laid out the same way, drawn and driven like the rest
  let extra = (def.extra || []).map(line => {
    const out = [];
    for (let i = 0; i + 1 < line.length; i++) {
      const [a, b] = [line[i], line[i + 1]], d = Math.hypot(b[0] - a[0], b[1] - a[1]);
      for (let at = 0; at < d; at += DRIFT_STEP) out.push([a[0] + (b[0] - a[0]) * at / d, a[1] + (b[1] - a[1]) * at / d]);
    }
    out.push(line[line.length - 1]);
    return out;
  });
  // the world: the loop plus room for the scenery, its edge a wall
  const M = 40, every = pts.concat(...extra), xs = every.map(p => p[0]), ys = every.map(p => p[1]);
  const x0 = Math.min(...xs) - M, y0 = Math.min(...ys) - M;
  const shift = ([x, y]) => [x - x0, y - y0];
  pts = pts.map(shift);
  extra = extra.map(line => line.map(shift));
  const box = [Math.max(...xs) - x0 + M, Math.max(...ys) - y0 + M];
  const hump = def.hump ? [...shift(def.hump), def.hump[2], def.hump[3]] : null;
  const stands = def.stands ? [...shift(def.stands), def.stands[2]] : null;
  // every bit of road, for how far the car (or a tree) is from it
  const all = pts.concat(...extra);
  const onExtra = (x, y, r) => extra.some(line => line.some(([px, py]) => (px - x) ** 2 + (py - y) ** 2 < r * r));
  const N = pts.length, W = def.w;
  const ang = pts.map((p, i) => { const q = pts[(i + 1) % N]; return Math.atan2(q[1] - p[1], q[0] - p[0]); });
  // how hard each bit turns over 18 m, and which way (+ is right), for the
  // kerbs and the tyre walls
  const turn = ang.map((a, i) => {
    const d = ang[(i + 6) % N] - ang[(i - 6 + N) % N];
    return Math.atan2(Math.sin(d), Math.cos(d));
  });
  // the shapes, built once in metres; the camera only changes the transform
  const road = new Path2D();
  pts.forEach(([x, y], i) => i ? road.lineTo(x, y) : road.moveTo(x, y));
  road.closePath();
  for (const line of extra) line.forEach(([x, y], i) => i ? road.lineTo(x, y) : road.moveTo(x, y));
  const kerbA = new Path2D(), kerbB = new Path2D(), half = W / 2;
  for (let i = 0; i < N; i++) {
    if (Math.abs(turn[i]) < 0.2) continue;
    const [x, y] = pts[i], [x2, y2] = pts[(i + 1) % N];
    const nx = -Math.sin(ang[i]), ny = Math.cos(ang[i]);
    const k = (i >> 1) % 2 ? kerbA : kerbB;
    for (const sd of [-half, half]) {
      // no kerb across the road where an oval straight leaves the bend
      if (extra.length && onExtra(x + nx * sd, y + ny * sd, half - 0.3)) continue;
      k.moveTo(x + nx * sd, y + ny * sd);
      k.lineTo(x2 + nx * sd, y2 + ny * sd);
    }
  }
  return { pts, ang, turn, N, W, box, road, kerbA, kerbB, hump, extra, all, stands };
}
// how high the hump's dome is at x, y: 0 off it, its height in the middle
function driftHumpAt(T, x, y) {
  if (!T.hump) return 0;
  const [hx, hy, r, h] = T.hump, s = ((x - hx) ** 2 + (y - hy) ** 2) / (r * r);
  return s < 1 ? h * (1 - s) ** 2 : 0;
}

// a rectangle turned to angle a, centred on x, y: in its own frame, or tested
const rectLocal = (R, x, y) => {
  const dx = x - R.x, dy = y - R.y, c = Math.cos(R.a), s = Math.sin(R.a);
  return [dx * c + dy * s, -dx * s + dy * c];
};
const inRect = (R, x, y, m) => {
  if (!R) return false;
  const [lx, ly] = rectLocal(R, x, y);
  return Math.abs(lx) < R.w / 2 + m && Math.abs(ly) < R.h / 2 + m;
};

/* The scenery: placed once per circuit, from a fixed seed, so a circuit
   always looks the same. Trees, tyre walls and buildings are solid;
   floodlights are on at night (Black) and off by day (Ivory). All drawn from
   code: no images. */
function driftScenery(T) {
  let seed = 20261101;
  const rnd = () => (seed = seed * 16807 % 2147483647) / 2147483647;
  const away = (x, y) => {
    let bd = Infinity;
    for (const [px, py] of T.all) bd = Math.min(bd, (px - x) ** 2 + (py - y) ** 2);
    return Math.sqrt(bd);
  };
  const half = T.W / 2;
  // the pit building and the grandstand face each other across the main
  // straight (35 m past the line, or where the circuit says); each is
  // shortened, or left out, where the circuit comes close. Their local y
  // points away from the track: the garages are on -y.
  const i0 = Math.round(35 / DRIFT_STEP);
  const [sx, sy, sa] = T.stands || [...T.pts[i0], T.ang[i0]];
  const place = (side, off, w, h) => {
    const a = sa, px = sx, py = sy;
    const nx = -Math.sin(a) * side, ny = Math.cos(a) * side, d = half + off + h / 2;
    for (let len = w; len >= w * 0.5; len *= 0.8) {
      const R = { x: px + nx * d, y: py + ny * d, a: side > 0 ? a : a + Math.PI, w: len, h };
      const c = Math.cos(R.a), s = Math.sin(R.a);
      let ok = true;
      for (let u = -1; u <= 1; u += 0.25) for (const v of [-1, 1]) {
        const lx = u * len / 2, ly = v * h / 2;
        if (away(R.x + lx * c - ly * s, R.y + lx * s + ly * c) < half + off - 0.5) ok = false;
      }
      if (ok) return R;
    }
    return null;
  };
  const pit = place(1, 6, 90, 9), stand = place(-1, 6, 100, 11);
  const clear = (x, y, m) => !inRect(pit, x, y, m) && !inRect(stand, x, y, m);
  // tyre walls on the outside of the corners, three tyres to a stack
  const tyres = [];
  for (let i = 0; i < T.N; i += 4) {
    const d = T.turn[i];
    if (Math.abs(d) < 0.22) continue;
    const a = T.ang[i], side = d > 0 ? -1 : 1, off = half + 4;
    const x = T.pts[i][0] - Math.sin(a) * side * off, y = T.pts[i][1] + Math.cos(a) * side * off;
    if (away(x, y) < off - 0.5 || !clear(x, y, 1)) continue;
    for (let k = -1; k <= 1; k++) tyres.push([x + Math.cos(a) * k * 0.95, y + Math.sin(a) * k * 0.95, (i >> 2) % 2]);
  }
  // floodlight poles every 50 m or so, alternating sides
  const lights = [];
  const pole = (px, py, a, s) => {
    const off = half + 2.5, x = px - Math.sin(a) * s * off, y = py + Math.cos(a) * s * off;
    if (away(x, y) < off - 0.5 || !clear(x, y, 1)) return;
    // the pool of light falls on the track, between the pole and the centre
    lights.push([x, y, x + Math.sin(a) * s * 5, y - Math.cos(a) * s * 5]);
  };
  for (let i = 0, s = 1; i < T.N; i += 30, s = -s) pole(...T.pts[i], T.ang[i], s);
  for (const line of T.extra) {
    for (let i = 15, s = 1; i < line.length - 1; i += 30, s = -s) {
      const [x, y] = line[i], [x2, y2] = line[i + 1];
      pole(x, y, Math.atan2(y2 - y, x2 - x), s);
    }
  }
  // trees, well back from the track
  const trees = [], many = T.box[0] * T.box[1] / 450;
  for (let n = 0; n < many * 3 && trees.length < many; n++) {
    const x = 6 + rnd() * (T.box[0] - 12), y = 6 + rnd() * (T.box[1] - 12);
    const r = 2 + rnd() * 2.6;
    if (away(x, y) < half + 10 + r || !clear(x, y, 4 + r)) continue;
    trees.push([x, y, r]);
  }
  return { pit, stand, tyres, lights, trees };
}

function driftLoad(key, empty) {
  try { return JSON.parse(localStorage.getItem(key)) || empty; } catch (e) { return empty; }
}
function driftSave(key, v) {
  try { localStorage.setItem(key, JSON.stringify(v)); } catch (e) {}
}
const driftTime = s => s == null ? '-' : Math.floor(s / 60) + ':' + (s % 60).toFixed(2).padStart(5, '0');

async function driftPage() {
  if (driftStop) driftStop();
  const pick = driftLoad(DRIFT_PICK, {});
  const carId = DRIFT_CARS[pick.car] ? pick.car : 'bullet';
  const trackId = DRIFT_TRACKS[pick.track] ? pick.track : 'tsukuba';
  const K = { ...DRIFT_BASE, ...DRIFT_CARS[carId] };
  const p = $('#page');
  p.innerHTML = '';
  const card = el('div', 'card');
  const credit = DRIFT_TRACKS[trackId].credit;
  // (no heading: the page's own title says Drift Shed)
  card.innerHTML = '<p class="tiny dim">Something to do while a car converts. Arrow keys or WASD to drive: '
    + 'lift off and the engine slows you, Down brakes harder (hold it once stopped to reverse). '
    + 'Space for the handbrake, R to reset, P to pause. Hold an angle for drift points; '
    + 'more than a moment on the grass ends the combo.' + (credit ? ' ' + esc(credit) : '') + '</p>';
  const status = el('div', 'tiny dim');
  status.style.margin = '0 0 10px';
  // the car and the circuit: a choice rebuilds the page around it
  const picker = el('div', 'row wrap');
  picker.style.cssText = 'gap:6px 18px;margin:0 0 10px;align-items:center';
  const group = (label, list, cur, key) => {
    const g = el('div', 'row');
    g.style.cssText = 'gap:6px;align-items:center';
    g.append(el('span', 'tiny dim', esc(label)));
    for (const [id, def] of Object.entries(list)) {
      const b = el('button', 'sm' + (id === cur ? ' primary' : ''), esc(def.name));
      b.onclick = () => { b.blur(); if (id !== cur) { driftSave(DRIFT_PICK, { car: carId, track: trackId, [key]: id }); driftPage(); } };
      g.append(b);
    }
    picker.append(g);
  };
  group('Car', DRIFT_CARS, carId, 'car');
  group('Track', DRIFT_TRACKS, trackId, 'track');
  const stats = el('div', 'stats');
  stats.style.flexWrap = 'nowrap';
  stats.style.gap = '8px';
  const tile = label => {
    const s = el('div', 'stat', '<span>' + esc(label) + '</span><b class="mono">-</b>');
    // one row of five, however narrow: the track wants the height
    s.style.cssText = 'flex:1 1 0;min-width:0;max-width:none;padding:8px 10px';
    stats.append(s);
    return s.querySelector('b');
  };
  const tLap = tile('Lap'), tLast = tile('Last lap'), tBest = tile('Best lap'),
    tDrift = tile('Drift'), tBestD = tile('Best drift');
  tDrift.style.color = 'var(--quiet-line-ink)';
  const wrap = el('div');
  const cv = el('canvas');
  cv.style.cssText = 'display:block;width:100%;border:1px solid var(--steel)';
  cv.setAttribute('aria-label', 'Drift game');
  wrap.append(cv);
  card.append(status, picker, stats, wrap);
  p.append(card);

  const T = driftTrack(DRIFT_TRACKS[trackId]), S = driftScenery(T), BOX = T.box;
  const ctx = cv.getContext('2d');
  let C = {}, theme = null, dpr = 1;
  // best lap and best drift, kept per car and circuit
  const bests = driftLoad(DRIFT_KEY, {}), slot = trackId + '|' + carId;
  const best = bests[slot] = bests[slot] && typeof bests[slot] === 'object' ? bests[slot] : {};
  const saveBest = () => driftSave(DRIFT_KEY, bests);
  tBest.textContent = driftTime(best.lap);
  tBestD.textContent = best.drift ? best.drift.toLocaleString() : '-';

  function colours() {
    theme = document.documentElement.dataset.theme || '';
    const s = getComputedStyle(document.documentElement);
    const v = n => s.getPropertyValue(n).trim();
    const ivory = theme === 'ivory';
    // Ivory: never yellow as text or a thin line, only as a fill (app.css)
    C = { ground: v('--bg'), track: ivory ? v('--line2') : v('--panel3'), edge: v('--line2'),
      fg: v('--fg'), dim: v('--dim'), signal: v('--signal'), shed: v('--shed'),
      bone: ivory ? v('--ivory') : v('--bone'), beyond: v('--beyond'), optimal: v('--optimal'),
      ink: ivory ? v('--shed') : v('--signal'),
      // The Quiet Line's blue, for the drift: the combo and the tyre walls
      blue: v('--quiet-line') || '#5B9BE6', blueInk: v('--quiet-line-ink') || '#5B9BE6',
      mark: ivory ? 'rgba(10,10,10,0.2)' : 'rgba(0,0,0,0.5)',
      veil: ivory ? 'rgba(250,246,236,0.84)' : 'rgba(10,10,10,0.74)',
      // the scenery: Black is the circuit at night, Ivory by day
      night: !ivory,
      tree: ivory ? '#8EA27B' : '#12261A', treeHi: ivory ? '#A4B690' : '#1B3725',
      shade: 'rgba(60,50,30,0.16)', tyre: ivory ? '#1A1A1A' : '#050505', rim: ivory ? '#7A746A' : '#2E2E2E',
      building: ivory ? v('--panel3') : '#161616', roof: ivory ? v('--line2') : '#222222',
      pole: ivory ? '#7A746A' : '#3A3A3A', rubber: ivory ? 'rgba(10,10,10,0.07)' : 'rgba(0,0,0,0.22)',
      // the cars' paint: La Shush black and Signal Yellow
      paint: { black: '#141414', yellow: v('--signal') }, glass: '#0A0A0A',
      carRim: ivory ? 'rgba(10,10,10,0.35)' : 'rgba(238,233,223,0.28)' };
    C.car = carSprite();
    C.grass = pattern(96, 12, ivory ? ['#CCD5B4', '#C3CDA9', '#B6C29A', '#D8E0C3']
      : ['#0D1610', '#101B13', '#16241A', '#090F0B'], 420, true);
    C.asphalt = pattern(64, 6, ivory ? [C.track, C.track, '#BFB49D', '#DCD3C1']
      : [C.track, C.track, '#2C2C2C', '#191919'], 520, false);
  }
  // Ground textures: one tile of a few metres, repeated - mown stripes for
  // grass, speckle and the odd crack for asphalt
  function pattern(px, metres, [a, b, s1, s2], specks, stripes) {
    const t = document.createElement('canvas');
    t.width = t.height = px;
    const g = t.getContext('2d');
    g.fillStyle = a; g.fillRect(0, 0, px, px);
    if (stripes) { g.fillStyle = b; g.fillRect(px / 2, 0, px / 2, px); }
    let sd = 7;
    const r = () => (sd = sd * 16807 % 2147483647) / 2147483647;
    for (let i = 0; i < specks; i++) {
      g.fillStyle = i % 2 ? s1 : s2;
      g.fillRect(r() * px | 0, r() * px | 0, 1, 1 + (r() * 2 | 0));
    }
    if (!stripes) {
      g.strokeStyle = s2; g.lineWidth = 0.7;
      for (let i = 0; i < 3; i++) {
        let x = r() * px, y = r() * px;
        g.beginPath(); g.moveTo(x, y);
        for (let k = 0; k < 5; k++) { x += (r() - 0.5) * 14; y += (r() - 0.5) * 14; g.lineTo(x, y); }
        g.stroke();
      }
    }
    const pat = ctx.createPattern(t, 'repeat');
    pat.setTransform(new DOMMatrix().scale(metres / px));
    return pat;
  }
  // light sprites, made once: a floodlight's pool, a headlight's beam and a
  // tail light's red glow
  const sprite = (w, h, paint) => { const c = document.createElement('canvas'); c.width = w; c.height = h; paint(c.getContext('2d')); return c; };
  const GLOW = sprite(64, 64, g => {
    const r = g.createRadialGradient(32, 32, 0, 32, 32, 32);
    r.addColorStop(0, 'rgba(255,40,40,0.55)'); r.addColorStop(1, 'rgba(255,40,40,0)');
    g.fillStyle = r; g.fillRect(0, 0, 64, 64);
  });
  const POOL = sprite(128, 128, g => {
    const r = g.createRadialGradient(64, 64, 0, 64, 64, 64);
    r.addColorStop(0, 'rgba(255,238,205,0.30)'); r.addColorStop(0.55, 'rgba(255,238,205,0.12)'); r.addColorStop(1, 'rgba(255,238,205,0)');
    g.fillStyle = r; g.fillRect(0, 0, 128, 128);
  });
  const BEAM = sprite(160, 80, g => {
    const r = g.createRadialGradient(0, 40, 0, 0, 40, 160);
    r.addColorStop(0, 'rgba(255,248,225,0.42)'); r.addColorStop(1, 'rgba(255,248,225,0)');
    g.fillStyle = r;
    g.beginPath(); g.moveTo(0, 34); g.lineTo(160, 0); g.lineTo(160, 80); g.lineTo(0, 46); g.closePath(); g.fill();
  });
  function size() {
    dpr = Math.min(2, window.devicePixelRatio || 1);
    const w = Math.max(280, wrap.clientWidth);
    // as tall as fits under the tiles without scrolling the page
    const pg = $('#page');
    const room = pg ? pg.getBoundingClientRect().bottom - wrap.getBoundingClientRect().top - 36 : 400;
    const h = Math.round(Math.max(240, Math.min(w * 0.62, room)));
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
    cv.style.height = h + 'px';
  }

  // ---- state ----
  const keys = {};
  // tyre marks: a ring of segments, x1 y1 x2 y2 each, oldest overwritten
  const MARKS = 2400, marks = new Float32Array(MARKS * 4);
  let car, cam, lap, lastMark, nMarks = 0, combo = 0, comboT = 0, calmT = 0, grassT = 0,
    flash = '', flashT = 0, flashBad = false, running = false, started = false,
    raf = 0, prevT = 0, sector = 0, offTrack = false, hudT = 0;
  function reset() {
    const i = 6;
    // i: where along the track the car is; z, vz: its height and climb (the
    // hump); brake: how far the pedal is squeezed; still: how long Down has
    // been held at a standstill; land: the moment a landing unsettles it
    car = { x: T.pts[i][0], y: T.pts[i][1], h: T.ang[i], u: 0, w: 0, r: 0, steer: 0, i,
      z: 0, vz: 0, air: false, airT: 0, brake: 0, still: 0, land: 0 };
    cam = { x: car.x, y: car.y, z: 1 };
    lap = null; lastMark = null; sector = 0; combo = 0; comboT = 0; grassT = 0; nMarks = 0;
  }

  // How far the car is from the road, and where along the lap it is. The
  // distance is to the nearest bit of road (the oval's straights too); where
  // it is follows the car along the lap from last time, so on the figure 8
  // the crossing straight doesn't steal the lap. Only more than the track's
  // width off the lap does it jump to the nearest bit of it, and a jump of
  // more than 60 m along is a lap cut short (true back to update).
  function locate() {
    // a couple of thousand points: a full search is cheaper than clever
    let gd = Infinity, gi = 0, gl = Infinity, li = car.i, ld = Infinity;
    for (const [x, y] of T.all) gd = Math.min(gd, (x - car.x) ** 2 + (y - car.y) ** 2);
    const d2 = i => (T.pts[i][0] - car.x) ** 2 + (T.pts[i][1] - car.y) ** 2;
    for (let i = 0; i < T.N; i++) { const d = d2(i); if (d < gl) { gl = d; gi = i; } }
    for (let k = -40; k <= 40; k++) {
      const i = (car.i + k + T.N) % T.N, d = d2(i);
      if (d < ld) { ld = d; li = i; }
    }
    let cut = false;
    if (ld <= T.W * T.W) car.i = li;
    else {
      const along = Math.abs(gi - car.i);
      cut = Math.min(along, T.N - along) * DRIFT_STEP > 60;
      car.i = gi;
    }
    return [Math.sqrt(gd), cut];
  }

  // One physics step. Body frame: u forward, w to the right, r yaw rate
  // (clockwise on screen, as y is down). Returns whether the rears slide.
  function step(dt) {
    const k = K, L = k.a + k.b, g = 9.81;
    const thr = keys.up ? 1 : 0, hb = keys.hb ? 1 : 0, air = car.air;
    const want = (keys.right ? 1 : 0) - (keys.left ? 1 : 0);
    // steering: quick in, quicker back, less lock at speed
    const rate = want ? 6 : 8;
    car.steer += Math.max(-rate * dt, Math.min(rate * dt, want - car.steer));
    const u = car.u, w = car.w, r = car.r, su = u >= 0 ? 1 : -1;
    // Down: the brakes while rolling forward, squeezed on from a third to
    // full over a fifth of a second, so a tap is a dab and a hold is a stop.
    // Reverse only once stopped and Down still held, never by surprise.
    car.still = keys.down && u < 0.5 ? car.still + dt : 0;
    const rev = car.still > 0.35;
    car.brake = keys.down && !rev ? Math.max(0.35, Math.min(1, car.brake + dt * 3)) : 0;
    const brk = car.brake;
    // Steering into the slide (the countersteer) always gets full lock: the
    // speed limit is only there to stop a twitchy turn-in. And the wheels
    // follow the slide by themselves a little, as a drift car's caster
    // does - without it a slide past the lock could never be caught.
    const beta = u > 3 ? Math.atan2(w, u) : 0;
    const lim = car.steer * beta > 0 ? 1 : Math.max(k.lockFast, 1 - Math.abs(u) / 75);
    const d = car.d = Math.max(-1, Math.min(1, car.steer * k.lock * lim + k.caster * beta));
    // the grass has less grip, and a landing takes a moment to settle
    if (car.land > 0) car.land -= dt;
    const mu = k.mu * (offTrack ? 0.75 : 1) * (car.land > 0 ? 0.75 : 1);
    const Wt = k.m * g, Nf = Wt * k.b / L, Nr = Wt * k.a / L;
    const ue = Math.max(Math.abs(u), 1.5);
    // longitudinal: engine (power limited), engine braking off the throttle
    // (more the faster it goes), brakes, reverse up to 12 m/s. In the air
    // the tyres touch nothing.
    const drive = !air && thr ? Math.min(k.trac * mu * Nr, k.power / Math.max(Math.abs(u), 4)) : 0;
    const eng = !air && !thr && !brk && !rev && !hb && u > 1 ? k.m * (k.eb + k.eb2 * u) : 0;
    const stop = !air && brk && u > 0.5 ? brk * k.brake * mu * Wt : 0;
    let Fx = drive - eng - stop;
    if (!air && rev) Fx -= 0.35 * mu * Nr * Math.max(0, 1 + Math.min(0, u) / 12);
    if (!air && hb) Fx -= su * 0.5 * mu * Nr;
    Fx -= k.drag * u * Math.abs(u) + (air ? 0 : k.roll * u * (offTrack ? 5 : 1));
    // lateral: slip angles into a linear tyre that saturates; the rear only
    // has what the drive, the engine braking, its third of the brakes and
    // the handbrake leave of its grip
    const af = Math.atan((w + k.a * r) / ue) - d * su;
    const ar = Math.atan((w - k.b * r) / ue);
    const capF = air ? 0 : mu * Nf * k.front, gripR = air ? 0 : mu * Nr * (hb ? 0.45 : 1);
    const capR = Math.sqrt(Math.max(0, gripR * gripR - ((drive + eng + stop / 3) * k.steal) ** 2));
    const Fyf = Math.max(-capF, Math.min(capF, -k.cs * Nf * af));
    const Fyr = Math.max(-capR, Math.min(capR, -k.cs * Nr * ar));
    const cd = Math.cos(d), sd = Math.sin(d);
    car.u += ((Fx - Fyf * sd) / k.m + w * r) * dt;
    car.w += ((Fyf * cd + Fyr) / k.m - u * r) * dt;
    car.r += ((k.a * Fyf * cd - k.b * Fyr) / k.I) * dt;
    // a little yaw damping off the handbrake: slides wind down, not up
    if (!hb) car.r -= car.r * k.yawDamp * dt;
    // The spin guard: past about 45 degrees of slide the rotation is reined
    // in hard, so a big drift holds its angle instead of spinning out
    const over = Math.abs(beta) - 0.8;
    if (over > 0 && car.r * beta < 0) car.r -= car.r * k.guard * over * dt;
    // standing still means still
    if (!air && !thr && !rev && Math.abs(car.u) < 0.4) { car.u *= 0.9; car.w *= 0.9; car.r *= 0.9; }
    car.h += car.r * dt;
    const c = Math.cos(car.h), s = Math.sin(car.h);
    car.x += (car.u * c - car.w * s) * dt;
    car.y += (car.u * s + car.w * c) * dt;
    if (T.hump) hop(dt, g);
    if (offTrack) collide(c, s);
    const m = 3;
    if (car.x < m || car.x > BOX[0] - m || car.y < m || car.y > BOX[1] - m) {
      car.x = Math.max(m, Math.min(BOX[0] - m, car.x));
      car.y = Math.max(m, Math.min(BOX[1] - m, car.y));
      car.u *= -0.25; car.w *= 0.3; car.r *= 0.3;
      loseCombo('Into the wall.');
    }
    return !air && (Math.abs(ar) > 0.12 || (hb || brk > 0.6) && Math.abs(u) > 3);
  }

  // The hump: on the ground the car rides its dome; where the dome falls
  // away faster than the car would fall, it flies, and lands where it lands.
  // Slow, it's a bump; flat out, a hop of twenty or thirty metres.
  function hop(dt, g) {
    const ground = driftHumpAt(T, car.x, car.y);
    if (car.air) {
      car.vz -= g * dt; car.z += car.vz * dt; car.airT += dt;
      if (car.z > ground) return;
      // down: a thump that costs a little speed and unsettles the grip
      const thump = -car.vz;
      car.u *= 1 - Math.min(0.06, thump * 0.012);
      car.land = Math.min(0.3, thump * 0.08);
      car.air = false; car.z = ground; car.vz = 0;
      if (car.airT > 0.4) say('Air: ' + car.airT.toFixed(2) + ' s');
      return;
    }
    const fly = car.z + car.vz * dt - g * dt * dt / 2;
    if (fly > ground + 1e-4) { car.air = true; car.airT = 0; car.z = fly; car.vz -= g * dt; return; }
    car.vz = (ground - car.z) / dt; car.z = ground;
  }

  // The scenery is solid. The car is a 1.6 m circle: pushed back out of
  // whatever it hit, it keeps the speed along the surface and bounces a
  // little off it, so you glance off a tyre wall rather than stick to it.
  function collide(c, s) {
    const R = 1.6;
    let hit = false;
    const push = (nx, ny, depth) => {
      car.x += nx * depth; car.y += ny * depth;
      let vx = car.u * c - car.w * s, vy = car.u * s + car.w * c;
      const vn = vx * nx + vy * ny;
      if (vn < 0) { vx -= 1.3 * vn * nx; vy -= 1.3 * vn * ny; vx *= 0.8; vy *= 0.8; }
      car.u = vx * c + vy * s; car.w = -vx * s + vy * c; car.r *= 0.5;
      hit = true;
    };
    const round = (x, y, rad) => {
      const dx = car.x - x, dy = car.y - y, d = Math.hypot(dx, dy);
      if (d < rad + R && d > 1e-6) push(dx / d, dy / d, rad + R - d);
    };
    for (const B of [S.pit, S.stand]) {
      if (!B) continue;
      // the closest point of the building, back in the world
      let [lx, ly] = rectLocal(B, car.x, car.y);
      lx = Math.max(-B.w / 2, Math.min(B.w / 2, lx)); ly = Math.max(-B.h / 2, Math.min(B.h / 2, ly));
      const bc = Math.cos(B.a), bs = Math.sin(B.a);
      round(B.x + lx * bc - ly * bs, B.y + lx * bs + ly * bc, 0);
    }
    for (const [x, y, r] of S.trees) round(x, y, r * 0.55);
    for (const [x, y] of S.tyres) round(x, y, 0.45);
    if (hit) loseCombo('Into the scenery.');
  }
  function say(t, bad) { flash = t; flashT = 2.2; flashBad = !!bad; }
  function loseCombo(why) {
    if (combo > 50) say(why + ' Combo lost.', true);
    combo = 0; comboT = 0;
  }
  function bank() {
    const pts = Math.round(combo);
    combo = 0; comboT = 0;
    if (pts < 1) return;
    if (!best.drift || pts > best.drift) {
      best.drift = pts; saveBest();
      tBestD.textContent = pts.toLocaleString();
      say('New best drift: ' + pts.toLocaleString());
    } else say('+' + pts.toLocaleString());
  }

  function update(dt) {
    let slide = false;
    const sub = Math.ceil(dt * 240);
    for (let i = 0; i < sub; i++) slide = step(dt / sub) || slide;
    // off the track once the car's middle is a metre past the edge line;
    // flying over the grass doesn't count
    const [dist, cut] = locate();
    offTrack = dist > T.W / 2 + 1 && !car.air;
    grassT = offTrack ? grassT + dt : 0;
    // lap timing: four sectors in order, then the line. A shortcut (across
    // the grass, or round the oval instead of the 8) loses the lap: the
    // clock starts again at the line.
    const sec = Math.floor(car.i / T.N * 4);
    if (cut) {
      if (lap != null) say('Off the lap: the clock starts again at the line.', true);
      lap = null; sector = sec;
    } else if (sec === (sector + 1) % 4) {
      if (sec === 0) {
        if (lap != null) {
          tLast.textContent = driftTime(lap);
          if (!best.lap || lap < best.lap) {
            best.lap = lap; saveBest();
            tBest.textContent = driftTime(lap);
            say('New best lap: ' + driftTime(lap));
          }
        }
        lap = 0;
      }
      sector = sec;
    }
    if (lap != null) lap += dt;
    // drifting: 10 degrees of body slip or more, at speed, on the track
    const speed = Math.hypot(car.u, car.w);
    const beta = Math.abs(Math.atan2(car.w, Math.abs(car.u)));
    // a wheel or two on the grass for a moment is forgiven, more isn't
    if (grassT > 0.3 && combo > 0) loseCombo('Off track.');
    else if (beta > 1.9 && speed > 3) loseCombo('Spun.');
    else if (beta > 0.17 && speed > 8 && car.u > 0 && !offTrack) {
      comboT += dt; calmT = 0;
      combo += speed * beta * 57.3 * 0.12 * Math.min(5, 1 + comboT / 2) * dt;
    } else if (combo > 0 && (calmT += dt) > 0.7) bank();
    if (flashT > 0) flashT -= dt;
    // tyre marks from the rear wheels
    const c = Math.cos(car.h), s = Math.sin(car.h), hw = K.wid / 2 - 0.25;
    const bx = car.x - c * K.b, by = car.y - s * K.b;
    const now = [bx - s * hw, by + c * hw, bx + s * hw, by - c * hw];
    if (slide && speed > 2 && lastMark) {
      for (let k = 0; k < 2; k++) {
        const j = (nMarks++ % MARKS) * 4;
        marks[j] = lastMark[k * 2]; marks[j + 1] = lastMark[k * 2 + 1];
        marks[j + 2] = now[k * 2]; marks[j + 3] = now[k * 2 + 1];
      }
    }
    lastMark = slide ? now : null;
    // the camera looks ahead of the car and pulls back a little with speed
    // (only a little: pulling back far makes everything look slower)
    const look = 0.6;
    const tx = car.x + (car.u * c - car.w * s) * look, ty = car.y + (car.u * s + car.w * c) * look;
    const e = 1 - Math.exp(-dt * 4);
    cam.x += (tx - cam.x) * e; cam.y += (ty - cam.y) * e;
    cam.z += (1 / (1 + speed / 140) - cam.z) * (1 - Math.exp(-dt * 1.5));
  }

  function draw() {
    if ((document.documentElement.dataset.theme || '') !== theme) colours();
    const W = cv.width, H = cv.height;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = C.ground;
    ctx.fillRect(0, 0, W, H);
    // the world, through the camera: about 80 m across at a standstill
    const z = W / 80 * cam.z;
    ctx.setTransform(z, 0, 0, z, W / 2 - cam.x * z, H / 2 - cam.y * z);
    // what the camera can see, plus a margin: only that much scenery is drawn
    const vw = W / 2 / z + 12, vh = H / 2 / z + 12;
    const seen = (x, y) => Math.abs(x - cam.x) < vw && Math.abs(y - cam.y) < vh;
    ctx.fillStyle = C.grass;
    ctx.fillRect(0, 0, BOX[0], BOX[1]);
    ctx.strokeStyle = C.rim; ctx.lineWidth = 1; ctx.strokeRect(0, 0, BOX[0], BOX[1]);
    drawBuildings();
    // the road: kerb-side edge, white lines, textured asphalt, and the
    // darker rubbered-in line down the middle
    ctx.lineJoin = 'round';
    ctx.strokeStyle = C.edge; ctx.lineWidth = T.W + 0.8; ctx.stroke(T.road);
    ctx.strokeStyle = C.bone; ctx.lineWidth = T.W - 0.3; ctx.stroke(T.road);
    ctx.strokeStyle = C.asphalt; ctx.lineWidth = T.W - 0.9; ctx.stroke(T.road);
    ctx.strokeStyle = C.rubber; ctx.lineWidth = 4.5; ctx.stroke(T.road);
    if (T.hump) drawHump();
    // kerbs: yellow and black blocks, flat
    ctx.lineCap = 'butt'; ctx.lineWidth = 1.4;
    ctx.strokeStyle = C.signal; ctx.stroke(T.kerbA);
    ctx.strokeStyle = C.shed; ctx.stroke(T.kerbB);
    // start / finish, a two-row chequer
    const half = T.W / 2, n = 8, cell = T.W / n;
    ctx.save(); ctx.translate(T.pts[0][0], T.pts[0][1]); ctx.rotate(T.ang[0]);
    for (let r = 0; r < 2; r++) for (let k = 0; k < n; k++) {
      ctx.fillStyle = (r + k) % 2 ? C.shed : C.bone;
      ctx.fillRect((r - 1) * cell, -half + k * cell, cell, cell);
    }
    ctx.restore();
    drawQuietLine();
    if (nMarks) {
      ctx.beginPath();
      for (let k = 0, m = Math.min(nMarks, MARKS); k < m; k++) {
        const j = k * 4;
        ctx.moveTo(marks[j], marks[j + 1]); ctx.lineTo(marks[j + 2], marks[j + 3]);
      }
      ctx.strokeStyle = C.mark; ctx.lineWidth = 0.3; ctx.lineCap = 'round'; ctx.stroke();
    }
    // tyre walls: black stacks, every other one banded in Quiet Line Blue
    for (const band of [0, 1]) {
      ctx.beginPath();
      for (const [x, y, b] of S.tyres) if (b === band && seen(x, y)) { ctx.moveTo(x + 0.45, y); ctx.arc(x, y, 0.45, 0, 7); }
      ctx.fillStyle = C.tyre; ctx.fill();
      ctx.strokeStyle = band ? C.blue : C.rim; ctx.lineWidth = 0.14; ctx.stroke();
    }
    if (C.night) {
      // headlights, before the car so the beams start under its nose
      ctx.save(); ctx.translate(car.x, car.y); ctx.rotate(car.h);
      ctx.globalCompositeOperation = 'lighter';
      ctx.drawImage(BEAM, K.len / 2 - 0.2, -5, 22, 10);
      ctx.restore();
    }
    drawCar(ctx);
    drawTrees(seen);
    // floodlight poles, and at night their pools of light on the track
    ctx.fillStyle = C.pole;
    for (const [x, y] of S.lights) if (seen(x, y)) ctx.fillRect(x - 0.4, y - 0.4, 0.8, 0.8);
    if (C.night) {
      ctx.globalCompositeOperation = 'lighter';
      for (const [, , x, y] of S.lights) if (seen(x, y)) ctx.drawImage(POOL, x - 17, y - 17, 34, 34);
      ctx.fillStyle = 'rgba(255,238,205,0.9)';
      for (const [x, y] of S.lights) if (seen(x, y)) ctx.fillRect(x - 0.22, y - 0.22, 0.44, 0.44);
      ctx.globalCompositeOperation = 'source-over';
    }
    // the minimap, top right
    const mw = Math.min(150 * dpr, W * 0.28), ms = mw / BOX[0], pad = 10 * dpr;
    const mx = W - mw - pad, my = pad;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = C.veil;
    ctx.fillRect(mx - 6 * dpr, my - 6 * dpr, mw + 12 * dpr, BOX[1] * ms + 12 * dpr);
    ctx.setTransform(ms, 0, 0, ms, mx, my);
    ctx.strokeStyle = C.dim; ctx.lineWidth = 2.2 / ms * dpr; ctx.stroke(T.road);
    ctx.fillStyle = C.ink;
    const dot = 3.5 * dpr / ms;
    ctx.fillRect(car.x - dot, car.y - dot, dot * 2, dot * 2);
    // messages, in the UI's own type
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.textAlign = 'center';
    const mid = W / 2, font = (w, px) => w + ' ' + Math.round(px * dpr) + 'px Inter, "Segoe UI", sans-serif';
    if (!running) {
      ctx.fillStyle = C.veil;
      ctx.fillRect(0, 0, W, H);
      ctx.fillStyle = C.fg;
      ctx.font = font('italic 900', 34);
      ctx.fillText(started ? 'PAUSED' : 'DRIFT SHED', mid, H / 2 - 4 * dpr);
      const tw = ctx.measureText(started ? 'PAUSED' : 'DRIFT SHED').width;
      // the brand's full stop, in the accent
      ctx.textAlign = 'left'; ctx.fillStyle = C.ink;
      ctx.fillText('.', mid + tw / 2, H / 2 - 4 * dpr);
      ctx.textAlign = 'center'; ctx.fillStyle = C.dim;
      ctx.font = font('500', 12);
      ctx.fillText('La Shush ' + K.name + ' · ' + DRIFT_TRACKS[trackId].name + '. '
        + (started ? 'Press P or an arrow key to carry on' : 'Press an arrow key to start'), mid, H / 2 + 22 * dpr);
      return;
    }
    if (combo > 1) {
      ctx.fillStyle = C.blueInk;
      ctx.font = font('italic 900', 24);
      ctx.fillText(Math.round(combo).toLocaleString() + '  ×' + Math.min(5, 1 + comboT / 2).toFixed(1), mid, 38 * dpr);
    }
    if (flashT > 0) {
      ctx.globalAlpha = Math.min(1, flashT);
      ctx.fillStyle = flashBad ? C.beyond : C.optimal;
      ctx.font = font('600', 14);
      ctx.fillText(flash, mid, H - 20 * dpr);
      ctx.globalAlpha = 1;
    }
  }

  // The Quiet Line's paint on the tarmac, in its blue: the starting grid's
  // boxes behind the line, and THE QUIET LINE across the main straight.
  function drawQuietLine() {
    // a point the given metres along the track from the line (back if < 0)
    const along = d => {
      let i = 0, left = Math.abs(d);
      const step = d < 0 ? -1 : 1;
      while (left > 0) {
        const j = (i + step + T.N) % T.N;
        left -= Math.hypot(T.pts[j][0] - T.pts[i][0], T.pts[j][1] - T.pts[i][1]);
        i = j;
      }
      return [T.pts[i][0], T.pts[i][1], T.ang[i]];
    };
    ctx.strokeStyle = C.blue; ctx.lineWidth = 0.22; ctx.lineCap = 'butt';
    for (let k = 0; k < 6; k++) {
      const [x, y, a] = along(-9 - k * 7);
      const side = k % 2 ? 1 : -1;
      ctx.save(); ctx.translate(x, y); ctx.rotate(a);
      // a box open at the front, the grid slot's number-plate line at the back
      const oy = side * T.W / 4;
      ctx.beginPath();
      ctx.moveTo(2.4, oy - 1.3); ctx.lineTo(-2.4, oy - 1.3); ctx.lineTo(-2.4, oy + 1.3); ctx.lineTo(2.4, oy + 1.3);
      ctx.stroke();
      ctx.restore();
    }
    const [x, y, a] = along(40);
    ctx.save(); ctx.translate(x, y); ctx.rotate(a);
    // keep the lettering the right way up whichever way the straight runs
    if (Math.cos(a) < 0) ctx.rotate(Math.PI);
    ctx.globalAlpha = 0.85;
    ctx.fillStyle = C.blueInk;
    ctx.font = 'italic 900 3.4px Inter, "Segoe UI", sans-serif';
    ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
    ctx.fillText('THE QUIET LINE', 0, 0);
    ctx.globalAlpha = 1; ctx.textBaseline = 'alphabetic';
    ctx.restore();
  }

  // The hump where the figure 8 crosses: a dome, lit on the side toward the
  // light and shaded on the far side, ringed in white paint.
  function drawHump() {
    const [x, y, R] = T.hump;
    ctx.save();
    ctx.beginPath(); ctx.arc(x, y, R, 0, 7); ctx.clip();
    let gr = ctx.createRadialGradient(x - R * 0.3, y - R * 0.3, 0, x, y, R);
    gr.addColorStop(0, C.night ? 'rgba(255,255,255,0.11)' : 'rgba(255,255,255,0.38)');
    gr.addColorStop(1, 'rgba(255,255,255,0)');
    ctx.fillStyle = gr; ctx.fillRect(x - R, y - R, R * 2, R * 2);
    gr = ctx.createRadialGradient(x - R * 0.45, y - R * 0.45, R * 0.8, x - R * 0.2, y - R * 0.2, R * 1.5);
    gr.addColorStop(0, 'rgba(0,0,0,0)');
    gr.addColorStop(1, C.night ? 'rgba(0,0,0,0.4)' : 'rgba(0,0,0,0.16)');
    ctx.fillStyle = gr; ctx.fillRect(x - R, y - R, R * 2, R * 2);
    ctx.restore();
    ctx.strokeStyle = C.bone; ctx.lineWidth = 0.25; ctx.globalAlpha = 0.75;
    ctx.setLineDash([1.2, 1]);
    ctx.beginPath(); ctx.arc(x, y, R - 0.3, 0, 7); ctx.stroke();
    ctx.setLineDash([]); ctx.globalAlpha = 1;
  }

  // The car from above: the wheels (the fronts turned), then its picture.
  // Off the ground its shadow stays down and the car comes up at the camera.
  function drawCar(g) {
    const hw = K.wid / 2, P = C.car, z = car.z, lift = z - driftHumpAt(T, car.x, car.y);
    if (lift > 0.03) {
      g.save();
      g.translate(car.x + lift * 0.9, car.y + lift * 1.2); g.rotate(car.h);
      g.globalAlpha = Math.max(0.12, 0.38 - lift * 0.15);
      g.fillStyle = '#000'; g.fillRect(-K.len / 2, -hw, K.len, K.wid);
      g.restore();
    }
    g.save();
    g.translate(car.x, car.y);
    g.rotate(car.h);
    if (z > 0.03) g.scale(1 + z * 0.16, 1 + z * 0.16);
    g.fillStyle = '#050505';
    for (const [wx, wy] of [[K.a, -1], [K.a, 1], [-K.b, -1], [-K.b, 1]]) {
      g.save(); g.translate(wx, wy * (hw - 0.14));
      if (wx > 0) g.rotate(car.d || 0);
      g.fillRect(-0.36, -0.16, 0.72, 0.32); g.restore();
    }
    g.drawImage(P.img, -P.w / 2, -P.h / 2, P.w, P.h);
    // Tail lights: on at night, bright with a glow under braking (the brake
    // key while rolling forward; backing up isn't braking)
    const t = P.tail, braking = keys.down && car.u > 0.3;
    if (braking || C.night) {
      g.fillStyle = braking ? '#FF2A2A' : '#C0272D';
      g.fillRect(t.x, t.y, t.w, t.h); g.fillRect(t.x, -t.y - t.h, t.w, t.h);
      g.globalCompositeOperation = 'lighter';
      const r = braking ? (C.night ? 1.6 : 0.9) : 0.7;
      for (const y of [t.y + t.h / 2, -t.y - t.h / 2]) g.drawImage(GLOW, t.x - r, y - r, r * 2, r * 2);
    }
    g.restore();
  }

  /* The car's picture, in its AC1 livery: drawn once per theme at 48 px a
     metre, then just stamped every frame. Not to scale in every detail, but
     each car its own: x forward, y to the right, metres. */
  function carSprite() {
    const PX = 48, pad = 0.7, L = K.len, W = K.wid, hl = L / 2, hw = W / 2;
    const cvs = document.createElement('canvas');
    cvs.width = Math.ceil((L + pad * 2) * PX); cvs.height = Math.ceil((W + pad * 2) * PX);
    const g = cvs.getContext('2d');
    g.setTransform(PX, 0, 0, PX, cvs.width / 2, cvs.height / 2);
    const body = C.paint[K.body], stripe = C.paint[K.stripe], dark = '#0B0B0B';
    const shape = K.shape;
    // The outline: the left side (y < 0) from the nose to the tail as lines
    // and curves, sampled to points, then mirrored for the right.
    const side = [];
    let px = hl, py = 0;
    const to = (x, y) => { side.push([x, y]); px = x; py = y; };
    const curve = (cx, cy, x, y) => {
      const x0 = px, y0 = py;
      for (let i = 1; i <= 8; i++) {
        const t = i / 8, u = 1 - t;
        to(u * u * x0 + 2 * u * t * cx + t * t * x, u * u * y0 + 2 * u * t * cy + t * t * y);
      }
    };
    // a wheel arch: a bulge of the side over the wheel
    const arch = (x, y, out) => { to(x + 0.7, y); curve(x, y - out * 2, x - 0.7, y); };
    side.push([hl, 0]);
    if (shape === 'proto') {
      // pointed nose, front wheel pods, pinched sidepods, wide rear
      curve(hl - 0.1, -0.35, hl - 0.75, -0.5);
      curve(K.a + 0.6, -0.55, K.a + 0.35, -hw + 0.05);
      curve(K.a - 0.2, -hw - 0.02, K.a - 0.75, -hw + 0.25);
      curve(-0.2, -hw + 0.4, -K.b + 0.7, -hw + 0.12);
      curve(-K.b, -hw - 0.02, -hl + 0.3, -hw + 0.05);
      curve(-hl, -hw + 0.05, -hl, -hw + 0.35); to(-hl, 0);
    } else {
      const bw = hw - (shape === 'hatch' ? 0.12 : 0.05);
      const nose = shape === 'coupe' ? 0.55 : 0.32, tail = shape === 'sedan' ? 0.22 : 0.3;
      to(hl, -(bw - nose - 0.15));
      curve(hl - 0.02, -bw + 0.02, hl - nose, -bw);
      arch(K.a, -bw, shape === 'hatch' ? 0.12 : 0.04);
      arch(-K.b, -bw, shape === 'hatch' ? 0.12 : 0.04);
      to(-hl + tail, -bw);
      curve(-hl, -bw + 0.02, -hl, -bw + tail + 0.1); to(-hl, 0);
    }
    const outline = new Path2D();
    side.forEach(([x, y], i) => i ? outline.lineTo(x, y) : outline.moveTo(x, y));
    for (let i = side.length - 1; i >= 0; i--) outline.lineTo(side[i][0], -side[i][1]);
    outline.closePath();
    const poly = pts => { g.beginPath(); pts.forEach(([x, y], i) => i ? g.lineTo(x, y) : g.moveTo(x, y)); g.closePath(); g.fill(); };
    const both = (x, y, w, h) => { g.fillRect(x, y, w, h); g.fillRect(x, -y - h, w, h); };

    // body, with a soft shadow under it
    g.save();
    g.shadowColor = 'rgba(0,0,0,0.5)'; g.shadowBlur = 0.3 * PX; g.shadowOffsetX = 0.06 * PX; g.shadowOffsetY = 0.14 * PX;
    g.fillStyle = body; g.fill(outline);
    g.restore();
    g.save();
    g.clip(outline);
    // stripes
    g.fillStyle = stripe;
    if (shape === 'sedan') {
      for (const y of [-0.62, 0.44]) { g.fillRect(0.95, y, hl - 0.95, 0.18); g.fillRect(-hl, y, 1.05, 0.18); }
      both(-hl, -hw, L, 0.14);
    } else {
      g.fillRect(-hl, -0.36, L, 0.24); g.fillRect(-hl, 0.12, L, 0.24);
    }
    // glass: windscreen, side windows and rear window, or the canopy
    g.fillStyle = dark;
    if (shape === 'proto') {
      g.beginPath(); g.ellipse(0.15, 0, 1.05, 0.46, 0, 0, 7); g.fill();
      g.fillStyle = 'rgba(255,255,255,0.18)';
      g.beginPath(); g.ellipse(0.45, -0.16, 0.45, 0.1, -0.15, 0, 7); g.fill();
    } else {
      const G = { hatch: [0.8, 0.12, -1.0, -1.8], coupe: [0.7, 0.02, -0.85, -1.45], sedan: [0.9, 0.25, -0.95, -1.4] }[shape];
      const [w0, w1, r0, r1] = G, gw = hw - 0.3, rw = hw - 0.42;
      poly([[w0, -gw], [w0 + 0.08, 0], [w0, gw], [w1, rw], [w1, -rw]]);
      poly([[r0, -rw], [r0, rw], [r1, gw - 0.08], [r1 - 0.05, 0], [r1, -gw + 0.08]]);
      both(r0, -gw + 0.02, w1 - r0, 0.13);
      // a glint on the windscreen
      g.fillStyle = 'rgba(255,255,255,0.16)';
      poly([[w0 - 0.08, -gw + 0.25], [w0 - 0.04, -gw + 0.6], [w1 + 0.12, -rw + 0.5], [w1 + 0.1, -rw + 0.25]]);
    }
    // bonnet vents (the hatch and the coupe), the sedan's LSP
    if (shape === 'hatch' || shape === 'coupe') {
      g.fillStyle = dark;
      for (let i = 0; i < 3; i++) both(hl - 1.25 + i * 0.13, -0.85, 0.07, 0.32);
    }
    if (shape === 'sedan') {
      g.fillStyle = stripe;
      g.font = 'italic 900 0.5px Inter, "Segoe UI", sans-serif';
      g.textAlign = 'center'; g.textBaseline = 'middle';
      g.save(); g.translate(1.65, 0); g.rotate(Math.PI / 2); g.fillText('LSP', 0, 0); g.restore();
    }
    // lights: headlights at the nose, red at the tail
    g.fillStyle = '#EDE6D2';
    if (shape === 'proto') { both(hl - 0.95, -0.52, 0.42, 0.08); }
    else if (shape === 'coupe') { both(hl - 0.42, -bwOf(shape) + 0.12, 0.3, 0.32); }
    else both(hl - 0.1, -hw + 0.3, 0.1, 0.42);
    // the sheen: darker at the sides, lighter down the middle, so it reads as rounded
    const sheen = g.createLinearGradient(0, -hw, 0, hw);
    sheen.addColorStop(0, 'rgba(0,0,0,0.38)'); sheen.addColorStop(0.2, 'rgba(255,255,255,0.06)');
    sheen.addColorStop(0.5, 'rgba(255,255,255,0.16)'); sheen.addColorStop(0.8, 'rgba(255,255,255,0.06)');
    sheen.addColorStop(1, 'rgba(0,0,0,0.38)');
    g.fillStyle = sheen; g.fillRect(-hl - 0.1, -hw - 0.1, L + 0.2, W + 0.2);
    g.restore();
    g.strokeStyle = C.carRim; g.lineWidth = 0.05; g.stroke(outline);
    // mirrors
    if (shape !== 'proto') {
      const [w0] = { hatch: [0.8], coupe: [0.7], sedan: [0.9] }[shape];
      g.fillStyle = body;
      both(w0 - 0.25, -hw - 0.16, 0.16, 0.2);
    }
    // the rear wing on its posts, wider than the body; the sedan's lip spoiler
    g.fillStyle = dark;
    if (shape === 'sedan') {
      g.fillRect(-hl + 0.08, -hw + 0.25, 0.12, W - 0.5);
    } else {
      const span = hw + (shape === 'proto' ? 0.05 : 0.08);
      both(-hl + 0.25, -0.55, 0.3, 0.08);
      g.fillRect(-hl - 0.12, -span, 0.42, span * 2);
      g.fillStyle = 'rgba(255,255,255,0.14)'; g.fillRect(-hl + 0.22, -span, 0.08, span * 2);
      g.fillStyle = dark; both(-hl - 0.18, -span - 0.04, 0.55, 0.08);
      if (K.body === 'black') { g.strokeStyle = C.carRim; g.lineWidth = 0.04; g.strokeRect(-hl - 0.12, -span, 0.42, span * 2); }
    }
    // tail lights, at the very back so the wing doesn't hide them; drawCar
    // lights them up under braking
    const tail = { x: -hl - 0.04, y: -hw + 0.2, w: 0.12, h: shape === 'sedan' ? 0.55 : 0.36 };
    g.fillStyle = '#7A1418';
    both(tail.x, tail.y, tail.w, tail.h);
    return { img: cvs, w: cvs.width / PX, h: cvs.height / PX, tail };
    function bwOf(s) { return hw - (s === 'hatch' ? 0.12 : 0.05); }
  }

  // the pit building (LSP on the roof, its garages lit at night) and the
  // grandstand (rows of blue and yellow seats), each turned to the straight
  function drawBuildings() {
    const B = S.pit;
    if (B) {
      ctx.save(); ctx.translate(B.x, B.y); ctx.rotate(B.a);
      const x0 = -B.w / 2, y0 = -B.h / 2;
      ctx.fillStyle = C.building; ctx.fillRect(x0, y0, B.w, B.h);
      ctx.fillStyle = C.roof;
      for (let x = x0 + 2; x < x0 + B.w - 6; x += 6) ctx.fillRect(x, y0, 4.6, 1.2);
      if (C.night) {
        ctx.globalCompositeOperation = 'lighter';
        ctx.fillStyle = 'rgba(255,214,150,0.55)';
        for (let x = x0 + 2; x < x0 + B.w - 6; x += 6) ctx.fillRect(x, y0, 4.6, 1.2);
        ctx.fillStyle = 'rgba(255,214,150,0.07)';
        for (let x = x0 + 2; x < x0 + B.w - 6; x += 6) ctx.fillRect(x - 1, y0 - 5, 6.6, 5);
        ctx.globalCompositeOperation = 'source-over';
      }
      // on Ivory never yellow type (app.css): the roof lettering goes black
      ctx.fillStyle = C.night ? C.signal : C.shed;
      ctx.font = 'italic 900 5.5px Inter, "Segoe UI", sans-serif';
      ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
      // keep the lettering the right way up whichever way the building faces
      if (Math.cos(B.a) < 0) ctx.rotate(Math.PI);
      ctx.fillText('LSP', 0, 0.4);
      ctx.textBaseline = 'alphabetic';
      ctx.restore();
    }
    const D = S.stand;
    if (D) {
      ctx.save(); ctx.translate(D.x, D.y); ctx.rotate(D.a);
      const x0 = -D.w / 2, y0 = -D.h / 2;
      ctx.fillStyle = C.building; ctx.fillRect(x0, y0, D.w, D.h);
      for (let r = 0; r < 4; r++) {
        ctx.fillStyle = r % 2 ? C.signal : C.blue;
        ctx.fillRect(x0 + 1.5, y0 + 1 + r * 2.2, D.w - 3, 1);
      }
      ctx.fillStyle = C.roof; ctx.fillRect(x0, y0 + D.h - 1.2, D.w, 1.2);
      ctx.restore();
    }
  }
  // trees: a canopy and a lighter crown, with a shadow by day
  function drawTrees(seen) {
    const vis = S.trees.filter(([x, y]) => seen(x, y));
    const blob = (dx, dy, k) => {
      ctx.beginPath();
      for (const [x, y, r] of vis) { ctx.moveTo(x + dx * r + r * k, y + dy * r); ctx.arc(x + dx * r, y + dy * r, r * k, 0, 7); }
    };
    if (!C.night) { blob(0.25, 0.3, 1); ctx.fillStyle = C.shade; ctx.fill(); }
    blob(0, 0, 1); ctx.fillStyle = C.tree; ctx.fill();
    blob(-0.25, -0.25, 0.55); ctx.fillStyle = C.treeHi; ctx.fill();
  }

  // the tiles change a few times a second at most
  function hud(dt) {
    if ((hudT -= dt) > 0) return;
    hudT = 0.1;
    tLap.textContent = lap == null ? '-' : driftTime(lap);
    tDrift.textContent = combo > 1 ? Math.round(combo).toLocaleString() : '-';
  }

  function frame(t) {
    raf = 0;
    if (!cv.isConnected) { stop(); return; }
    const dt = Math.min(0.05, (t - prevT) / 1000 || 0);
    prevT = t;
    if (running) { update(dt); hud(dt); }
    draw();
    if (running) raf = requestAnimationFrame(frame);
  }
  function play() {
    if (running) return;
    running = started = true;
    prevT = performance.now();
    if (!raf) raf = requestAnimationFrame(frame);
  }
  function pause() {
    if (!running) return;
    running = false;
    for (const k in keys) keys[k] = false;
    if (!raf) raf = requestAnimationFrame(frame);
  }

  const MAP = { ArrowUp: 'up', KeyW: 'up', ArrowDown: 'down', KeyS: 'down',
    ArrowLeft: 'left', KeyA: 'left', ArrowRight: 'right', KeyD: 'right', Space: 'hb' };
  function onKey(e) {
    if (e.ctrlKey || e.altKey || e.metaKey) return;
    if (e.target.closest && e.target.closest('input, textarea, select')) return;
    const down = e.type === 'keydown';
    const k = MAP[e.code];
    if (k) {
      e.preventDefault();
      keys[k] = down;
      if (down && k !== 'hb') play();
      return;
    }
    if (!down) return;
    if (e.code === 'KeyP' || e.code === 'Escape') { e.preventDefault(); running ? pause() : started && play(); }
    if (e.code === 'KeyR') { e.preventDefault(); reset(); if (!running) draw(); }
  }
  addEventListener('keydown', onKey);
  addEventListener('keyup', onKey);
  addEventListener('blur', pause);
  const ro = new ResizeObserver(() => { size(); if (!running) draw(); });
  ro.observe(wrap);
  // the Black / Ivory switch: a running game picks it up next frame, a
  // paused one has no next frame, so repaint it here
  const mo = new MutationObserver(() => { if (!running) draw(); });
  mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });

  // what the converter is doing, so you know when to go back
  async function poll() {
    if (!cv.isConnected) return;
    const s = await api('convert/status');
    if (!s || !s.ok) { status.textContent = ''; return; }
    const what = esc(String(s.car || '').replace(/^track:/, ''));
    status.innerHTML = s.state === 'running'
      ? 'Converting <span class="mono">' + what + '</span> · ' + Math.round(s.secs || 0) + ' s'
      : s.state === 'done' && what ? '<b style="color:var(--ok)">Ready.</b> <span class="mono">' + what + '</span> is converted. Open Convert for the result.'
      : s.state === 'error' && what ? '<b style="color:var(--red)">The last conversion didn\'t work.</b> Open Convert for the details.'
      : 'No conversion running.';
  }
  const pollT = setInterval(poll, 2000);
  poll();

  function stop() {
    running = false;
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
    clearInterval(pollT);
    ro.disconnect();
    mo.disconnect();
    removeEventListener('keydown', onKey);
    removeEventListener('keyup', onKey);
    removeEventListener('blur', pause);
    if (driftStop === stop) driftStop = null;
  }
  driftStop = stop;
  colours();
  reset();
  size();
  draw();
}
