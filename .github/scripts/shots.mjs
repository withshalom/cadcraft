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
  // Tap the left handle, then close the drawer, then tap the right handle.
  await page.touchscreen.tap(16, height / 2);
  await page.waitForTimeout(2500);
  await page.screenshot({ path: `${out}/${name}-2-left-drawer.png` });
  await page.touchscreen.tap(width - 10, height / 2);
  await page.waitForTimeout(2500);
  await page.screenshot({ path: `${out}/${name}-3-after-tap-right.png` });
});

await visit('desktop', { viewport: { width: 1280, height: 800 } }, async () => {});

await browser.close();
server.close();
fs.writeFileSync(`${out}/logs.txt`, logs.join('\n') + '\n');
