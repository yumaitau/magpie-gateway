const { test, expect } = require('@playwright/test');
const AxeBuilder = require('@axe-core/playwright').default;
const { spawn } = require('node:child_process');
const path = require('node:path');

let portal;
let base;
test.beforeAll(async () => {
  portal = spawn(path.resolve(__dirname, '../target/debug/magpie-portal'), [], {
    env: { ...process.env, MAGPIE_DEMO: '0', MAGPIE_MOCK_IDP: '1', MAGPIE_SEED_EXAMPLES: '1',
      MAGPIE_DATABASE_URL: ':memory:', MAGPIE_SESSION_SECRET: 'test-access-control-only',
      MAGPIE_LISTEN: '127.0.0.1:0', MAGPIE_COOKIE_SECURE: '0', MAGPIE_GUARDRAIL_MODE: 'log-only' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  base = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Portal did not start')), 15000);
    portal.once('error', reject);
    portal.once('exit', code => { clearTimeout(timer); reject(new Error(`Portal exited: ${code}`)); });
    portal.stdout.on('data', chunk => {
      const match = chunk.toString().match(/listening on (127\.0\.0\.1:\d+)/);
      if (match) { clearTimeout(timer); resolve(`http://${match[1]}`); }
    });
  });
});
test.afterAll(() => portal?.kill());

async function login(page, name) {
  await page.goto(`${base}/login`);
  await page.getByRole('link', { name: 'Sign in with single sign-on' }).click();
  await page.getByRole('button', { name: `Continue as ${name}` }).click();
  await expect(page).toHaveURL(`${base}/dashboard`);
}

test('custom role and group delegate enforcement, persist the mode, and revoke the grant', async ({ page, browser }) => {
  const user = await browser.newPage();
  try {
    await login(user, 'Alex Nguyen');
    await login(page, 'Sam Patel');
    await user.goto(`${base}/settings`);
    await expect(user.getByLabel('Guardrail mode', { exact: true })).toBeDisabled();
    await expect(user.getByRole('link', { name: 'Access control', exact: true })).toHaveCount(0);
    await page.goto(`${base}/access`);
    const newRole = page.locator('details').filter({ has: page.getByText('Create a role', { exact: true }) });
    await newRole.locator('summary').click();
    await newRole.getByLabel('Role ID', { exact: true }).fill('trial-operator');
    await newRole.getByLabel('Role name', { exact: true }).fill('Trial operator');
    await newRole.getByLabel('View settings', { exact: true }).check();
    await newRole.getByLabel('Change enforcement mode', { exact: true }).check();
    await newRole.getByRole('button', { name: 'Save role', exact: true }).click();
    const newGroup = page.locator('details').filter({ has: page.getByText('Create a user group', { exact: true }) });
    await newGroup.locator('summary').click();
    await newGroup.getByLabel('Group ID', { exact: true }).fill('trial-team');
    await newGroup.getByLabel('Group name', { exact: true }).fill('Trial team');
    await newGroup.getByRole('checkbox', { name: 'Trial operator', exact: true }).check();
    await newGroup.getByRole('checkbox', { name: /Alex Nguyen/ }).check();
    await newGroup.getByRole('button', { name: 'Save group', exact: true }).click();
    for (const details of await page.locator('details').all()) await details.locator('summary').click();
    const accessibility = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa']).analyze();
    expect(accessibility.violations, JSON.stringify(accessibility.violations, null, 2)).toEqual([]);
    await user.reload();
    await expect(user.getByLabel('Guardrail mode', { exact: true })).toBeEnabled();
    await expect(user.getByRole('button', { name: 'Save settings', exact: true })).toBeDisabled();
    for (const mode of ['enforce', 'log-only']) {
      await user.getByLabel('Guardrail mode', { exact: true }).selectOption(mode);
      await user.getByRole('button', { name: 'Apply mode', exact: true }).click();
      await user.reload();
      await expect(user.getByLabel('Guardrail mode', { exact: true })).toHaveValue(mode);
      const response = await user.request.post(`${base}/guardrails/request`, {
        headers: { 'x-magpie-region': 'ap-southeast-2', 'x-magpie-destination': 'example-au-model', 'x-magpie-model': 'example-au-model' },
        data: { body: { messages: [{ role: 'user', content: 'ABN 12 000 000 041' }] } },
      });
      const decision = await response.json();
      expect(Boolean(decision.action.body)).toBe(mode === 'enforce');
    }
    const role = page.locator('details').filter({ has: page.locator('summary', { hasText: /^Trial operator$/ }) });
    await role.getByLabel('Change enforcement mode', { exact: true }).uncheck();
    await role.getByRole('button', { name: 'Save role', exact: true }).click();
    await user.reload();
    await expect(user.getByLabel('Guardrail mode', { exact: true })).toBeDisabled();
  } finally { await user.close(); }
});
