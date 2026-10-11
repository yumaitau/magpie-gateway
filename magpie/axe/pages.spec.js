const { test, expect } = require('@playwright/test');
const AxeBuilder = require('@axe-core/playwright').default;

const pages = [
  ['/login', 'login'],
  ['/dashboard', 'dashboard'],
  ['/agents', 'agents'],
  ['/catalogue', 'catalogue'],
  ['/providers', 'providers'],
  ['/policies', 'policies'],
  ['/dlp', 'dlp'],
  ['/audit', 'audit'],
  ['/approvals', 'approvals'],
  ['/settings', 'settings'],
  ['/australia', 'australia'],
];

const tags = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

for (const [path, name] of pages) {
  test(`${name} has no axe violations`, async ({ page }) => {
    const response = await page.goto(path);
    expect(response.status()).toBe(200);
    const html = await page.content();
    if (path !== '/login') {
      expect(html).toContain('Example data');
    }
    expect(html).toContain('Magpie Gateway');
    const results = await new AxeBuilder({ page }).withTags(tags).analyze();
    expect(results.violations, JSON.stringify(results.violations, null, 2)).toEqual([]);
    if (process.env.MAGPIE_SCREENSHOTS) {
      await page.screenshot({
        path: `${process.env.MAGPIE_SCREENSHOTS}/${name}.png`,
        fullPage: true,
      });
    }
  });
}

// Alert destinations require an authenticated settings administrator. Their
// accessibility coverage lives in native/accounts.spec.js, which signs in.
test('anonymous demo does not expose alert destinations', async ({ page }) => {
  const response = await page.goto('/alerts');
  expect(response.status()).toBe(403);
});
