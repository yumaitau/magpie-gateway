const { defineConfig } = require('@playwright/test');

const channel = process.env.PLAYWRIGHT_CHROME === '1' ? 'chrome' : undefined;

module.exports = defineConfig({
  testDir: '.',
  timeout: 60000,
  use: {
    baseURL: process.env.MAGPIE_BASE || 'http://127.0.0.1:8088',
    channel,
  },
});
