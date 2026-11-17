// Bref — a 15-second motion reel, drawn on a canvas.
//
// Every frame is a pure function of time: render(t) draws the whole picture
// for the instant t, so the reel can be previewed live in a browser or
// rendered frame by frame (with motion blur) by render.mjs.
//
// The logo is the official dev.andrea.Bref.svg, taken apart into its pieces:
// the tile, the paper card, the moss card and its shadow, and the ember link.
'use strict';

const W = 1920, H = 1080, FPS = 60, DUR = 15;
const COL = { deep: '#0d140e', ink: '#18221a', moss: '#3a4b36', paper: '#ece7dc', ember: '#e8873f' };
const D2R = Math.PI / 180;
const SANS = 'PlexSans', MONO = 'PlexMono', SERIF = 'PlexSerif';

// The 13 themes Bref ships: [background, text, faint, accent] (src/ui/theme.rs).
const THEMES = [
  ['Bref Dark', '#1b1a19', '#e4e0da', '#77716a', '#e0926b'],
  ['Bref Light', '#fbfaf8', '#26231f', '#a8a29a', '#b4532a'],
  ['Dracula', '#282a36', '#f8f8f2', '#6272a4', '#bd93f9'],
  ['One Dark', '#282c34', '#abb2bf', '#5c6370', '#61afef'],
  ['Gruvbox Dark', '#282828', '#ebdbb2', '#928374', '#fabd2f'],
  ['Nord', '#2e3440', '#d8dee9', '#616e88', '#88c0d0'],
  ['Catppuccin Mocha', '#1e1e2e', '#cdd6f4', '#6c7086', '#cba6f7'],
  ['Tokyo Night', '#1a1b26', '#c0caf5', '#565f89', '#7aa2f7'],
  ['Rosé Pine', '#191724', '#e0def4', '#6e6a86', '#ebbcba'],
  ['Solarized Dark', '#002b36', '#93a1a1', '#586e75', '#268bd2'],
  ['Solarized Light', '#fdf6e3', '#586e75', '#93a1a1', '#268bd2'],
  ['Catppuccin Latte', '#eff1f5', '#4c4f69', '#9ca0b0', '#8839ef'],
  ['Gruvbox Light', '#fbf1c7', '#3c3836', '#928374', '#b57614'],
];

// ───────────────────────────── math ─────────────────────────────

