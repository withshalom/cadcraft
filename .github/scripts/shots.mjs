// Loads the built site in headless Chromium at a phone size and a desktop size, and writes
// screenshots and the browser console to the output folder. Usage: node shots.mjs <site> <out>
import { chromium, devices } from 'playwright';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

const [, , root, out] = process.argv;
fs.mkdirSync(out, { recursive: true });
const logs = [];
const log = (line) => {
  logs.push(line);
  console.log(line);
};

const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.json': 'application/json' };
const server = http
  .createServer((req, res) => {
    let p = decodeURIComponent(req.url.split('?')[0]);
    if (p === '/') p = '/index.html';
    const file = path.join(root, p);
    fs.readFile(file, (error, data) => {
      if (error) {
        res.statusCode = 404;
        res.end();
        return;
      }
      res.setHeader('content-type', types[path.extname(file)] || 'application/octet-stream');
      res.end(data);
    });
  })
  .listen(8099);

const browser = await chromium.launch({
  args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist', '--enable-webgl'],
});

async function visit(name, contextOptions, steps) {
  const context = await browser.newContext(contextOptions);
  // Headless Chromium with an emulated pixel ratio reports devicePixelContentBoxSize in CSS pixels,
  // which leaves the canvas at 1x and makes the UI look 3x too big. iOS Safari has no such
  // property (egui falls back to contentBoxSize x devicePixelRatio), so remove it to match a phone.
  if (name === 'phone') {
    await context.addInitScript(() => {
      delete ResizeObserverEntry.prototype.devicePixelContentBoxSize;
    });
  }
  const page = await context.newPage();
  page.on('console', (m) => log(`[${name}] console.${m.type()}: ${m.text()}`));
  page.on('pageerror', (e) => log(`[${name}] PAGEERROR: ${e.message}`));
  page.on('requestfailed', (r) => log(`[${name}] REQUEST FAILED: ${r.url()}`));
  page.on('crash', () => log(`[${name}] PAGE CRASHED`));
  try {
    await page.goto('http://localhost:8099/?webgl&sample', { waitUntil: 'load', timeout: 60000 });
    await page.waitForTimeout(20000);
    const loading = await page.evaluate(() => {
      const el = document.getElementById('cadcraft_loading');
      return el ? `still present (display=${getComputedStyle(el).display})` : 'gone';
    });
    log(`[${name}] loading text: ${loading}; viewport ${page.viewportSize()?.width}x${page.viewportSize()?.height}`);
    const metrics = await page.evaluate(() => {
      const c = document.getElementById('cadcraft_canvas');
      const r = c.getBoundingClientRect();
      const v = window.visualViewport;
      return JSON.stringify({
        inner: [window.innerWidth, window.innerHeight],
        dpr: window.devicePixelRatio,
        visual: v ? [v.width, v.height, v.scale, v.offsetLeft, v.offsetTop] : null,
        canvasPx: [c.width, c.height],
        canvasRect: [r.left, r.top, r.width, r.height],
        scroll: [document.documentElement.scrollWidth, document.documentElement.scrollHeight, window.scrollX, window.scrollY],
      });
    });
    log(`[${name}] page metrics: ${metrics}`);
    await page.screenshot({ path: `${out}/${name}-1-start.png` });
    await steps(page, name);
  } catch (error) {
    log(`[${name}] SCRIPT ERROR: ${error.message}`);
    try {
      await page.screenshot({ path: `${out}/${name}-error.png` });
    } catch {}
  }
  await context.close();
}

await visit('phone', { ...devices['iPhone 14 Pro'] }, async (page, name) => {
  const { width, height } = page.viewportSize();
  const shot = async (label) => {
    await page.waitForTimeout(2500);
    await page.screenshot({ path: `${out}/${name}-${label}.png` });
  };
  const focused = async (label) => {
    const tag = await page.evaluate(() => (document.activeElement ? document.activeElement.tagName : 'none'));
    log(`[${name}] after ${label}: focused element = ${tag}`);
  };
  // The left handle, then the dimmed drawing to close it, then the right handle.
  await page.touchscreen.tap(16, height / 2);
  await shot('2-left-drawer');
  await page.touchscreen.tap(width - 8, height / 2);
  await shot('3-left-closed');
  await page.touchscreen.tap(width - 14, height / 2);
  await shot('4-right-drawer');
  await page.touchscreen.tap(8, height / 2);
  await shot('5-right-closed');
  // The File menu, then close it.
  await page.touchscreen.tap(24, 82);
  await shot('6-file-menu');
  // Outside the menu (which now reaches x=223), so this does not click one of its items.
  await page.touchscreen.tap(310, 420);
  await shot('7-menu-closed');
  // The command line: a tap should hand focus to the hidden text input (what brings up the keyboard).
  await page.touchscreen.tap(width / 2, height - 75);
  await shot('8-command-tapped');
  await focused('tapping the command line');
  // The Start tab (the page shown when no drawing is open).
  await page.touchscreen.tap(110, 123);
  await shot('9-start-tab');
});

await visit('desktop', { viewport: { width: 1280, height: 800 } }, async () => {});

await browser.close();
server.close();
fs.writeFileSync(`${out}/logs.txt`, logs.join('\n') + '\n');
