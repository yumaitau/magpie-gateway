import { expect, test } from '@playwright/test';

import { mockGateway } from './fixtures';

test('first model setup uses defaults and reveals the next steps after saving', async ({
	page
}) => {
	const gateway = await mockGateway(page, {});
	await page.goto('/');
	await expect(page.getByRole('heading', { name: 'Start with your first model' })).toBeVisible();
	await expect(page.getByRole('dialog')).toHaveCount(0);
	await expect(page.getByRole('link', { name: 'Try your model' })).toHaveCount(0);
	await page.getByRole('button', { name: 'Add your first model' }).click();
	await expect(page.getByRole('heading', { name: 'Add model', exact: true })).toBeVisible();
	await page.getByRole('combobox', { name: 'Provider', exact: true }).click();
	await page.getByRole('option', { name: 'OpenAI', exact: true }).click();
	await expect(page.getByRole('textbox', { name: 'Incoming model match' })).not.toBeVisible();
	await page.getByPlaceholder('sk-...').fill('sk-test-onboarding');
	await page.getByRole('button', { name: 'Save model', exact: true }).click();
	await expect.poll(() => gateway.postedConfigs.length).toBe(2);
	await page.getByRole('link', { name: 'Try your model', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'LLM Playground' })).toBeVisible();
	expect(gateway.postedConfigs[1].llm).toMatchObject({
		models: [
			{
				name: 'openai/*',
				provider: 'openai',
				params: { apiKey: 'sk-test-onboarding' },
				transformation: { model: 'llmRequest.model.stripPrefix("openai/")' }
			}
		]
	});
	await page.locator('.nav-list').getByRole('link', { name: 'Home', exact: true }).click();
	await expect(page.getByRole('link', { name: 'Try your model' })).toBeVisible();
	await page.getByRole('link', { name: 'Connect your app', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'Client Setup', exact: true })).toBeVisible();
});

test('advanced navigation is discoverable and opens for a direct link', async ({ page }) => {
	await mockGateway(page);
	await page.goto('/');
	const nav = page.locator('.nav-list');
	await expect(nav.getByRole('link', { name: 'Models', exact: true })).toBeVisible();
	await expect(nav.getByRole('link', { name: 'Raw Configuration' })).not.toBeVisible();
	await nav.locator('.advanced-navigation > summary').focus();
	await page.keyboard.press('Enter');
	await expect(nav.getByRole('link', { name: 'Raw Configuration' })).toBeVisible();
	await nav.getByRole('link', { name: 'Providers', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'LLM Providers' })).toBeVisible();
	await page.reload();
	await expect(nav.getByRole('link', { name: 'Providers', exact: true })).toBeVisible();
	await expect(nav.locator('.advanced-navigation')).toHaveAttribute('open', '');
});

test('mobile navigation keeps advanced pages accessible without horizontal overflow', async ({
	page
}) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await mockGateway(page);
	await page.goto('/');
	await expect(page.getByRole('link', { name: 'Try your model' })).toBeVisible();
	await page.locator('.mobile-nav-trigger').click();
	const nav = page.locator('.mobile-nav-menu');
	await expect(nav.getByRole('link', { name: 'Providers', exact: true })).not.toBeVisible();
	await nav.locator('.advanced-navigation > summary').click();
	await nav.getByRole('link', { name: 'Providers', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'LLM Providers' })).toBeVisible();
	await expect(nav).not.toBeVisible();
	expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(
		true
	);
});

test('failed gateway setup stays on the start page and reports the error', async ({ page }) => {
	await mockGateway(page, {});
	await page.route('**/api/config', route =>
		route.request().method() === 'POST'
			? route.fulfill({
					status: 500,
					contentType: 'text/plain',
					body: 'Unable to save configuration'
				})
			: route.fallback()
	);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add your first model' }).click();
	await expect(page.getByText('Save failed', { exact: true })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Add your first model' })).toBeEnabled();
	await expect(page).toHaveURL(/\/$/);
	await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('setup completes only after a successful model response and explicit app confirmation', async ({
	page
}) => {
	await mockGateway(page);
	await page.route('**/v1/chat/completions', route =>
		route.fulfill({ status: 401, body: 'Invalid provider credential' })
	);
	await page.goto('/llm/playground?model=fast');
	await page.getByRole('button', { name: 'Test connection', exact: true }).click();
	await expect(page.getByText('Invalid provider credential', { exact: true })).toBeVisible();
	expect(
		await page.evaluate(
			() => JSON.parse(localStorage.getItem('magpie.setup.v1') || '{}').testedModel
		)
	).toBeFalsy();
	await page.route('**/v1/chat/completions', route =>
		route.fulfill({ json: { choices: [{ message: { role: 'assistant', content: 'OK' } }] } })
	);
	await page.getByRole('button', { name: 'Test connection', exact: true }).click();
	await expect(
		page.getByText('Model responded successfully. You can now connect your app.')
	).toBeVisible();
	await page.goto('/llm/client-setup');
	await page.getByRole('button', { name: "I've connected my app" }).click();
	await page.goto('/');
	await expect(page.locator('.setup-checklist > summary')).toHaveText('Setup complete');
	await expect(page.locator('.setup-checklist')).not.toHaveAttribute('open', '');
	await page.reload();
	await expect(page.locator('.setup-checklist > summary')).toHaveText('Setup complete');
});

test('API key is immediately available while alternative credential sources stay accessible', async ({
	page
}) => {
	await mockGateway(page);
	await page.goto('/llm/models#add=model');
	await page.getByRole('combobox', { name: 'Provider', exact: true }).click();
	await page.getByRole('option', { name: 'OpenAI', exact: true }).click();
	await expect(page.getByPlaceholder('sk-...')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Env var', exact: true })).not.toBeVisible();
	await page.getByText('Advanced credentials', { exact: true }).click();
	await page.getByRole('button', { name: 'Env var', exact: true }).click();
	await page.getByPlaceholder('ENV_VAR_NAME').fill('OPENAI_API_KEY');
	await page.getByRole('button', { name: 'Save model', exact: true }).click();
	await expect(page.getByText('Configuration saved', { exact: true })).toBeVisible();
});