const clamp = (x, a = 0, b = 1) => (x < a ? a : x > b ? b : x);
const lerp = (a, b, t) => a + (b - a) * t;
const seg = (t, a, b) => clamp((t - a) / (b - a));
const E = {
  outCubic: x => 1 - (1 - x) ** 3,
  inCubic: x => x ** 3,
  inOutCubic: x => (x < 0.5 ? 4 * x ** 3 : 1 - (-2 * x + 2) ** 3 / 2),
  inQuart: x => x ** 4,
  outQuart: x => 1 - (1 - x) ** 4,
  outQuint: x => 1 - (1 - x) ** 5,
  outExpo: x => (x >= 1 ? 1 : 1 - 2 ** (-10 * x)),
  inExpo: x => (x <= 0 ? 0 : 2 ** (10 * x - 10)),
  inOutExpo: x => (x <= 0 ? 0 : x >= 1 ? 1 : x < 0.5 ? 2 ** (20 * x - 10) / 2 : (2 - 2 ** (-20 * x + 10)) / 2),
  outBack: (x, s = 1.7) => 1 + (s + 1) * (x - 1) ** 3 + s * (x - 1) ** 2,
  inBack: (x, s = 1.7) => (s + 1) * x ** 3 - s * x ** 2,
};
// A damped spring from 0 to 1, released at t = 0.
function spring(t, f = 2, z = 0.4) {
  if (t <= 0) return 0;
  const w = 2 * Math.PI * f, wd = w * Math.sqrt(1 - z * z);
  return 1 - Math.exp(-z * w * t) * (Math.cos(wd * t) + (z * w / wd) * Math.sin(wd * t));
}
function mulberry32(a) {
  return function () {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
function hex(h) { const n = parseInt(h.slice(1), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; }
function rgba(h, a = 1) { const [r, g, b] = hex(h); return `rgba(${r},${g},${b},${clamp(a)})`; }
function mix(h1, h2, t, a = 1) {
  const p = hex(h1), q = hex(h2);
  return `rgba(${Math.round(lerp(p[0], q[0], t))},${Math.round(lerp(p[1], q[1], t))},${Math.round(lerp(p[2], q[2], t))},${a})`;
}
function shake(t, t0, amp, dur = 0.6) {
  if (t < t0 || t > t0 + dur) return [0, 0];
  const k = t - t0, d = Math.exp(-k * 8) * amp;
  return [d * (Math.sin(k * 93 + 1.3) * 0.8 + Math.sin(k * 151) * 0.35), d * (Math.cos(k * 77 + 0.4) * 0.8 + Math.sin(k * 131 + 2) * 0.3)];
}
// Times at which each character of a string is typed, a little uneven like a hand.
function typing(str, start, cps, seed) {
  const r = mulberry32(seed), out = [];
  let tt = start;
  for (let i = 0; i < str.length; i++) {
    out.push(tt);
    tt += (1 / cps) * lerp(0.6, 1.4, r()) * (str[i] === ' ' ? 1.25 : 1);
  }
  return out;
}
let FRAME_T = null;
const ft = t => (FRAME_T === null ? t : FRAME_T);
const typed = (times, t) => { let n = 0; while (n < times.length && times[n] <= t) n++; return n; };

// ─────────────────────────── drawing kit ───────────────────────────

function font(fam, size, weight = 400) { return `${weight} ${size}px ${fam}`; }
function txt(ctx, s, x, y, o = {}) {
  ctx.font = o.font; ctx.fillStyle = o.color || COL.paper;
  ctx.textAlign = o.align || 'left'; ctx.textBaseline = 'alphabetic';
  ctx.letterSpacing = (o.ls || 0) + 'px';
  ctx.fillText(s, x, y);
  ctx.letterSpacing = '0px';
}
function measure(ctx, s, f, ls = 0) {
  ctx.font = f; ctx.letterSpacing = ls + 'px';
  const w = ctx.measureText(s).width;
  ctx.letterSpacing = '0px';
  return w;
}
function rrect(ctx, x, y, w, h, r) { ctx.beginPath(); ctx.roundRect(x, y, w, h, Math.max(0, Math.min(r, w / 2, h / 2))); }
function line(ctx, x1, y1, x2, y2) { ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke(); }
function circle(ctx, x, y, r, fill) { if (r <= 0) return; ctx.beginPath(); ctx.arc(x, y, r, 0, Math.PI * 2); ctx.fillStyle = fill; ctx.fill(); }
function glow(ctx, x, y, r, color, a) {
  if (a <= 0) return;
  const g = ctx.createRadialGradient(x, y, 0, x, y, r);
  g.addColorStop(0, rgba(color, a)); g.addColorStop(1, rgba(color, 0));
  ctx.fillStyle = g; ctx.fillRect(x - r, y - r, r * 2, r * 2);
}
function partialRect(ctx, x, y, w, h, p) {
  if (p <= 0) return;
  const per = 2 * (w + h); let left = per * p;
  ctx.beginPath(); ctx.moveTo(x, y);
  const legs = [[w, 0], [0, h], [-w, 0], [0, -h]];
  let cx = x, cy = y;
  for (const [dx, dy] of legs) {
    const len = Math.abs(dx + dy), k = Math.min(1, left / len);
    cx += dx * k; cy += dy * k; ctx.lineTo(cx, cy);
    left -= len; if (left <= 0) break;
  }
  ctx.stroke();
}
function ring(ctx, x, y, t, t0, r0, r1, dur, color, a0, w0) {
  const p = seg(t, t0, t0 + dur);
  if (p <= 0 || p >= 1) return;
  const e = E.outExpo(p);
  ctx.beginPath(); ctx.arc(x, y, lerp(r0, r1, e), 0, Math.PI * 2);
  ctx.strokeStyle = rgba(color, a0 * (1 - p) ** 1.5); ctx.lineWidth = w0 * (1 - e) + 0.6; ctx.stroke();
}
function sparks(ctx, x, y, t, t0, n, seed, r0, r1, dur, scale = 1) {
  const r = mulberry32(seed);
  ctx.lineCap = 'round';
  for (let i = 0; i < n; i++) {
    const ang = r() * Math.PI * 2, len = lerp(0.35, 1, r()), dl = r() * 0.07, d2 = lerp(0.6, 1, r()), warm = r() < 0.72;
    const p = seg(t, t0 + dl, t0 + dl + dur * d2);
    if (p <= 0 || p >= 1) continue;
    const e = E.outExpo(p), d = lerp(r0, r0 + (r1 - r0) * len, e), tail = lerp(90, 0, e) * len * scale;
    const c = Math.cos(ang), s = Math.sin(ang);
    ctx.strokeStyle = rgba(warm ? COL.ember : COL.paper, (1 - p) * 0.9);
    ctx.lineWidth = (4 * (1 - p) + 1) * scale;
    line(ctx, x + c * (d - tail), y + s * (d - tail), x + c * d, y + s * d);
  }
}
const DUST = (() => {
  const r = mulberry32(99);
  return Array.from({ length: 80 }, () => ({ x: r() * W, y: r() * H, z: 0.25 + r() * 0.75, ph: r() * 6.283, sp: 8 + r() * 26 }));
})();
function dust(ctx, t, a) {
  if (a <= 0) return;
  for (const p of DUST) {
    const y = (((p.y - t * p.sp * p.z) % (H + 40)) + H + 40) % (H + 40) - 20;
    const x = p.x + Math.sin(t * 0.5 + p.ph) * 22 * p.z;
    circle(ctx, x, y, 0.8 + p.z * 2, rgba(COL.paper, a * (0.08 + 0.22 * p.z) * (0.6 + 0.4 * Math.sin(t * 2.2 + p.ph))));
  }
}

// ──────────────────────── the logo, in pieces ────────────────────────
// All in the SVG's own 128-unit space: the caller sets the transform.

function drawTile(ctx, { a = 1, s = 1, r = 28 } = {}) {
  if (a <= 0 || s <= 0) return;
  ctx.save(); ctx.globalAlpha *= a;
  ctx.translate(64, 64); ctx.scale(s, s);
  rrect(ctx, -64, -64, 128, 128, r); ctx.fillStyle = COL.ink; ctx.fill();
  ctx.lineWidth = 0.5 / s; ctx.strokeStyle = rgba(COL.paper, 0.07); ctx.stroke();
  ctx.restore();
}
function drawCardA(ctx, { dx = 0, dy = 0, rot = 0, s = 1, a = 1, ln = 1 } = {}) {
  if (a <= 0) return;
  ctx.save(); ctx.globalAlpha *= a;
  ctx.translate(40 + dx, 48 + dy); ctx.rotate((-7 + rot) * D2R); ctx.scale(s, s); ctx.translate(-40, -48);
  rrect(ctx, 15, 17, 50, 62, 11); ctx.fillStyle = COL.paper; ctx.fill();
  if (ln > 0) { ctx.strokeStyle = COL.ink; ctx.lineWidth = 7; ctx.lineCap = 'round'; line(ctx, 27, 33, 27 + 20 * ln, 33); }
  ctx.restore();
}
function drawCardB(ctx, { dx = 0, dy = 0, rot = 0, s = 1, a = 1, ln = 1 } = {}) {
  if (a <= 0) return;
  ctx.save(); ctx.globalAlpha *= a;
  ctx.translate(88 + dx, 80 + dy); ctx.rotate((7 + rot) * D2R); ctx.scale(s, s); ctx.translate(-88, -80);
  rrect(ctx, 66, 52, 50, 62, 11); ctx.fillStyle = '#0d140e'; ctx.fill();
  rrect(ctx, 63, 49, 50, 62, 11); ctx.fillStyle = COL.moss; ctx.fill();
  if (ln > 0) { ctx.strokeStyle = COL.paper; ctx.lineWidth = 7; ctx.lineCap = 'round'; line(ctx, 75, 96, 75 + 20 * ln, 96); }
  ctx.restore();
}
function drawLink(ctx, { p1 = [40, 57], p2 = [88, 72], r1 = 9.5, r2 = 9.5, a = 1, prog = 1 } = {}) {
  if (a <= 0) return;
  ctx.save(); ctx.globalAlpha *= a;
  if (prog > 0) {
    ctx.strokeStyle = COL.ember; ctx.lineWidth = 6.5; ctx.lineCap = 'round';
    line(ctx, p1[0], p1[1], lerp(p1[0], p2[0], prog), lerp(p1[1], p2[1], prog));
  }
  circle(ctx, p1[0], p1[1], r1, COL.ember);
  circle(ctx, p2[0], p2[1], r2, COL.ember);
  ctx.restore();
}
const CARD_CLIP = (() => {
  const p = new Path2D();
  const a = new Path2D(); a.roundRect(15, 17, 50, 62, 11);
  p.addPath(a, new DOMMatrix().translate(40, 48).rotate(-7).translate(-40, -48));
  const b = new Path2D(); b.roundRect(63, 49, 50, 62, 11);
  p.addPath(b, new DOMMatrix().translate(88, 80).rotate(7).translate(-88, -80));
  return p;
})();
// A band of light across the two cards.
function sheen(ctx, p) {
  if (p <= 0 || p >= 1) return;
  ctx.save(); ctx.clip(CARD_CLIP);
  ctx.translate(64, 64); ctx.rotate(24 * D2R);
  const x = lerp(-120, 120, E.inOutCubic(p));
  const g = ctx.createLinearGradient(x - 26, 0, x + 26, 0);
  g.addColorStop(0, 'rgba(255,255,255,0)'); g.addColorStop(0.5, 'rgba(255,255,255,0.33)'); g.addColorStop(1, 'rgba(255,255,255,0)');
  ctx.fillStyle = g; ctx.fillRect(x - 26, -160, 52, 320);
  ctx.restore();
}
// Camera onto the logo: `depth` > 1 is closer to the lens, so it travels faster when the camera dives.
function logoCam(ctx, cam, S, cx, cy, depth = 1) {
  const s = S * Math.pow(cam.s, depth);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.translate(cx + cam.sx, cy + cam.sy); ctx.rotate(cam.rot); ctx.scale(s, s); ctx.translate(-cam.fx, -cam.fy);
}

// ───────────────────────── 01 · assemble (0 → 3.47) ─────────────────────────

const S1 = 4.6; // logo scale in the opening: 128 units → 589 px
const P1 = (x, y) => [960 + (x - 64) * S1, 540 + (y - 64) * S1];

function guidesBack(ctx, t, a) {
  if (a <= 0) return;
  ctx.save();
  ctx.lineWidth = 1;
  ctx.strokeStyle = rgba(COL.paper, 0.15 * a);
  const h = E.outExpo(seg(t, 0.08, 0.9)), v = E.outExpo(seg(t, 0.16, 0.95));
  line(ctx, 960 - 960 * h, 540, 960 + 960 * h, 540);
  line(ctx, 960, 540 - 540 * v, 960, 540 + 540 * v);
  const [x0, y0] = P1(0, 0), side = 128 * S1;
  ctx.strokeStyle = rgba(COL.paper, 0.07 * a);
  for (let i = 1; i < 8; i++) {
    const p = E.outExpo(seg(t, 0.3 + i * 0.035, 0.95 + i * 0.035));
    line(ctx, x0 + i * 16 * S1, y0, x0 + i * 16 * S1, y0 + side * p);
    line(ctx, x0, y0 + i * 16 * S1, x0 + side * p, y0 + i * 16 * S1);
  }
  ctx.strokeStyle = rgba(COL.paper, 0.22 * a);
  partialRect(ctx, x0, y0, side, side, E.inOutCubic(seg(t, 0.2, 1.15)));
  ctx.strokeStyle = rgba(COL.paper, 0.1 * a);
  const c = E.inOutCubic(seg(t, 0.3, 1.35));
  if (c > 0) { ctx.beginPath(); ctx.arc(960, 540, 64 * S1 * Math.SQRT2, -Math.PI / 2, -Math.PI / 2 + c * Math.PI * 2); ctx.stroke(); }
  // corner ticks of the tile
  ctx.strokeStyle = rgba(COL.paper, 0.35 * a);
  const tk = 14 * E.outBack(seg(t, 0.9, 1.2));
  for (const [cx, cy] of [[x0, y0], [x0 + side, y0], [x0, y0 + side], [x0 + side, y0 + side]]) {
    line(ctx, cx - tk, cy, cx + tk, cy); line(ctx, cx, cy - tk, cx, cy + tk);
  }
  ctx.restore();
}
function guidesFront(ctx, t, a) {
  if (a <= 0) return;
  ctx.save();
  const f = font(MONO, 15, 400);
  // coordinates of the two link ends
  const la = seg(t, 0.85, 1.15) * a;
  if (la > 0) {
    for (const [x, y, dx, dy] of [[40, 57, -1, -1], [88, 72, 1, 1]]) {
      const [px, py] = P1(x, y);
      const ex = px + dx * 150, ey = py + dy * 120;
      ctx.strokeStyle = rgba(COL.paper, 0.35 * la); ctx.lineWidth = 1;
      line(ctx, px + dx * 50, py + dy * 40, ex, ey); line(ctx, ex, ey, ex + dx * 60, ey);
      txt(ctx, `x ${x}  y ${y}`, ex + dx * 70, ey + 5, { font: f, color: rgba(COL.paper, 0.6 * la), align: dx < 0 ? 'right' : 'left', ls: 1 });
    }
  }
  // the two angles
  const aa = seg(t, 1.55, 1.85) * a;
  if (aa > 0) {
    for (const [cx, cy, dir, label] of [[40, 48, -1, '−7°'], [88, 80, 1, '+7°']]) {
      const [px, py] = P1(cx, cy);
      const base = dir < 0 ? Math.PI : 0, turn = 7 * D2R; // -7 on the left, +7 on the right: both point downward
      const L = 300 * E.outExpo(seg(t, 1.55, 2.0));
      ctx.lineWidth = 1;
      ctx.setLineDash([6, 6]); ctx.strokeStyle = rgba(COL.paper, 0.3 * aa);
      line(ctx, px, py, px + Math.cos(base) * L, py);
      ctx.setLineDash([]); ctx.strokeStyle = rgba(COL.ember, 0.8 * aa);
      const ang = dir < 0 ? Math.PI - turn : turn;
      line(ctx, px, py, px + Math.cos(ang) * L, py + Math.sin(ang) * L);
      ctx.beginPath(); ctx.arc(px, py, L * 0.82, Math.min(base, ang), Math.max(base, ang)); ctx.stroke();
      txt(ctx, label, px + Math.cos(base) * (L + 18), py + 30, { font: font(MONO, 17, 700), color: rgba(COL.ember, aa), align: dir < 0 ? 'right' : 'left' });
    }
  }
  ctx.restore();
}

function sceneOpen(ctx, t) {
  ctx.fillStyle = COL.deep; ctx.fillRect(0, 0, W, H);
  const dv = seg(t, 2.7, 3.47);
  const ant = Math.sin(Math.PI * seg(t, 2.5, 2.9)) * 0.045; // a breath back before the dive
  const breathe = 1 + 0.03 * E.inOutCubic(seg(t, 1.95, 2.75));
  const zs = Math.exp(Math.log(34) * E.inQuart(dv));
  const ce = E.inOutCubic(dv);
  const [sx, sy] = shake(t, 1.9, 18, 0.7);
  const cam = { s: zs * breathe * (1 - ant), rot: 7 * D2R * ce, fx: lerp(64, 40, ce), fy: lerp(64, 48, ce), sx, sy };

  glow(ctx, 960 + sx, 540 + sy, 720, COL.ember, 0.11 * seg(t, 0.05, 0.9) * (1 - dv));
  glow(ctx, 960 + sx, 540 + sy, 380, COL.ember, 0.18 * (1 - seg(t, 1.9, 2.4)) * seg(t, 1.88, 1.92));
  dust(ctx, t, 0.8 * (1 - dv));
  const gA = seg(t, 0.02, 0.3) * (1 - seg(t, 2.0, 2.45));
  ctx.save(); ctx.translate(sx, sy); guidesBack(ctx, t, gA); ctx.restore();

  // the dot that becomes the link
  const pop = E.outBack(seg(t, 0.04, 0.4), 2.4);
  const sp = spring(t - 0.44, 1.6, 0.42);
  const squash = 1 - 0.18 * Math.sin(Math.PI * seg(t, 0.3, 0.46));
  const c = [64, 64.5];
  const p1 = [lerp(c[0], 40, sp), lerp(c[1], 57, sp)], p2 = [lerp(c[0], 88, sp), lerp(c[1], 72, sp)];
  const r = 9.5 * pop * lerp(1.35, 1, seg(t, 0.44, 0.8)) * squash;

  // ripple off the first dot
  ring(ctx, 960 + sx, 540 + sy, t, 0.1, 30, 420, 0.9, COL.ember, 0.55, 6);
  ring(ctx, 960 + sx, 540 + sy, t, 0.22, 20, 300, 0.9, COL.paper, 0.25, 2);

  // tile
  const tp = seg(t, 1.6, 2.1);
  logoCam(ctx, cam, S1, 960, 540, 0.85);
  drawTile(ctx, { a: clamp(tp * 5), s: lerp(0.55, 1, E.outBack(tp, 1.5)), r: lerp(64, 28, E.outCubic(tp)) });

  // paper card
  const pa = spring(t - 0.92, 1.25, 0.5), ra = spring(t - 0.92, 1.0, 0.4);
  logoCam(ctx, cam, S1, 960, 540, 1);
  if (t > 0.92) drawCardA(ctx, { dx: lerp(-215, 0, pa), dy: lerp(-150, 0, pa), rot: lerp(-160, 0, ra), s: lerp(1.5, 1, pa), ln: E.outExpo(seg(t, 1.55, 1.95)) });
  // moss card
  const pb = spring(t - 1.06, 1.25, 0.5), rb = spring(t - 1.06, 1.0, 0.4);
  logoCam(ctx, cam, S1, 960, 540, 1.35);
  if (t > 1.06) drawCardB(ctx, { dx: lerp(215, 0, pb), dy: lerp(150, 0, pb), rot: lerp(140, 0, rb), s: lerp(1.5, 1, pb), ln: E.outExpo(seg(t, 1.68, 2.08)) });
  logoCam(ctx, cam, S1, 960, 540, 1);
  sheen(ctx, seg(t, 2.0, 2.55));

  // link, on top, as in the SVG
  logoCam(ctx, cam, S1, 960, 540, 1.7);
  drawLink(ctx, { p1, p2, r1: r, r2: r, a: 1 - seg(t, 2.95, 3.3) });
  ctx.setTransform(1, 0, 0, 1, 0, 0);

  // impact when the tile lands
  ring(ctx, 960 + sx, 540 + sy, t, 1.9, 300, 1150, 0.85, COL.ember, 0.55, 16);
  ring(ctx, 960 + sx, 540 + sy, t, 1.97, 300, 900, 0.8, COL.paper, 0.3, 3);
  sparks(ctx, 960 + sx, 540 + sy, t, 1.9, 46, 7, 330, 820, 0.75);
  ctx.save(); ctx.translate(sx, sy); guidesFront(ctx, t, gA); ctx.restore();
}

// ───────────────────────── 02 · write (3.47 → 7) ─────────────────────────

const ED = { x0: 380, y1: 352, y2: 470, top: 552, rowH: 66 };
const L1 = '# Notes that open', L2 = 'before you **blink**.', LF = '=sum(above)';
const T1 = typing(L1, 3.66, 27, 3);
const T2 = typing(L2, T1[T1.length - 1] + 0.24, 33, 4);
const TB = T2[19];                        // the second closing star: "blink" turns bold
const TT = T2[T2.length - 1] + 0.16;      // the table arrives
const TF = typing(LF, TT + 0.55, 34, 5);  // the formula is typed
const TR = TF[TF.length - 1] + 0.12;      // ...and resolves
const KEYS = [...T1, ...T2, ...TF];
const lastKey = t => { let k = -1; for (const x of KEYS) if (x <= t) k = Math.max(k, x); return k; };

function caretOn(t) { const k = lastKey(t); if (k < 0) return true; const idle = t - k; return idle < 0.45 || ((idle - 0.45) % 0.9) < 0.5; }
function caret(ctx, x, base, size, t) {
  if (!caretOn(t)) return;
  ctx.fillStyle = COL.ember;
  ctx.fillRect(x + 4, base - size * 0.8, Math.max(4, size * 0.06), size * 0.98);
}

function drawEditor(ctx, t) {
  ctx.save();
  const dz = 1.06 * (1 + 0.04 * E.outCubic(seg(t, 3.4, 7.0)));
  ctx.translate(960, 540); ctx.scale(dz, dz); ctx.translate(-960, -495);
  const x0 = ED.x0, ink = COL.ink;

  // header: path and save state
  const hb = E.outExpo(seg(t, 3.5, 4.0));
  ctx.globalAlpha = hb;
  const hy = 128 - (1 - hb) * 30;
  const fpath = font(MONO, 24, 400);
  txt(ctx, '~/Notes/', x0, hy, { font: fpath, color: rgba(ink, 0.4), ls: 1 });
  txt(ctx, 'Ideas.md', x0 + measure(ctx, '~/Notes/', fpath, 1), hy, { font: font(MONO, 24, 700), color: rgba(ink, 0.85), ls: 1 });
  const k = lastKey(t), editing = k > 0 && t - k < 0.4;
  txt(ctx, editing ? 'editing' : 'saved', 1620, hy, { font: fpath, color: rgba(ink, 0.5), align: 'right', ls: 1 });
  const sw = measure(ctx, editing ? 'editing' : 'saved', fpath, 1);
  ctx.beginPath(); ctx.arc(1620 - sw - 22, hy - 8, 7, 0, Math.PI * 2);
  if (editing) { ctx.strokeStyle = COL.ember; ctx.lineWidth = 2.5; ctx.stroke(); } else { ctx.fillStyle = COL.ember; ctx.fill(); }
  ctx.strokeStyle = rgba(ink, 0.12); ctx.lineWidth = 2;
  line(ctx, x0 - 90, 168, x0 - 90 + (1620 - x0 + 90) * E.outExpo(seg(t, 3.55, 4.2)), 168);
  ctx.globalAlpha = 1;

  // gutter
  const fnum = font(MONO, 22, 400);
  const nums = [[1, ED.y1, T1[0]], [2, ED.y2, T2[0]]];
  for (let r = 0; r < 5; r++) nums.push([3 + r, ED.top + r * ED.rowH + ED.rowH * 0.64, TT + 0.12 + r * 0.06]);
  for (const [n, y, at] of nums) if (t >= at) txt(ctx, String(n), x0 - 64, y, { font: fnum, color: rgba(ink, 0.22 * seg(t, at, at + 0.15)), align: 'right' });

  let cx = x0, cy = ED.y1, cs = 60;

  // line 1: "# " turns the line into a heading as you type
  const n1 = typed(T1, t), tH = T1[1];
  const styled = n1 >= 2;
  const hs = styled ? lerp(60, 132, spring(t - tH, 2.1, 0.42)) : 60;
  const f1 = font(SANS, hs, styled ? 700 : 400), ls1 = styled ? -hs * 0.025 : 0;
  const mk = L1.slice(0, Math.min(n1, 2));
  txt(ctx, mk, x0, ED.y1, { font: f1, color: rgba(ink, styled ? 0.26 : 1), ls: ls1 });
  let x = x0 + measure(ctx, mk, f1, ls1);
  const rest = L1.slice(2, Math.max(2, n1));
  txt(ctx, rest, x, ED.y1, { font: f1, color: ink, ls: ls1 });
  x += measure(ctx, rest, f1, ls1);
  if (t < T2[0] - 0.05) { cx = x; cy = ED.y1; cs = hs; }

  // line 2: "**blink**" turns bold once closed, markers stay, dimmed
  const n2 = typed(T2, t);
  if (n2 > 0 || t >= T2[0] - 0.05) {
    const bold = n2 >= 20, s2 = 62;
    const parts = [[0, 11, 'n'], [11, 13, 'm'], [13, 18, 'b'], [18, 20, 'm'], [20, 21, 'n']];
    let xx = x0;
    for (const [a, b, kind] of parts) {
      const s = L2.slice(a, Math.min(b, n2));
      if (!s) break;
      const f = font(SANS, s2, kind === 'b' && bold ? 700 : 400);
      const w = measure(ctx, s, f);
      if (kind === 'b' && bold) {
        const e = E.outExpo(seg(t, TB + 0.04, TB + 0.42));
        ctx.fillStyle = rgba(COL.ember, 0.42);
        const hx = xx - 6, hw = (w + 12) * e;
        ctx.beginPath(); ctx.moveTo(hx + 4, ED.y2 - s2 * 0.6); ctx.lineTo(hx + hw, ED.y2 - s2 * 0.64); ctx.lineTo(hx + hw - 4, ED.y2 + s2 * 0.14); ctx.lineTo(hx, ED.y2 + s2 * 0.16); ctx.closePath(); ctx.fill();
        const pz = lerp(1.22, 1, spring(t - TB, 2.6, 0.32));
        ctx.save(); ctx.translate(xx + w / 2, ED.y2 - s2 * 0.35); ctx.scale(pz, pz); ctx.translate(-(xx + w / 2), -(ED.y2 - s2 * 0.35));
        txt(ctx, s, xx, ED.y2, { font: f, color: ink });
        ctx.restore();
      } else {
        txt(ctx, s, xx, ED.y2, { font: f, color: kind === 'm' ? rgba(ink, 0.3) : ink });
      }
      xx += w;
    }
    if (t >= T2[0] - 0.05 && t < TF[0] - 0.05) { cx = xx; cy = ED.y2; cs = s2; }
  }

  // the table, and a formula that adds up its column
  if (t >= TT) {
    const top = ED.top, rh = ED.rowH, c1 = 480, c2 = 300, tw = c1 + c2;
    const rows = [['Thing', 'Count'], ['Ideas', '12'], ['Links', '21'], ['Coffee', '9'], ['Total', '']];
    ctx.fillStyle = rgba(ink, 0.05 * seg(t, TT, TT + 0.3));
    ctx.fillRect(x0, top, tw * E.outExpo(seg(t, TT, TT + 0.45)), rh);
    for (let i = 0; i <= 5; i++) {
      const p = E.outExpo(seg(t, TT + i * 0.035, TT + i * 0.035 + 0.5));
      ctx.strokeStyle = rgba(ink, i === 1 || i === 4 ? 0.5 : 0.16); ctx.lineWidth = i === 1 || i === 4 ? 2.5 : 1.5;
      if (p > 0) line(ctx, x0, top + i * rh, x0 + tw * p, top + i * rh);
    }
    [0, c1, tw].forEach((vx, j) => {
      const p = E.outExpo(seg(t, TT + 0.06 + j * 0.05, TT + 0.06 + j * 0.05 + 0.5));
      ctx.strokeStyle = rgba(ink, 0.16); ctx.lineWidth = 1.5;
      if (p > 0) line(ctx, x0 + vx, top, x0 + vx, top + 5 * rh * p);
    });
    rows.forEach((row, r) => {
      const e = E.outExpo(seg(t, TT + 0.1 + r * 0.055, TT + 0.1 + r * 0.055 + 0.45));
      if (e <= 0) return;
      const by = top + r * rh + rh * 0.64 + (1 - e) * 18;
      ctx.globalAlpha = e;
      txt(ctx, row[0], x0 + 28, by, { font: font(SANS, 36, r === 0 || r === 4 ? 700 : 400), color: r === 0 ? rgba(ink, 0.6) : ink });
      if (row[1]) txt(ctx, row[1], x0 + tw - 28, by, { font: font(r === 0 ? SANS : MONO, 36, r === 0 ? 700 : 400), color: r === 0 ? rgba(ink, 0.6) : ink, align: 'right' });
      ctx.globalAlpha = 1;
    });
    // formula cell
    const fy = top + 4 * rh + rh * 0.64, fx = x0 + c1 + 26;
    const flash = t >= TR ? 0.06 + 0.3 * (1 - E.outCubic(seg(t, TR, TR + 0.9))) : 0;
    if (flash > 0) { ctx.fillStyle = rgba(COL.ember, flash); ctx.fillRect(x0 + c1 + 1, top + 4 * rh + 1.5, c2 - 2, rh - 3); }
    const nF = typed(TF, t), ff = font(MONO, 34, 400);
    let fxx = fx;
    for (let i = 0; i < nF; i++) {
      const ch = LF[i], w = measure(ctx, ch, ff);
      const e = E.outExpo(seg(t, TR + i * 0.014, TR + i * 0.014 + 0.4));
      if (e < 1) {
        ctx.save(); ctx.globalAlpha = 1 - e;
        ctx.translate(fxx + w / 2, fy - 12 - 60 * e); ctx.rotate((i % 2 ? 1 : -1) * 0.5 * e);
        txt(ctx, ch, -w / 2, 12, { font: ff, color: COL.ember });
        ctx.restore();
      }
      fxx += w;
    }
    if (t >= TF[0] - 0.05 && t < TR) { cx = fxx - 2; cy = fy; cs = 40; }
    if (t >= TR) {
      // odometer
      const fd = font(MONO, 36, 700), dw = measure(ctx, '0', fd), right = x0 + tw - 28;
      ctx.save(); ctx.beginPath(); ctx.rect(x0 + c1, top + 4 * rh, c2, rh); ctx.clip();
      ['4', '2'].forEach((d, i) => {
        const e = E.outExpo(seg(t, TR + 0.06 + i * 0.08, TR + 0.06 + i * 0.08 + 0.65));
        const v = Number(d) - 10 * (1 - e), fl = Math.floor(v), fr = v - fl;
        const dx = right - (2 - i) * dw;
        for (const [dig, off] of [[fl, -fr], [fl + 1, 1 - fr]]) {
          txt(ctx, String(((dig % 10) + 10) % 10), dx, fy + off * rh, { font: fd, color: COL.ink });
        }
      });
      ctx.restore();
      cx = right; cy = fy; cs = 40;
    }
  }
  caret(ctx, cx, cy, cs, t);
  ctx.restore();
}

// The editor page, seen as a card: full screen at k = 1, a note among notes when small.
const K_END = 0.11;
function drawPage(ctx, t, cx, cy, k, rot, w, h, rr) {
  ctx.save();
  ctx.translate(cx, cy); ctx.rotate(rot); ctx.scale(k, k);
  if (k < 0.95) { ctx.fillStyle = rgba('#000000', 0.35 * (1 - k)); rrect(ctx, -w / 2 + 3 / k * 4, -h / 2 + 3 / k * 4, w, h, rr); ctx.fill(); }
  rrect(ctx, -w / 2, -h / 2, w, h, rr); ctx.fillStyle = COL.paper; ctx.fill();
  const contentA = seg(k, 0.16, 0.45), miniA = 1 - seg(k, 0.16, 0.3);
  if (contentA > 0) {
    ctx.save(); ctx.clip(); ctx.globalAlpha = contentA; ctx.translate(-960, -540); drawEditor(ctx, t); ctx.restore();
  }
  if (miniA > 0) {
    const u = w / 50, L = -w / 2, T = -h / 2;
    ctx.globalAlpha = miniA; ctx.lineCap = 'round';
    ctx.strokeStyle = COL.ink; ctx.lineWidth = 7 * u; line(ctx, L + 12 * u, T + 16 * u, L + 32 * u, T + 16 * u);
    ctx.strokeStyle = rgba(COL.ink, 0.28); ctx.lineWidth = 3 * u;
    for (const [y, x2] of [[28, 38], [35, 30], [42, 35]]) line(ctx, L + 12 * u, T + y * u, L + x2 * u, T + y * u);
    ctx.globalAlpha = 1;
  }
  ctx.restore();
}

// ───────────────────────── 03 · link (7 → 9.6) ─────────────────────────

const NAMES = ['Ideas', 'Daily 2026-11-17', 'Rust', 'Reading list', 'Kanban', 'Recipes', 'Trips', 'Meetings', 'Inbox', 'Books', 'Projects', 'Garden', 'Music', 'Todo', 'Quotes', 'Drafts'];
const GRAPH = (() => {
  const r = mulberry32(21), N = NAMES.length, nodes = [];
  for (let i = 0; i < N; i++) {
    const yy = 1 - (2 * (i + 0.5)) / N, rad = Math.sqrt(1 - yy * yy), th = i * 2.399963 + 0.6, R = lerp(560, 680, r());
    nodes.push({ x: Math.cos(th) * rad * R * 1.2, y: yy * 330, z: Math.sin(th) * rad * R, sc: lerp(0.72, 1.0, r()), rot: (i % 2 ? 1 : -1) * lerp(4, 10, r()), moss: i % 3 === 1, name: NAMES[i] });
  }
  const order = [...Array(N).keys()];
  for (let i = N - 1; i > 0; i--) { const j = Math.floor(r() * (i + 1)); [order[i], order[j]] = [order[j], order[i]]; }
  order.forEach((n, i) => (nodes[n].order = i));
  const edges = [];
  for (const i of [0, 3, 5, 8, 11, 14]) edges.push([-1, i]);
  for (let i = 0; i < N - 1; i += 2) edges.push([i, i + 1]);
  for (let k = 0; k < 7; k++) { const a = Math.floor(r() * N), b = Math.floor(r() * N); if (a !== b) edges.push([a, b]); }
  return { nodes, edges };
})();

function project(p, ang, tilt, rf) {
  let x = p.x * rf, y = p.y * rf, z = p.z * rf;
  const ca = Math.cos(ang), sa = Math.sin(ang);
  [x, z] = [x * ca + z * sa, -x * sa + z * ca];
  const ct = Math.cos(tilt), st = Math.sin(tilt);
  [y, z] = [y * ct - z * st, y * st + z * ct];
  const ps = 2000 / (2000 + z);
  return { X: 960 + x * ps, Y: 540 + y * ps, ps, z };
}
const fogOf = z => clamp(1 - ((z + 500) / 1300) * 0.78, 0.22, 1);

function drawNode(ctx, X, Y, u, rot, moss, a, dot) {
  ctx.save(); ctx.globalAlpha = a; ctx.translate(X, Y); ctx.rotate(rot * D2R);
  ctx.lineCap = 'round';
  if (moss) {
    rrect(ctx, -22 * u, -28 * u, 50 * u, 62 * u, 11 * u); ctx.fillStyle = '#0d140e'; ctx.fill();
    rrect(ctx, -25 * u, -31 * u, 50 * u, 62 * u, 11 * u); ctx.fillStyle = COL.moss; ctx.fill();
    ctx.strokeStyle = rgba(COL.paper, 0.25); ctx.lineWidth = 3 * u;
    line(ctx, -13 * u, -17 * u, 11 * u, -17 * u); line(ctx, -13 * u, -10 * u, 4 * u, -10 * u);
    ctx.strokeStyle = COL.paper; ctx.lineWidth = 7 * u; line(ctx, -13 * u, 16 * u, 7 * u, 16 * u);
  } else {
    rrect(ctx, -25 * u, -31 * u, 50 * u, 62 * u, 11 * u); ctx.fillStyle = COL.paper; ctx.fill();
    ctx.strokeStyle = COL.ink; ctx.lineWidth = 7 * u; line(ctx, -13 * u, -15 * u, 7 * u, -15 * u);
    ctx.strokeStyle = rgba(COL.ink, 0.25); ctx.lineWidth = 3 * u;
    line(ctx, -13 * u, -2 * u, 13 * u, -2 * u); line(ctx, -13 * u, 6 * u, 5 * u, 6 * u);
  }
  ctx.restore();
  if (dot > 0) circle(ctx, X, Y + 2 * u, 5.5 * u * dot, rgba(COL.ember, a));
}

function sceneLink(ctx, t) {
  ctx.fillStyle = COL.deep; ctx.fillRect(0, 0, W, H);
  const pb = seg(t, 6.95, 7.7), pe = E.inOutExpo(pb);
  const coll = seg(t, 8.98, 9.42);
  let k = Math.exp(lerp(0, Math.log(K_END), pe)) * (1 + 0.25 * E.inQuart(coll));
  const rot = lerp(0, -7, E.inOutCubic(pb)) * D2R;
  const pw = lerp(1920, 1909, pe), ph = lerp(1080, 2364, pe), prr = lerp(0, 200, pe);

  if (t < 7.12) { drawPage(ctx, t, 960, 540, k, rot, pw, ph, prr); return; }

  glow(ctx, 960, 540, 900, COL.ember, 0.07 * seg(t, 7.1, 7.8) * (1 - coll));
  dust(ctx, t, 0.6 * seg(t, 7.1, 7.8));
  const ang = lerp(-1.3, 0, E.outCubic(seg(t, 7.1, 8.5))) + 0.2 * (t - 7.1) + 3.2 * E.inQuart(coll);
  const tilt = 0.3, cf = 1 - E.inBack(coll, 2.2);
  const P = GRAPH.nodes.map(n => {
    const d = 7.22 + n.order * 0.032, ap = seg(t, d, d + 0.6);
    const pr = project(n, ang, tilt, lerp(0.05, 1, E.outExpo(ap)) * cf);
    return { ...pr, n, ap, sc: E.outBack(seg(t, d, d + 0.45), 1.8) * (1 - 0.85 * E.inQuart(coll)) };
  });
  const C = { X: 960, Y: 540, ps: 1, z: 0 };
  const at = i => (i < 0 ? C : P[i]);
  // links
  const linked = new Set();
  ctx.lineCap = 'round';
  GRAPH.edges.forEach(([a, b], i) => {
    const e = E.outExpo(seg(t, 7.55 + i * 0.026, 7.55 + i * 0.026 + 0.45));
    if (e <= 0) return;
    const A = at(a), B = at(b);
    if ((a >= 0 && A.ap <= 0) || B.ap <= 0) return;
    linked.add(a); linked.add(b);
    ctx.strokeStyle = rgba(COL.ember, 0.85 * Math.min(fogOf(A.z), fogOf(B.z)));
    ctx.lineWidth = 7 * (A.ps + B.ps) / 2;
    line(ctx, A.X, A.Y, lerp(A.X, B.X, e), lerp(A.Y, B.Y, e));
  });
  // cards, far to near, the page among them
  const items = P.map((p, i) => ({ ...p, i })).concat([{ ...C, i: -1 }]).sort((a, b) => b.z - a.z);
  for (const it of items) {
    if (it.i < 0) {
      drawPage(ctx, t, 960, 540, k, rot, pw, ph, prr);
      if (linked.has(-1)) circle(ctx, 960, 548, 15 * E.outBack(seg(t, 7.6, 7.9), 2), COL.ember);
      continue;
    }
    if (it.ap <= 0 || it.sc <= 0) continue;
    const u = 3.3 * it.ps * it.sc, fog = fogOf(it.z) * seg(it.ap, 0, 0.15);
    drawNode(ctx, it.X, it.Y, u, it.n.rot, it.n.moss, fog, linked.has(it.i) ? 1 : 0);
    const la = fog * seg(it.ap, 0.3, 0.7) * (1 - seg(t, 8.9, 9.05));
    if (la > 0) txt(ctx, `[[${it.n.name}]]`, it.X, it.Y + 34 * u + 22 * it.ps, { font: font(MONO, Math.round(17 * it.ps), 400), color: rgba(COL.paper, 0.6 * la), align: 'center' });
  }
  caption(ctx, t);
}

function caption(ctx, t) {
  const fs = 112, base = 960, fw = font(SANS, fs, 700);
  const sa = seg(t, 7.6, 8.0) * (1 - seg(t, 8.9, 9.2));
  if (sa > 0) {
    const g = ctx.createLinearGradient(0, 700, 0, H);
    g.addColorStop(0, rgba(COL.deep, 0)); g.addColorStop(0.55, rgba(COL.deep, 0.85 * sa)); g.addColorStop(1, rgba(COL.deep, 0.95 * sa));
    ctx.fillStyle = g; ctx.fillRect(0, 700, W, H - 700);
  }
  ctx.save();
  ctx.beginPath(); ctx.rect(0, base - fs * 1.02, W, fs * 1.3); ctx.clip();
  let x = 150;
  [['Everything', COL.paper], ['links.', COL.ember]].forEach(([w, c], i) => {
    const e = E.outExpo(seg(t, 7.78 + i * 0.09, 7.78 + i * 0.09 + 0.7)), o = E.inExpo(seg(t, 8.82 + i * 0.05, 8.82 + i * 0.05 + 0.32));
    txt(ctx, w, x, base + (1 - e) * fs * 1.15 - o * fs * 1.15, { font: fw, color: c, ls: -3 });
    x += measure(ctx, w + ' ', fw, -3);
  });
  ctx.restore();
  const e = seg(t, 7.95, 8.25) * (1 - seg(t, 8.8, 9.0));
  if (e > 0) txt(ctx, '[[LINKS]]  ·  BACKLINKS  ·  TAGS  ·  GRAPH', 154, base - fs - 26, { font: font(MONO, 22, 400), color: rgba(COL.paper, 0.55 * e), ls: 4 });
}

// ───────────────────────── 04 · fast (9.4 → 12.3) ─────────────────────────

function beatOpen(ctx, t) { // 75 ms
  ctx.fillStyle = COL.ember; ctx.fillRect(0, 0, W, H);
  const cp = E.outExpo(seg(ft(t), 9.46, 10.05)), n = Math.round(75 * cp);
  const ent = E.outExpo(seg(t, 9.4, 9.9));
  const fb = font(SANS, 420, 700);
  txt(ctx, String(n), 1010, 640 + (1 - ent) * 160, { font: fb, color: COL.deep, align: 'right', ls: -16 });
  const em = E.outExpo(seg(t, 9.52, 9.95));
  txt(ctx, 'ms', 1040 + (1 - em) * 80, 640, { font: font(SANS, 170, 700), color: rgba(COL.deep, em), ls: -4 });
  const la = E.outExpo(seg(t, 9.6, 10.0));
  txt(ctx, 'TO OPEN THE WINDOW', 560, 760 + (1 - la) * 20, { font: font(MONO, 32, 400), color: rgba(COL.deep, 0.8 * la), ls: 7 });
  // a timeline from 0 to 75 ms
  const bx0 = 560, bx1 = 1380, by = 860;
  ctx.strokeStyle = rgba(COL.deep, 0.25 * la); ctx.lineWidth = 3; line(ctx, bx0, by, bx0 + (bx1 - bx0) * la, by);
  ctx.strokeStyle = COL.deep; ctx.lineWidth = 8; ctx.lineCap = 'round';
  if (cp > 0) line(ctx, bx0, by, lerp(bx0, bx1, n / 75), by);
  [0, 25, 50, 75].forEach((v, i) => {
    const x = lerp(bx0, bx1, v / 75), a = seg(t, 9.62 + i * 0.05, 9.8 + i * 0.05);
    ctx.strokeStyle = rgba(COL.deep, 0.5 * a); ctx.lineWidth = 2; line(ctx, x, by + 14, x, by + 26);
    txt(ctx, String(v), x, by + 56, { font: font(MONO, 20, 400), color: rgba(COL.deep, 0.6 * a), align: 'center' });
  });
}

function beatKey(ctx, t) { // 1 ms
  ctx.fillStyle = COL.paper; ctx.fillRect(0, 0, W, H);
  // a 5,000-line note racing past behind
  const r = mulberry32(5), scroll = (t - 10.15) * 1500, lh = 44;
  const first = Math.floor(scroll / lh);
  for (let j = first; j < first + 28; j++) {
    const rr = mulberry32(j * 31 + 7), y = j * lh - scroll + 30;
    txt(ctx, (4970 + j).toLocaleString('en-US'), 150, y, { font: font(MONO, 18, 400), color: rgba(COL.ink, 0.16), align: 'right' });
    ctx.fillStyle = rgba(COL.ink, 0.055);
    let x = 190; const words = 3 + Math.floor(rr() * 9);
    for (let w = 0; w < words && x < 1800; w++) { const ww = 30 + rr() * 140; ctx.fillRect(x, y - 16, ww, 16); x += ww + 18; }
  }
  void r;
  const right = 900;
  const drop = spring(t - 10.27, 2.0, 0.42);
  txt(ctx, '1', right, 640 - (1 - drop) * 520, { font: font(SANS, 420, 700), color: COL.ink, align: 'right', ls: -16 });
  const em = E.outExpo(seg(t, 10.34, 10.7));
  txt(ctx, 'ms', 930 + (1 - em) * 80, 640, { font: font(SANS, 170, 700), color: rgba(COL.ink, em), ls: -4 });
  const msw = measure(ctx, 'ms', font(SANS, 170, 700), -4);
  if (em > 0.5 && ((t - 10.4) % 0.28) < 0.16) { ctx.fillStyle = COL.ember; ctx.fillRect(930 + msw + 22, 640 - 128, 12, 140); }
  const la = E.outExpo(seg(t, 10.4, 10.75));
  txt(ctx, 'A KEYSTROKE, IN A 5,000-LINE NOTE', 650, 760 + (1 - la) * 20, { font: font(MONO, 32, 400), color: rgba(COL.ink, 0.75 * la), ls: 7 });
  return { cx: 930 + msw + 28, cy: 570 };
}

function beatRows(ctx, t) { // 1,000,000 rows
  ctx.fillStyle = COL.ink; ctx.fillRect(0, 0, W, H);
  const k = t - 10.85, rh = 54, cw = 240;
  const off = (5200 / 3) * (1 - Math.exp(-3 * Math.max(0, k))) * 3 + 260 * Math.max(0, k);
  const first = Math.floor(off / rh);
  ctx.lineWidth = 1;
  for (let j = first; j < first + Math.ceil(H / rh) + 2; j++) {
    const y = j * rh - off;
    ctx.strokeStyle = rgba(COL.paper, 0.05); line(ctx, 0, y, W, y);
    const rr = mulberry32(j * 13 + 1);
    for (let c = 0; c < 8; c++) {
      const v = c === 0 ? (j + 1).toLocaleString('en-US') : c % 3 === 1 ? (rr() * 9999).toFixed(2) : Math.floor(rr() * 99999).toString(36);
      txt(ctx, v, c * cw + 24, y + 35, { font: font(MONO, 21, 400), color: rgba(COL.paper, c === 0 ? 0.3 : 0.2) });
    }
  }
  for (let c = 1; c < 8; c++) { ctx.strokeStyle = rgba(COL.paper, 0.05); line(ctx, c * cw, 0, c * cw, H); }
  const g = ctx.createRadialGradient(960, 560, 100, 960, 560, 900);
  g.addColorStop(0, rgba(COL.ink, 0.92)); g.addColorStop(0.5, rgba(COL.ink, 0.6)); g.addColorStop(1, rgba(COL.ink, 0.1));
  ctx.fillStyle = g; ctx.fillRect(0, 0, W, H);
  // scrollbar
  const cp = E.outExpo(seg(ft(t), 10.95, 11.55));
  ctx.fillStyle = rgba(COL.paper, 0.08); ctx.fillRect(1868, 120, 8, 840);
  ctx.fillStyle = COL.ember; ctx.fillRect(1868, 120 + cp * 800, 8, 40);
  const n = Math.round(1e6 * cp), fb = font(MONO, 200, 700);
  const full = measure(ctx, '1,000,000', fb, -8);
  const ent = E.outExpo(seg(t, 10.9, 11.3));
  txt(ctx, n.toLocaleString('en-US'), 960 + full / 2, 610 + (1 - ent) * 120, { font: fb, color: rgba(COL.paper, ent), align: 'right', ls: -8 });
  const la = E.outExpo(seg(t, 11.05, 11.4));
  txt(ctx, 'ROWS, IN THE CSV GRID', 960, 730 + (1 - la) * 20, { font: font(MONO, 32, 400), color: rgba(COL.ember, la), align: 'center', ls: 7 });
}

function sceneFast(ctx, t) {
  if (t < 10.5) beatOpen(ctx, t);
  if (t >= 10.15) {
    // a slanted wipe from the right
    const p = E.inOutExpo(seg(t, 10.15, 10.48)), X = lerp(W + 360, -360, p), sl = Math.tan(14 * D2R) * H;
    ctx.save();
    if (p < 1) { ctx.beginPath(); ctx.moveTo(X + sl / 2, 0); ctx.lineTo(W + 400, 0); ctx.lineTo(W + 400, H); ctx.lineTo(X - sl / 2, H); ctx.closePath(); ctx.clip(); }
    const c = beatKey(ctx, t);
    ctx.restore();
    if (p < 1) { ctx.strokeStyle = COL.deep; ctx.lineWidth = 10; line(ctx, X + sl / 2, 0, X - sl / 2, H); }
    if (t >= 10.85) {
      const q = seg(t, 10.85, 11.18), rad = 2300 * E.inOutExpo(q);
      ctx.save(); ctx.beginPath(); ctx.arc(c.cx, c.cy, Math.max(0.1, rad), 0, Math.PI * 2); ctx.clip();
      beatRows(ctx, t); ctx.restore();
      if (q < 1) { ctx.beginPath(); ctx.arc(c.cx, c.cy, Math.max(0.1, rad), 0, Math.PI * 2); ctx.strokeStyle = COL.ember; ctx.lineWidth = 6; ctx.stroke(); }
    }
  }
}

// ──────────────────── 05 · themes, 06 · bref (11.6 → 15) ────────────────────

const S2 = 2.6, TILE = 128 * S2;
function stripes(ctx, t, lx) {
  const n = THEMES.length, sw = W / n;
  const col = seg(t, 12.26, 12.74);
  const textA = 1 - seg(t, 12.22, 12.34);
  const big = 1 + 0.25 * (1 - E.outExpo(seg(t, 11.62, 12.15)));
  // bounding clip that rounds as it lands on the tile
  const ce = E.inOutExpo(col);
  ctx.save();
  if (col > 0) {
    const bx = lerp(0, lx - TILE / 2, ce), by = lerp(0, 540 - TILE / 2, ce);
    rrect(ctx, bx, by, lerp(W, TILE, ce), lerp(H, TILE, ce), lerp(0, 28 * S2, ce)); ctx.clip();
  }
  THEMES.forEach(([name, bg, fg, faint, acc], i) => {
    const d = 11.6 + i * 0.022, drop = E.outExpo(seg(t, d, d + 0.42));
    if (drop <= 0) return;
    const ci = E.inOutExpo(seg(t, 12.26 + Math.abs(i - 6) * 0.012, 12.6 + Math.abs(i - 6) * 0.012));
    const x = lerp(i * sw, lx - TILE / 2 + (i * TILE) / n, ci), w = lerp(sw, TILE / n, ci) + 1;
    const y = lerp(-H * (1 - drop), 540 - TILE / 2, ci), h = lerp(H, TILE, ci);
    ctx.save(); ctx.beginPath(); ctx.rect(x, y, w, h); ctx.clip();
    ctx.fillStyle = mix(bg, COL.ink, seg(ci, 0.4, 0.9)); ctx.fillRect(x, y, w, h);
    if (textA > 0) {
      ctx.globalAlpha = textA;
      ctx.translate(0, y);
      ctx.save(); ctx.translate(960, 560); ctx.scale(big, big);
      txt(ctx, '13', 0, 150, { font: font(SANS, 560, 700), color: fg, align: 'center', ls: -24 });
      txt(ctx, 'themes', 0, 300, { font: font(SANS, 116, 700), color: acc, align: 'center', ls: -3 });
      ctx.restore();
      circle(ctx, x + w / 2, 128, 7, acc);
      ctx.save(); ctx.translate(x + w / 2 - 6, 160); ctx.rotate(Math.PI / 2);
      txt(ctx, name.toUpperCase(), 0, 0, { font: font(MONO, 15, 400), color: faint, ls: 3 });
      ctx.restore();
    }
    ctx.restore();
  });
  ctx.restore();
}

function sceneFinale(ctx, t) {
  // the wordmark lockup
  const fw = font(SERIF, 290, 600), wm = measure(ctx, 'Bref', fw, -4), gap = 70;
  const total = TILE + gap + wm, LX = 960 - total / 2 + TILE / 2, CY = 520;
  const m = E.inOutExpo(seg(t, 13.25, 13.85));
  const lx = lerp(960, LX, m);

  if (t < 12.2) { beatRows(ctx, t); } else { ctx.fillStyle = COL.deep; ctx.fillRect(0, 0, W, H); }
  if (t >= 12.2) {
    glow(ctx, lx, CY, 760, COL.ember, 0.1 * seg(t, 12.6, 13.2));
    dust(ctx, t, 0.7 * seg(t, 12.6, 13.4));
  }
  if (t < 12.76) {
    ctx.save(); ctx.translate(0, (CY - 540) * E.inOutExpo(seg(t, 12.26, 12.74))); stripes(ctx, t, lx); ctx.restore();
  }
  if (t < 12.6) return;

  const [sx, sy] = shake(t, 13.14, 7, 0.5);
  const cam = { s: 1, rot: 0, fx: 64, fy: 64, sx, sy };
  logoCam(ctx, cam, S2, lx, CY);
  drawTile(ctx, { a: seg(t, 12.6, 12.72) });
  const pa = spring(t - 12.62, 1.9, 0.5), ra = spring(t - 12.62, 1.5, 0.38);
  if (t > 12.62) drawCardA(ctx, { dy: lerp(-110, 0, pa), dx: lerp(-30, 0, pa), rot: lerp(-40, 0, ra), s: lerp(1.25, 1, pa), a: seg(t, 12.62, 12.68), ln: E.outExpo(seg(t, 12.84, 13.12)) });
  const pb = spring(t - 12.7, 1.9, 0.5), rb = spring(t - 12.7, 1.5, 0.38);
  if (t > 12.7) drawCardB(ctx, { dy: lerp(110, 0, pb), dx: lerp(30, 0, pb), rot: lerp(40, 0, rb), s: lerp(1.25, 1, pb), a: seg(t, 12.7, 12.76), ln: E.outExpo(seg(t, 12.92, 13.2)) });
  sheen(ctx, seg(t, 13.4, 13.95));
  const pulse = t > 14.2 ? 1 + 0.06 * Math.sin((t - 14.2) * 6) * Math.exp(-(t - 14.2) * 1.2) : 1;
  drawLink(ctx, {
    r1: 9.5 * E.outBack(seg(t, 12.92, 13.1), 2.6) * pulse,
    r2: 9.5 * E.outBack(seg(t, 13.1, 13.28), 2.6) * pulse,
    prog: E.outExpo(seg(t, 12.98, 13.22)),
  });
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const [dx2, dy2] = [lx + (88 - 64) * S2 + sx, CY + (72 - 64) * S2 + sy];
  ring(ctx, dx2, dy2, t, 13.12, 20, 380, 0.7, COL.ember, 0.6, 8);
  sparks(ctx, dx2, dy2, t, 13.12, 22, 13, 30, 280, 0.6, 0.6);

  // wordmark: each letter rises through a slot
  const cap = (() => { ctx.font = fw; return ctx.measureText('B').actualBoundingBoxAscent; })();
  const base = CY + cap / 2, wx = lx + TILE / 2 + gap;
  if (t > 13.3) {
    ctx.save(); ctx.beginPath(); ctx.rect(wx - 40, base - cap * 1.35, wm + 120, cap * 1.65); ctx.clip();
    let x = wx;
    'Bref'.split('').forEach((ch, i) => {
      const e = E.outExpo(seg(t, 13.52 + i * 0.06, 13.52 + i * 0.06 + 0.75));
      const xi = wx + measure(ctx, 'Bref'.slice(0, i), fw, -4);
      txt(ctx, ch, xi, base + (1 - e) * cap * 1.5, { font: fw, color: COL.paper, ls: -4 });
      x = xi;
    });
    void x;
    ctx.restore();
  }
  // tagline, uncovered by a sweeping bar
  const tag = [
    ['A fast, minimal note-taking app.', font(MONO, 30, 400), rgba(COL.paper, 0.7), 13.92, 0, 1],
    ['PLAIN MARKDOWN  ·  LINKED NOTES  ·  WRITTEN IN RUST', font(MONO, 19, 400), COL.ember, 14.12, 4, 54],
  ];
  for (const [s, f, c, t0, ls, dy] of tag) {
    const p = E.inOutCubic(seg(t, t0, t0 + 0.55));
    if (p <= 0) continue;
    const tw = measure(ctx, s, f, ls), y = base + 84 + dy;
    ctx.save(); ctx.beginPath(); ctx.rect(wx, y - 40, tw * p + 2, 60); ctx.clip();
    txt(ctx, s, wx + 4, y, { font: f, color: c, ls });
    ctx.restore();
    if (p < 1) { ctx.fillStyle = COL.ember; ctx.fillRect(wx + tw * p, y - 28, 5, 36); }
  }
}

// ───────────────────────────── HUD ─────────────────────────────

const LIGHT = [[0, 0], [3.3, 0], [3.42, 1], [6.95, 1], [7.45, 0], [9.38, 0], [9.52, 1], [10.85, 1], [11.05, 0], [11.62, 0], [11.9, 0.15], [12.3, 0]];
const SCENES = [[0, '01 / ASSEMBLE'], [2.72, '02 / WRITE'], [7.0, '03 / LINK'], [9.4, '04 / FAST'], [11.6, '05 / THEMES'], [12.3, '06 / BREF']];
function lightAt(t) {
  for (let i = 1; i < LIGHT.length; i++) if (t < LIGHT[i][0]) return lerp(LIGHT[i - 1][1], LIGHT[i][1], seg(t, LIGHT[i - 1][0], LIGHT[i][0]));
  return LIGHT[LIGHT.length - 1][1];
}
function hud(ctx, t) {
  const a = seg(t, 0.15, 0.55) * (1 - seg(t, 13.5, 13.95));
  if (a <= 0) return;
  const l = lightAt(t), c = (al) => mix(COL.paper, COL.ink, l, al * a);
  const m = 44, L = 24;
  ctx.lineWidth = 2; ctx.strokeStyle = c(0.5);
  for (const [x, y, dx, dy] of [[m, m, 1, 1], [W - m, m, -1, 1], [m, H - m, 1, -1], [W - m, H - m, -1, -1]]) {
    line(ctx, x, y, x + dx * L, y); line(ctx, x, y, x, y + dy * L);
  }
  const f = font(MONO, 16, 400), o = { font: f, color: c(0.55), ls: 2 };
  txt(ctx, 'BREF — MOTION REEL', m + 40, m + 20, o);
  const fr = Math.floor(t * FPS + 1e-6), ss = String(Math.floor(fr / FPS)).padStart(2, '0'), ff = String(fr % FPS).padStart(2, '0');
  txt(ctx, `TC 00:00:${ss}:${ff}`, W - m - 40, m + 20, { ...o, align: 'right' });
  let label = SCENES[0][1];
  for (const [s, n] of SCENES) if (t >= s) label = n;
  txt(ctx, label, m + 40, H - m - 6, o);
  txt(ctx, '1920 × 1080 — 60 FPS', W - m - 40, H - m - 6, { ...o, align: 'right' });
  // progress along the reel, with a tick per scene
  const x0 = 520, x1 = 1400, y = H - m - 12;
  ctx.strokeStyle = c(0.18); ctx.lineWidth = 2; line(ctx, x0, y, x1, y);
  ctx.strokeStyle = COL.ember; ctx.globalAlpha = a; line(ctx, x0, y, lerp(x0, x1, t / DUR), y); ctx.globalAlpha = 1;
  ctx.strokeStyle = c(0.4);
  for (const [s] of SCENES) { const x = lerp(x0, x1, s / DUR); line(ctx, x, y - 6, x, y + 6); }
}

// ───────────────────────────── frame ─────────────────────────────

function render(ctx, t) {
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1; ctx.globalCompositeOperation = 'source-over'; ctx.letterSpacing = '0px';
  ctx.lineCap = 'butt'; ctx.setLineDash([]);
  if (t < 3.47) sceneOpen(ctx, t);
  else if (t < 9.36) sceneLink(ctx, t);
  else if (t < 9.62) {
    sceneLink(ctx, t);
    const q = seg(t, 9.36, 9.6), rad = 1150 * E.inOutCubic(q);
    ctx.save(); ctx.beginPath(); ctx.arc(960, 540, Math.max(0.1, rad), 0, Math.PI * 2); ctx.clip(); beatOpen(ctx, t); ctx.restore();
  } else if (t < 11.6) sceneFast(ctx, t);
  else sceneFinale(ctx, t);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  hud(ctx, t);
}

// Grain and vignette, applied once per frame after the motion blur.
let VIGNETTE, GRAIN;
function makePost() {
  VIGNETTE = document.createElement('canvas'); VIGNETTE.width = W; VIGNETTE.height = H;
  const v = VIGNETTE.getContext('2d'), g = v.createRadialGradient(960, 540, 420, 960, 540, 1250);
  g.addColorStop(0, 'rgba(0,0,0,0)'); g.addColorStop(1, 'rgba(0,0,0,0.34)');
  v.fillStyle = g; v.fillRect(0, 0, W, H);
  GRAIN = [0, 1, 2, 3].map(s => {
    const c = document.createElement('canvas'); c.width = c.height = 256;
    const x = c.getContext('2d'), img = x.createImageData(256, 256), r = mulberry32(1000 + s);
    for (let i = 0; i < img.data.length; i += 4) { const n = 128 + (r() - 0.5) * 200; img.data[i] = img.data[i + 1] = img.data[i + 2] = n; img.data[i + 3] = 255; }
    x.putImageData(img, 0, 0); return c;
  });
}
function post(ctx, frame, t) {
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = lerp(1, 0.3, lightAt(t));
  ctx.drawImage(VIGNETTE, 0, 0); ctx.globalAlpha = 1;
  const r = mulberry32(frame * 7 + 1);
  ctx.globalCompositeOperation = 'overlay'; ctx.globalAlpha = 0.07;
  ctx.save(); ctx.translate(-Math.floor(r() * 256), -Math.floor(r() * 256));
  ctx.fillStyle = ctx.createPattern(GRAIN[frame % GRAIN.length], 'repeat'); ctx.fillRect(0, 0, W + 256, H + 256);
  ctx.restore();
  ctx.globalCompositeOperation = 'source-over'; ctx.globalAlpha = 1;
}

// ───────────────────────────── drivers ─────────────────────────────

async function loadFonts() {
  const base = '../../assets/fonts/';
  const list = [[SANS, 'IBMPlexSans-Regular.ttf', '400'], [SANS, 'IBMPlexSans-Bold.ttf', '700'], [MONO, 'IBMPlexMono-Regular.ttf', '400'], [MONO, 'IBMPlexMono-Bold.ttf', '700'], [SERIF, 'IBMPlexSerif-SemiBold.ttf', '600']];
  await Promise.all(list.map(async ([fam, file, w]) => { const f = new FontFace(fam, `url(${base}${file})`, { weight: w }); await f.load(); document.fonts.add(f); }));
}

(async function main() {
  const out = document.getElementById('reel'), octx = out.getContext('2d');
  const sub = document.createElement('canvas'); sub.width = W; sub.height = H;
  const sctx = sub.getContext('2d');
  await loadFonts(); makePost();
  const params = new URLSearchParams(location.search);
  const SUB = Number(params.get('blur') || 8), SHUTTER = 0.5; // 180° shutter

  // One finished frame: SUB instants across the shutter, averaged, then grain.
  window.renderFrame = (frame, format = 'image/png') => {
    const t = frame / FPS; FRAME_T = t;
    for (let k = 0; k < SUB; k++) {
      render(sctx, t + (k / SUB) * (SHUTTER / FPS));
      octx.globalAlpha = 1 / (k + 1); octx.drawImage(sub, 0, 0);
    }
    FRAME_T = null;
    octx.globalAlpha = 1; post(octx, frame, t);
    return format ? out.toDataURL(format) : null;
  };
  window.reelReady = true;
  if (params.has('render')) return;
  // Live preview, looping.
  const start = performance.now();
  (function tick() {
    const t = ((performance.now() - start) / 1000) % DUR, f = Math.floor(t * FPS);
    render(octx, f / FPS); post(octx, f, f / FPS);
    requestAnimationFrame(tick);
  })();
})();
