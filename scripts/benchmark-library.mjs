import { chromium } from "playwright";
import { createServer } from "vite";

const count = Number(process.argv[2] ?? 1500);
const server = await createServer({
  server: { host: "127.0.0.1", port: 0, hmr: false },
});
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ headless: true });
  const port = server.httpServer.address().port;
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
  await page.goto(`http://127.0.0.1:${port}/#/library`);
  await page.locator("[data-software-id]").first().waitFor();
  const result = await page.evaluate(async (size) => {
    const { useApp } = await import("/src/lib/store.ts");
    const library = useApp.getState().library;
    const sample = library.software[0];
    const nextFrame = () => new Promise((resolve) => requestAnimationFrame(resolve));
    const flush = async () => { await nextFrame(); await nextFrame(); };
    const rows = () => document.querySelectorAll("main [data-software-id]").length;
    const software = Array.from({ length: size }, (_, i) => ({
      ...sample, id: `bench-${i}`, name: `Benchmark package ${String(i).padStart(5, "0")}`, starred: false,
    }));

    const start = performance.now();
    useApp.setState({ library: { ...library, software, settings: { ...library.settings, fetchSummaries: false } } });
    await flush();
    const renderMs = performance.now() - start;
    const mountedRows = rows();
    const searchStart = performance.now();
    useApp.getState().patch({ search: software[size - 1].name });
    for (let frame = 0; frame < 90 && rows() !== 1; frame++) await nextFrame();
    await flush();
    const searchMs = performance.now() - searchStart;

    useApp.getState().patch({ search: "" });
    await flush();
    const viewport = document.querySelector("main .overflow-y-auto");
    const samples = [];
    for (let i = 1; i <= 16; i++) {
      const before = performance.now();
      viewport.scrollTop = ((viewport.scrollHeight - viewport.clientHeight) * i) / 16;
      await nextFrame();
      samples.push(performance.now() - before);
    }
    samples.sort((a, b) => a - b);
    return {
      records: size,
      renderMs: Math.round(renderMs),
      mountedRows,
      searchMs: Math.round(searchMs),
      scrollP95Ms: Math.round(samples[Math.ceil(samples.length * 0.95) - 1]),
    };
  }, count);
  console.log(JSON.stringify(result));
} finally {
  await browser?.close();
  await server.close();
}
