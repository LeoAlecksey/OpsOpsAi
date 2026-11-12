import { defineConfig } from "@playwright/test";

// UI tests on a fake backend (tests/ui/mock.js): the real interface in Chromium, no Rust needed.
export default defineConfig({
  testDir: "tests/ui",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://localhost:1420",
    viewport: { width: 1280, height: 820 },
    // a local Chrome can be used instead of the bundled browser: PW_CHROME=/path/to/chrome
    launchOptions: process.env.PW_CHROME ? { executablePath: process.env.PW_CHROME } : {},
  },
  webServer: { command: "npx vite --port 1420 --strictPort", url: "http://localhost:1420", reuseExistingServer: !process.env.CI },
});
