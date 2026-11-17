// Renders the reel to an MP4, frame by frame, with motion blur.
//
//   node scripts/reel/render.mjs [out.mp4]          the whole reel
//   node scripts/reel/render.mjs --stills 1.2 7.5   PNG stills at those seconds
//
// Needs Playwright's Chromium and ffmpeg.
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { chromium } = require('playwright');
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const FPS = 60, FRAMES = 15 * FPS;

const types = { '.html': 'text/html', '.js': 'text/javascript', '.ttf': 'font/ttf' };
const server = http.createServer((req, res) => {
  const file = path.join(root, decodeURIComponent(new URL(req.url, 'http://x').pathname));
  if (!file.startsWith(root) || !fs.existsSync(file)) { res.writeHead(404).end(); return; }
  res.writeHead(200, { 'content-type': types[path.extname(file)] || 'application/octet-stream' });
  fs.createReadStream(file).pipe(res);
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const url = `http://127.0.0.1:${server.address().port}/scripts/reel/index.html?render`;

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
page.on('pageerror', e => console.error('page error:', e.message));
await page.goto(url);
await page.waitForFunction(() => window.reelReady === true, null, { timeout: 30000 });
const frame = f => page.evaluate(f => window.renderFrame(f).split(',')[1], f).then(b => Buffer.from(b, 'base64'));

const args = process.argv.slice(2);
if (args[0] === '--stills') {
  const dir = path.join(here, 'stills');
  fs.mkdirSync(dir, { recursive: true });
  for (const s of args.slice(1)) {
    const f = Math.round(Number(s) * FPS);
    fs.writeFileSync(path.join(dir, `t${s.padStart(5, '0')}.png`), await frame(f));
  }
  console.log('stills in', dir);
} else {
  const out = path.resolve(args[0] || path.join(here, 'bref-reel.mp4'));
  const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', String(FPS), '-c:v', 'png', '-i', '-',
    '-c:v', 'libx264', '-preset', 'slow', '-crf', '16', '-pix_fmt', 'yuv420p', '-profile:v', 'high', '-movflags', '+faststart', out], { stdio: ['pipe', 'inherit', 'inherit'] });
  const t0 = Date.now();
  for (let f = 0; f < FRAMES; f++) {
    const png = await frame(f);
    if (!ff.stdin.write(png)) await new Promise(r => ff.stdin.once('drain', r));
    if (f % 60 === 0) console.log(`frame ${f}/${FRAMES}  ${((Date.now() - t0) / 1000).toFixed(0)}s`);
  }
  ff.stdin.end();
  await new Promise(r => ff.on('close', r));
  console.log('wrote', out);
}
await browser.close();
server.close();
