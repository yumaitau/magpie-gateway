// End-to-end checks against a deployed gateway. Skipped unless MAGPIE_LIVE_URL
// is set. ./magpie.sh uitest <client> sets everything:
//   MAGPIE_LIVE_URL       https://... from Terraform
//   MAGPIE_ADMIN_PASSWORD portal administrator
//   MAGPIE_KEY            a gateway key, MAGPIE_PERSON its name
//   MAGPIE_MODEL          a model the deployment routes to
const { test, expect } = require('@playwright/test');
const AxeBuilder = require('@axe-core/playwright').default;

const base = process.env.MAGPIE_LIVE_URL;
test.skip(!base, 'MAGPIE_LIVE_URL is not set');
test.describe.configure({ mode: 'serial' });

const person = process.env.MAGPIE_PERSON;
const model = process.env.MAGPIE_MODEL;

const tags = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

async function signIn(page) {
  await page.goto(`${base}/login`);
  await page.getByLabel('Username').fill('admin');
  await page.getByLabel('Password').fill(process.env.MAGPIE_ADMIN_PASSWORD);
  await page.getByRole('button', { name: 'Sign in as administrator' }).click();
  await expect(page).toHaveURL(/\/dashboard$/);
}

async function ask(request, content) {
  return request.post(`${base}/v1/messages`, {
    headers: {
      authorization: `Bearer ${process.env.MAGPIE_KEY}`,
      'anthropic-version': '2023-06-01',
    },
    data: { model, max_tokens: 5, messages: [{ role: 'user', content }] },
    timeout: 60000,
  });
}

async function setKillAgent(page, value) {
  await page.goto(`${base}/settings`);
  await page.locator('#kill_agent').fill(value);
  await page.locator('form[action="/settings"] button[type="submit"]').click();
  await expect(page.locator('#kill_agent')).toHaveValue(value);
}

test('login page offers the administrator form and passes axe', async ({ page }) => {
  await page.goto(`${base}/login`);
  await expect(page.getByRole('heading', { name: 'Administrator' })).toBeVisible();
  const results = await new AxeBuilder({ page }).withTags(tags).analyze();
  expect(results.violations, JSON.stringify(results.violations, null, 2)).toEqual([]);
});

test('a wrong password is refused', async ({ page }) => {
  await page.goto(`${base}/login`);
  await page.getByLabel('Username').fill('admin');
  await page.getByLabel('Password').fill('not-the-password-at-all');
  await page.getByRole('button', { name: 'Sign in as administrator' }).click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page).not.toHaveURL(/\/dashboard$/);
});

for (const path of ['/dashboard', '/agents', '/catalogue', '/providers', '/policies', '/dlp', '/audit', '/approvals', '/settings', '/access']) {
  test(`${path} loads for the administrator without example data and passes axe`, async ({ page }) => {
    await signIn(page);
    const response = await page.goto(`${base}${path}`);
    expect(response.status()).toBe(200);
    const html = await page.content();
    expect(html).not.toContain('Example data');
    expect(html).toContain('Break-glass session');
    const results = await new AxeBuilder({ page }).withTags(tags).analyze();
    expect(results.violations, JSON.stringify(results.violations, null, 2)).toEqual([]);
  });
}

test('providers and approvals show the deployment models in Australia', async ({ page }) => {
  await signIn(page);
  await page.goto(`${base}/providers`);
  await expect(page.locator('body')).toContainText(model);
  await expect(page.locator('body')).toContainText('ap-southeast-2');
  await page.goto(`${base}/approvals`);
  await expect(page.locator('body')).toContainText(model);
});

test('a request through the gateway appears in the audit log under its sender', async ({ page, request }) => {
  const reply = await ask(request, 'Reply with the word ready.');
  expect(reply.status(), await reply.text()).toBe(200);
  await signIn(page);
  await page.goto(`${base}/audit`);
  await expect(page.locator('body')).toContainText(person);
  await expect(page.locator('body')).toContainText(model);
});

test('the per-agent kill switch respects the deployment mode and is restored after testing', async ({ page, request }) => {
  await signIn(page);
  await page.goto(`${base}/settings`);
  const previous = await page.locator('#kill_agent').inputValue();
  const logOnly = await page.locator('#guardrail_mode').inputValue() === 'log-only';
  await setKillAgent(page, [previous, person].filter(Boolean).join(', '));
  try {
    const blocked = await ask(request, 'Reply with the word ready.');
    expect(blocked.status(), await blocked.text()).toBe(logOnly ? 200 : 400);
    if (logOnly) {
      await page.goto(`${base}/audit`);
      await expect(page.locator('body')).toContainText('Log-only: would block');
    } else {
      expect((await blocked.json()).error.message).toContain('Magpie Gateway blocked');
    }
  } finally {
    await setKillAgent(page, previous);
  }
  const restored = await ask(request, 'Reply with the word ready.');
  expect(restored.status(), await restored.text()).toBe(200);
});

test('audit CSV export downloads for the administrator', async ({ page }) => {
  await signIn(page);
  const response = await page.request.get(`${base}/audit.csv`);
  expect(response.status()).toBe(200);
  expect(await response.text()).toContain(person);
});

test('log out ends the session', async ({ page }) => {
  await signIn(page);
  await page.getByRole('button', { name: 'Log out' }).click();
  const response = await page.goto(`${base}/dashboard`);
  await expect(page).toHaveURL(/\/login$/);
  expect(response.status()).toBe(200);
});

test('the mode selector saves the current choice without a deployment', async ({ page }) => {
  await signIn(page);
  await page.goto(`${base}/settings`);
  const mode = await page.getByLabel('Guardrail mode', { exact: true }).inputValue();
  await expect(page.getByLabel('Guardrail mode', { exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Apply mode', exact: true }).click();
  await expect(page.getByLabel('Guardrail mode', { exact: true })).toHaveValue(mode);
  await page.reload();
  await expect(page.getByLabel('Guardrail mode', { exact: true })).toHaveValue(mode);
});
