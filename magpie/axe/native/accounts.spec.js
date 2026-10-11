const { test, expect } = require('@playwright/test');
const AxeBuilder = require('@axe-core/playwright').default;
const base = process.env.MAGPIE_NATIVE_URL;
test.skip(!base, 'MAGPIE_NATIVE_URL is not set');
test.describe.configure({ mode: 'serial' });
const adminPassword = process.env.MAGPIE_NATIVE_PASSWORD || 'local-preview-passphrase-12345';
const tags = ['wcag2a','wcag2aa','wcag21a','wcag21aa','wcag22aa'];
async function login(page,username='admin',password=adminPassword){await page.goto(`${base}/ui/login`);await page.getByLabel('Email or username').fill(username);await page.getByLabel('Password',{exact:true}).fill(password);await page.getByRole('button',{name:'Sign in',exact:true}).click();await expect(page).not.toHaveURL(/\/login$/);}

test('all governance screens stay in the native shell and pass accessibility checks',async({page})=>{
 await login(page);
 for(const path of ['dashboard','agents','catalogue','providers','policies','dlp','audit','approvals','settings','access','australia']){
  await page.goto(`${base}/${path}`);await expect(page).toHaveURL(`${base}/ui/governance/${path}`);await expect(page.locator('.magpie-governance')).not.toBeEmpty();await expect(page.locator('.app-shell')).toHaveCount(1);await expect(page.locator('.shell')).toHaveCount(0);
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
 }
 await page.goto(`${base}/ui/governance/dlp`);await page.getByLabel('Text to check').fill('TFN 12345678');await page.getByRole('button',{name:'Run DLP',exact:true}).click();await expect(page.getByRole('heading',{name:'Sanitised preview'})).toBeVisible();await expect(page.locator('main pre')).not.toContainText('12345678');await expect(page).toHaveURL(/\/ui\/governance\/dlp$/);
 await page.goto(`${base}/ui/governance/access`);
 const suffix=String(Date.now());const roleName=`Trial readers ${suffix}`;const groupName=`Trial team ${suffix}`;
 const role=page.locator('details').filter({has:page.getByText('Create a role',{exact:true})});await role.locator('summary').click();await role.getByLabel('Role ID',{exact:true}).fill(`trial-${suffix}`);await role.getByLabel('Role name',{exact:true}).fill(roleName);await role.getByLabel('View dashboard',{exact:true}).check();await role.getByRole('button',{name:'Save role',exact:true}).click();await expect(page.locator('summary').filter({hasText:roleName})).toBeVisible();
 const group=page.locator('details').filter({has:page.getByText('Create a user group',{exact:true})});await group.locator('summary').click();await group.getByLabel('Group ID',{exact:true}).fill(`trial-team-${suffix}`);await group.getByLabel('Group name',{exact:true}).fill(groupName);await group.getByLabel(roleName,{exact:true}).check();await group.getByRole('button',{name:'Save group',exact:true}).click();await expect(page.locator('summary').filter({hasText:groupName})).toBeVisible();
 await page.goto(`${base}/ui/governance/settings`);const mode=await page.getByLabel('Guardrail mode',{exact:true}).inputValue();await page.getByRole('button',{name:'Apply mode',exact:true}).click();await expect(page.locator('.magpie-governance')).toHaveAttribute('aria-busy','false');await page.reload();await expect(page.getByLabel('Guardrail mode',{exact:true})).toHaveValue(mode);
});

test('invite, accept, sign in, change password and recover within one UI',async({page,browser})=>{
 await login(page);await page.getByRole('button',{name:/^Account:/}).click();await page.getByRole('link',{name:'Invite colleague',exact:true}).click();await expect(page).toHaveURL(`${base}/ui/people`);
 const email=`colleague-${Date.now()}@example.com.au`;const password='colleague-initial-passphrase-123';const next='colleague-updated-passphrase-456';
 await page.getByLabel('Colleague’s name').fill('Test Colleague');await page.getByLabel('Email address',{exact:true}).fill(email);await page.getByLabel('Viewer group',{exact:false}).check();await page.getByRole('button',{name:'Create invitation',exact:true}).click();await expect(page.getByLabel('Private account link')).toBeVisible();const invitation=await page.getByLabel('Private account link').inputValue();
 const context=await browser.newContext();const colleague=await context.newPage();await colleague.goto(invitation);await colleague.getByLabel('New password',{exact:true}).fill(password);await colleague.getByLabel('Confirm new password',{exact:true}).fill(password);await colleague.getByRole('button',{name:'Set password',exact:true}).click();await expect(colleague.getByText('Your password has been saved. Sign in to continue.')).toBeVisible();await login(colleague,email,password);await expect(colleague).toHaveURL(`${base}/ui/account`);
 await expect(colleague.getByRole('link',{name:'People',exact:true})).toHaveCount(0);expect((await colleague.request.get(`${base}/api/config`)).status()).toBe(403);expect((await colleague.request.get(`${base}/api/magpie/people`)).status()).toBe(403);
 await colleague.getByLabel('Current password',{exact:true}).fill(password);await colleague.getByLabel('New password',{exact:true}).fill(next);await colleague.getByLabel('Confirm new password',{exact:true}).fill(next);await colleague.getByRole('button',{name:'Change password',exact:true}).click();await expect(colleague).toHaveURL(/passwordChanged=1/);await login(colleague,email,next);
 await colleague.goto(invitation);await colleague.getByLabel('New password',{exact:true}).fill(password);await colleague.getByLabel('Confirm new password',{exact:true}).fill(password);await colleague.getByRole('button',{name:'Set password',exact:true}).click();await expect(colleague.getByRole('alert')).toContainText('already used');
 await page.reload();const person=page.getByRole('row').filter({hasText:email}).first();await person.getByRole('button',{name:'Create password reset link',exact:true}).click();const reset=await page.getByLabel('Private account link').inputValue();await colleague.goto(reset);await colleague.getByLabel('New password',{exact:true}).fill(password);await colleague.getByLabel('Confirm new password',{exact:true}).fill(password);await colleague.getByRole('button',{name:'Set password',exact:true}).click();await expect(colleague.getByText('Your password has been saved. Sign in to continue.')).toBeVisible();await login(colleague,email,password);
 await page.reload();const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations).toEqual([]);await context.close();
});

test('account and people navigation works on phones and in both themes',async({page})=>{
 await login(page);
 for(const path of ['account','people']) {await page.goto(`${base}/ui/${path}`);await expect(page.locator('main.content')).toContainText(path==='account'?'Change password':'Invite colleague');for(const width of [1440,390]){await page.setViewportSize({width,height:900});for(let i=0;i<2;i++){const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);await page.getByRole('button',{name:'Toggle theme',exact:true}).click();}}}
});
