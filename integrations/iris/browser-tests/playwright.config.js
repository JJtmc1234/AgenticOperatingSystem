import { defineConfig } from '@playwright/test';
const output = process.env.IRIS_TEST_OUTPUT || './test-results';
export default defineConfig({
  testDir: './tests', fullyParallel: false, workers: 1, retries: 0,
  timeout: 20000, globalTimeout: 120000,
  outputDir: output + '/artifacts',
  reporter: [['list'], ['html', { outputFolder: output + '/html', open: 'never' }],
    ['json', { outputFile: output + '/results.json' }]],
  use: { baseURL: 'http://127.0.0.1:18787', browserName: 'chromium',
    screenshot: 'only-on-failure', trace: 'retain-on-failure' },
  webServer: { command: 'node server.mjs', url: 'http://127.0.0.1:18787/',
    reuseExistingServer: false, timeout: 15000 },
});
