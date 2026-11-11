import { defineConfig } from "vitest/config";

// Pure frontend logic only: no DOM here, the UI is checked by hand until UI tests are added.
export default defineConfig({
  test: {
    include: ["tests/unit/**/*.test.ts"],
    environment: "node",
  },
});
