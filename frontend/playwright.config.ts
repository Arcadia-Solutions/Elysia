import { fileURLToPath } from 'node:url'
import { defineConfig, devices } from '@playwright/test'

// Runs its own backend (port 8091, database elysia_e2e) and Vite (port 5175)
// so the dev instances and their data are left alone. The tests edit the
// global upload settings, hence one worker.
export default defineConfig({
  testDir: 'e2e',
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: 'http://localhost:5175',
    trace: 'retain-on-failure',
    permissions: ['clipboard-read', 'clipboard-write'],
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      command: 'cargo run -q',
      cwd: '../backend',
      env: { ELYSIA_CONFIG: fileURLToPath(new URL('./e2e/config.yml', import.meta.url)) },
      url: 'http://localhost:8091/api/elysia-unauth-settings',
      timeout: 600_000,
      reuseExistingServer: !process.env.CI,
    },
    {
      command: 'npx vite --port 5175',
      env: { ELYSIA_API: 'http://localhost:8091' },
      url: 'http://localhost:5175',
      reuseExistingServer: !process.env.CI,
    },
  ],
})
