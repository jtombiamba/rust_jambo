import { defineConfig, devices } from '@playwright/test';

// Live diagnostic config: does NOT start a dev server.
// The app is expected to already be running at http://localhost:3000 (Docker).
export default defineConfig({
  testDir: './tests',
  timeout: 120000,
  expect: {
    timeout: 10000,
  },
  fullyParallel: false,
  workers: 1,
  reporter: 'line',
  use: {
    baseURL: 'http://localhost:3000',
    trace: 'on',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
