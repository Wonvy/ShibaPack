import { defineConfig } from "@playwright/test";

const baseURL = "http://127.0.0.1:1431";

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: true,
  use: {
    baseURL,
    viewport: { width: 1440, height: 900 },
    trace: "retain-on-failure",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 1431 --strictPort",
    url: baseURL,
    reuseExistingServer: false,
  },
});
