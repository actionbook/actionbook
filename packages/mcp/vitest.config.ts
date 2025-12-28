import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    include: ["src/**/*.test.ts"],
    reporters: ["default"],
    coverage: {
      reporter: ["text", "lcov"],
      enabled: process.env.CI === "true" || process.env.COVERAGE === "true",
    },
  },
});
