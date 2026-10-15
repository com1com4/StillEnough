import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // Node's version tests run separately; generated snapshots are not test roots.
    include: ["src/**/*.{test,spec}.{ts,tsx,js,jsx}"],
  },
});
