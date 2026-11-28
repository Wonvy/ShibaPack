import { defineConfig } from "@playwright/test";

const port = Number(process.env.SHIBAPACK_E2E_PORT ?? 1431);
if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("Invalid SHIBAPACK_E2E_PORT");
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: true,
  use: {
    baseURL,
    viewport: { width: 1440, height: 900 },
    trace: "retain-on-failure",
  },
  webServer: {
    command: `npm run dev -- --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
  },
});
