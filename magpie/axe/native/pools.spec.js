const { test, expect } = require('@playwright/test');
const AxeBuilder = require('@axe-core/playwright').default;

const base = process.env.MAGPIE_NATIVE_URL;
test.skip(!base, 'MAGPIE_NATIVE_URL is not set');

test('pool owners connect subscriptions, issue scoped keys, and revoke access', async ({ page }) => {
  await page.goto(`${base}/ui/login`);
  await page.getByLabel('Email or username').fill(process.env.MAGPIE_NATIVE_USER || 'admin');
  await page.getByLabel('Password', { exact: true }).fill(process.env.MAGPIE_NATIVE_PASSWORD || 'local-preview-passphrase-12345');
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await page.getByRole('link', { name: 'Subscription pools', exact: true }).first().click();
  const name = `Pool fixture ${Date.now()}`;
  await page.getByLabel('Pool name', { exact: true }).fill(name);
  await page.getByLabel('Share with signed-in colleagues').check();
  await page.getByRole('button', { name: 'Create pool', exact: true }).click();
  const pool = page.getByRole('region', { name: `Pool ${name}`, exact: true });
  await expect(pool).toBeVisible();
  await pool.getByRole('button', { name: 'Connect subscription', exact: true }).click();
  await pool.getByLabel('Subscription name', { exact: true }).fill('Claude fixture');
  await pool.getByLabel('Other model ID', { exact: true }).fill('claude-fixture');
  await pool.getByRole('button', { name: 'Add model', exact: true }).click();
  await pool.getByLabel('Claude setup-token', { exact: true }).fill(`sk-ant-oat01-${Date.now()}${'x'.repeat(40)}`);
  await pool.getByRole('button', { name: 'Save subscription', exact: true }).click();
  await expect(pool.getByRole('button', { name: 'Edit or reconnect Claude fixture' })).toBeVisible();
  await pool.getByLabel('Key name', { exact: true }).fill('Coding fixture');
  await pool.getByLabel('claude-fixture', { exact: true }).check();
  await pool.getByRole('button', { name: 'Issue pool API key', exact: true }).click();
  const token = await page.getByLabel('API key', { exact: true }).inputValue();
  expect(token).toMatch(/^mp_pool_/);
  const models = await page.request.get(`${base}/pool/v1/models`, { headers: { Authorization: `Bearer ${token}` } });
  expect(models.status()).toBe(200);
  expect((await models.json()).data.map(m => m.id)).toEqual(['claude-fixture']);
  await page.getByRole('button', { name: 'Dismiss key', exact: true }).click();

  for (const width of [1440, 390]) {
    await page.setViewportSize({ width, height: 950 });
    for (let theme = 0; theme < 2; theme++) {
      const axe = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
      expect(axe.violations).toEqual([]);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      await page.getByRole('button', { name: 'Toggle theme' }).click();
    }
  }
  await page.setViewportSize({ width: 1440, height: 950 });
  await pool.getByRole('button', { name: 'Revoke Coding fixture', exact: true }).click();
  await expect(pool.getByText('Coding fixture · Revoked')).toBeVisible();
  const denied = await page.request.get(`${base}/pool/v1/models`, { headers: { Authorization: `Bearer ${token}` } });
  expect(denied.status()).toBe(401);
  await pool.getByRole('button', { name: 'Disconnect Claude fixture', exact: true }).click();
  await expect(pool.getByText('No subscriptions connected.', { exact: false })).toBeVisible();
});
