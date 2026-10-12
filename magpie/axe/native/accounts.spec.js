const { test, expect } = require('@playwright/test');
const AxeBuilder = require('@axe-core/playwright').default;
const base = process.env.MAGPIE_NATIVE_URL;
test.skip(!base, 'MAGPIE_NATIVE_URL is not set');
test.describe.configure({ mode: 'serial' });
const adminPassword = process.env.MAGPIE_NATIVE_PASSWORD || 'local-preview-passphrase-12345';
const tags = ['wcag2a','wcag2aa','wcag21a','wcag21aa','wcag22aa'];
async function login(page,username='admin',password=adminPassword){await page.goto(`${base}/ui/login`);await page.getByLabel('Email or username').fill(username);await page.getByLabel('Password',{exact:true}).fill(password);const [response]=await Promise.all([page.waitForResponse(r=>r.url()===`${base}/api/magpie/login`&&r.request().method()==='POST'),page.getByRole('button',{name:'Sign in',exact:true}).click()]);expect(response.ok()).toBe(true);await expect(page).not.toHaveURL(/\/login$/);}

test('signed-in users skip login on direct visits and client navigation',async({page})=>{
 await login(page);
 for(const path of ['/login','/ui/login','/ui/login?passwordChanged=1','/ui/login/']) {
  await page.goto(`${base}${path}`);
  await expect(page).toHaveURL(`${base}/ui/`);
  await expect(page.getByLabel('Email or username')).toHaveCount(0);
 }
 await page.goto(`${base}/ui/reset-password`);
 await page.getByRole('link',{name:'Back to sign in',exact:true}).click();
 await expect(page).toHaveURL(`${base}/ui/`);
 await expect(page.getByLabel('Email or username')).toHaveCount(0);
 await page.request.post(`${base}/api/auth/logout`);
 await page.goto(`${base}/login`);
 await expect(page).toHaveURL(`${base}/ui/login`);
 await expect(page.getByLabel('Email or username')).toBeVisible();
});

test('all governance screens stay in the native shell and pass accessibility checks',async({page})=>{
 await login(page);
 for(const path of ['controls','dashboard','alerts','agents','catalogue','providers','policies','dlp','audit','approvals','settings','access','australia']){
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
 for(const path of ['/login','/ui/login']) {await colleague.goto(`${base}${path}`);await expect(colleague).toHaveURL(`${base}/ui/account`);await expect(colleague.getByLabel('Email or username')).toHaveCount(0);}
 await colleague.getByLabel('Current password',{exact:true}).fill(password);await colleague.getByLabel('New password',{exact:true}).fill(next);await colleague.getByLabel('Confirm new password',{exact:true}).fill(next);await colleague.getByRole('button',{name:'Change password',exact:true}).click();await expect(colleague).toHaveURL(/passwordChanged=1/);await login(colleague,email,next);
 await colleague.goto(invitation);await colleague.getByLabel('New password',{exact:true}).fill(password);await colleague.getByLabel('Confirm new password',{exact:true}).fill(password);await colleague.getByRole('button',{name:'Set password',exact:true}).click();await expect(colleague.getByRole('alert')).toContainText('already used');
 await page.reload();const person=page.getByRole('row').filter({hasText:email}).first();await person.getByRole('button',{name:'Create password reset link',exact:true}).click();const reset=await page.getByLabel('Private account link').inputValue();await colleague.goto(reset);await colleague.getByLabel('New password',{exact:true}).fill(password);await colleague.getByLabel('Confirm new password',{exact:true}).fill(password);await colleague.getByRole('button',{name:'Set password',exact:true}).click();await expect(colleague.getByText('Your password has been saved. Sign in to continue.')).toBeVisible();await login(colleague,email,password);
 await page.reload();const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations).toEqual([]);await context.close();
});

test('account and people navigation works on phones and in both themes',async({page})=>{
 await login(page);
 for(const path of ['account','people']) {await page.goto(`${base}/ui/${path}`);await expect(page.locator('main.content')).toContainText(path==='account'?'Change password':'Invite colleague');for(const width of [1440,390]){await page.setViewportSize({width,height:900});for(let i=0;i<2;i++){const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);await page.getByRole('button',{name:'Toggle theme',exact:true}).click();}}}
});


test('home has no artwork banner and alert destinations save without exposing secrets', async ({page}) => {
 await login(page);
 await page.goto(`${base}/ui/`);
 await expect(page.locator('.magpie-welcome')).toHaveCount(0);
 await page.goto(`${base}/ui/governance/alerts`);
 await expect(page.getByLabel('Email recipient (email only)',{exact:true})).not.toBeVisible();
 await page.getByLabel('Send to',{exact:true}).selectOption('email');
 await expect(page.getByLabel('Webhook URL (Slack, Teams or webhook)',{exact:true})).not.toBeVisible();
 await expect(page.getByLabel('Email recipient (email only)',{exact:true})).toBeVisible();
 await page.getByText('Email and advanced settings',{exact:true}).click();
 await expect(page.getByLabel('SMTP host',{exact:true})).toBeVisible();
 await page.getByLabel('Send to',{exact:true}).selectOption('slack');
 await expect(page.getByLabel('SMTP host',{exact:true})).not.toBeVisible();
 for(const width of [1440,390]) {
  await page.setViewportSize({width,height:900});
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 }
 await page.setViewportSize({width:1440,height:900});
 if(process.env.MAGPIE_SCREENSHOTS) await page.screenshot({path:`${process.env.MAGPIE_SCREENSHOTS}/alerts.png`,fullPage:true});
 await page.getByLabel('Destination name',{exact:true}).fill('Local UI test');
 await page.getByLabel('Webhook URL (Slack, Teams or webhook)',{exact:true}).fill('https://example.com/private-ui-test');
 await page.getByRole('button',{name:'Save destination',exact:true}).click();
 const destination=page.locator('details').filter({has:page.locator('summary').filter({hasText:'Local UI test · slack · Off'})}).first();
 await expect(destination).toBeVisible();
 expect(await page.locator('main').innerHTML()).not.toContain('private-ui-test');
 await destination.locator('summary').first().click();
 await destination.getByRole('button',{name:'Delete destination',exact:true}).click();
 await expect(page.getByText('No destinations yet. Add Slack, Teams, a webhook or email below.')).toBeVisible();
});


test('controls explain effective coverage and disabling creates a pending revision',async({page})=>{
 await login(page);
 await page.goto(`${base}/ui/governance/settings`);
 await page.getByLabel('Guardrail mode',{exact:true}).selectOption('log-only');
 await page.getByRole('button',{name:'Apply mode',exact:true}).click();
 await expect(page.locator('.magpie-governance')).toHaveAttribute('aria-busy','false');
 await page.goto(`${base}/ui/governance/controls`);
 await expect(page.getByRole('heading',{name:'Gateway mode: Observe only'})).toBeVisible();
 await page.getByText('See all eight strategies and their owners',{exact:true}).click();
 await expect(page.getByRole('rowheader',{name:'Regular backups',exact:true})).toBeVisible();
 for(const width of [1440,390]) {
  await page.setViewportSize({width,height:900});
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 }
 await page.setViewportSize({width:1440,height:900});
 if(process.env.MAGPIE_SCREENSHOTS) await page.screenshot({path:`${process.env.MAGPIE_SCREENSHOTS}/controls.png`,fullPage:true});
 await page.getByRole('link',{name:'Edit, disable or restore detectors',exact:true}).first().click();
 const row=page.getByRole('row').filter({has:page.getByRole('cell',{name:'email',exact:true})});
 await row.getByRole('button',{name:'Disable detector',exact:true}).click();
 await expect(page.getByText('Change saved for approval.',{exact:true})).toBeVisible();
 await expect(row).toContainText('Observe only');
 await expect(page.locator('section[aria-label="Change approvals"]')).toContainText('default-email');
 await expect(page.locator('section[aria-label="Change approvals"]')).toContainText('enabled');
});

test('help guides explain setup and options with contextual navigation',async({page})=>{
 await login(page);
 await page.getByRole('link',{name:'Getting started & help',exact:true}).click();
 await expect(page.getByRole('heading',{name:'Get your first model working',exact:true})).toBeVisible();
 for(const topic of ['start','model-sync','clients','dlp','controls','alerts','people','mcp','troubleshooting']) {
  await page.goto(`${base}/ui/help/${topic}`);
  await expect(page.locator('.help-guide li').first()).toBeVisible();
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
 }
 await page.getByLabel('Search guides and options').fill('SMTP');
 await expect(page.getByText('SMTP / sender / recipient',{exact:true})).toBeVisible();
 await expect(page.getByText('No matches. Try a detector name, screen name or setting.')).toHaveCount(0);
 await page.goto(`${base}/ui/dlp-tuning`);
 await page.getByRole('link',{name:'Help',exact:true}).click();
 await expect(page).toHaveURL(`${base}/ui/help/dlp`);
 await page.setViewportSize({width:390,height:900});
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
 if(process.env.MAGPIE_SCREENSHOTS) await page.screenshot({path:`${process.env.MAGPIE_SCREENSHOTS}/help-mobile.png`,fullPage:true});
});

test('DLP tuning compares an exact exception without saving the sample',async({page})=>{
 await login(page);
 const decision=await page.request.post(`${base}/mcp/check`,{data:{method:'tools/call',tool:null,model:null,agentId:'ui-test',destinationRegion:'au',destinationName:'local-test',phase:'request',actor:'ui-test',resultText:'fixture@example.com'}});expect(decision.ok()).toBe(true);
 await page.goto(`${base}/ui/governance/audit`);
 await page.getByRole('button',{name:'Tune email',exact:true}).first().click();
 await expect(page.getByLabel('1. Choose a detector',{exact:true})).toHaveValue('email');
 await page.getByLabel('1. Choose a detector',{exact:true}).selectOption('phone');
 await page.getByText('Exact-value exceptions and advanced options',{exact:true}).click();
 await page.getByLabel('Exact value to ignore',{exact:true}).fill('0412 345 678');
 await page.getByLabel('Exception label',{exact:true}).fill('Synthetic phone fixture');
 await page.getByLabel('Synthetic sample text',{exact:true}).fill('Phone 0412 345 678');
 await page.getByRole('button',{name:'Compare active and draft',exact:true}).click();
 await expect(page.getByRole('heading',{name:'Comparison',exact:true})).toBeVisible();
 await expect(page.locator('.dlp-tuner')).toContainText('Draft DLP: allow');
 await expect(page.locator('.dlp-tuner pre')).not.toContainText('0412 345 678');
 for(const width of [1440,390]) {
  await page.setViewportSize({width,height:900});
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 }
 if(process.env.MAGPIE_SCREENSHOTS) await page.screenshot({path:`${process.env.MAGPIE_SCREENSHOTS}/tuning-mobile.png`,fullPage:true});
 await page.setViewportSize({width:1440,height:900});
 if(process.env.MAGPIE_SCREENSHOTS) await page.screenshot({path:`${process.env.MAGPIE_SCREENSHOTS}/tuning-desktop.png`,fullPage:true});
 await page.getByRole('button',{name:'Save for approval',exact:true}).click();
 await expect(page.getByText('Saved for approval. The active rule has not changed. A different authorised person must approve it.',{exact:true})).toBeVisible();
 await expect(page.getByRole('button',{name:'Save for approval',exact:true})).toBeDisabled();
 const result=await (await page.request.get(`${base}/api/magpie/dlp`)).json();
 expect(result.rules.find(r=>r.kind==='phone').exceptions).toEqual([]);
 expect(JSON.stringify(result)).not.toContain('0412 345 678');
});


test('synced models follow provider endpoints and remain manually selectable', async ({page}) => {
 await login(page);
 const config={llm:{models:[
  {name:'anthropic/claude-opus-5-5',provider:'anthropic',params:{model:'claude-opus-5-5'}},
  {name:'anthropic/*',provider:'anthropic'},
  {name:'codex/*',provider:'openai'},
  {name:'grok/*',provider:{reference:'grok'},params:{}},
  {name:'zen/*',provider:{custom:{formats:[{type:'completions'}]}},params:{baseUrl:'https://opencode.ai/zen/v1'}},
  {name:'custom/*',provider:{custom:{formats:[{type:'completions'}]}},params:{baseUrl:'https://example.com/v1'}}
 ],providers:[{name:'grok',provider:'openai',params:{baseUrl:'https://api.x.ai/v1'}}]},binds:[]};
 await page.route('**/api/config',r=>r.fulfill({json:config}));
 await page.route('**/api/config/effective',r=>r.fulfill({json:config}));
 const catalog={syncedAt:'2026-10-12T00:00:00Z',intervalSeconds:3600,canSync:true,error:null,providers:[
  {provider:'anthropic',name:'Anthropic',models:['claude-opus-5-5']},
  {provider:'openai',name:'OpenAI',models:['codex-new-fixture','gpt-6.1-sol']},
  {provider:'xai',name:'xAI',models:['grok-new-fixture']},
  {provider:'opencode',name:'OpenCode',models:['zen-new-fixture']}
 ]};
 await page.route('**/api/magpie/model-catalog',r=>r.fulfill({json:catalog}));
 await page.goto(`${base}/ui/llm/models`);
 await expect(page.getByRole('button',{name:'All catalog models',exact:true})).toHaveAttribute('aria-pressed','true');
 const browse=page.getByRole('region',{name:'Browse model catalog',exact:true});
 await page.getByLabel('Search all models',{exact:true}).fill('Opus 5.5');
 await expect(browse.getByRole('cell',{name:'claude-opus-5-5',exact:true})).toBeVisible();
 await browse.getByRole('button',{name:'Configure claude-opus-5-5 via Anthropic',exact:true}).click();
 const drawer=page.getByRole('dialog',{name:'Add model',exact:true});
 await drawer.getByText('Model routing',{exact:false}).first().click();
 await expect(drawer.getByLabel('Incoming model match',{exact:false})).toHaveValue('anthropic/claude-opus-5-5-2');
 await expect(drawer.getByLabel('Explicit outgoing model',{exact:true})).toHaveValue('claude-opus-5-5');
 await drawer.getByRole('button',{name:'Close',exact:true}).click();
 await page.getByLabel('Search all models',{exact:true}).fill('Codex');
 await expect(browse.getByRole('cell',{name:'gpt-6.1-sol',exact:true})).toBeVisible();
 await page.getByLabel('Search all models',{exact:true}).fill('');
 await browse.getByLabel('Catalog provider',{exact:true}).selectOption('opencode');
 await expect(browse.getByRole('cell',{name:'zen-new-fixture',exact:true})).toBeVisible();
 await expect(browse.getByRole('cell',{name:'claude-opus-5-5',exact:true})).toHaveCount(0);
 for(const width of [1440,390]) {
  await page.setViewportSize({width,height:900});
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 }
 await page.setViewportSize({width:1440,height:900});
 await page.getByRole('button',{name:'Configured routes (6)',exact:true}).click();
 await expect(page.getByRole('cell',{name:'anthropic/*',exact:true})).toBeVisible();
 await page.getByRole('button',{name:'All catalog models',exact:true}).click();
 await page.getByText('Model catalog · Auto-sync hourly',{exact:true}).click();
 await expect(page.getByText(/Last successful sync:/)).toBeVisible();
 await page.getByRole('button',{name:'Sync now',exact:true}).click();
 await expect(page.getByRole('button',{name:'Sync now',exact:true})).toBeEnabled();
 for(const [route,model] of [['anthropic/*','claude-opus-5-5'],['codex/*','codex-new-fixture'],['grok/*','grok-new-fixture'],['zen/*','zen-new-fixture']]) {
  await page.goto(`${base}/ui/llm/client-setup`);
  await page.getByRole('combobox',{name:'Model',exact:true}).click();
  await page.getByRole('option',{name:route,exact:true}).click();
  const input=page.getByRole('textbox',{name:'Specific model',exact:true});
  await input.fill(model);
  await expect(page.getByRole('option',{name:model,exact:true})).toBeVisible();
  await page.getByRole('option',{name:model,exact:true}).click();
  await expect(input).toHaveValue(model);
 }
 await page.getByRole('combobox',{name:'Model',exact:true}).click();
 await page.getByRole('option',{name:'custom/*',exact:true}).click();
 await page.getByLabel('Specific model catalog provider',{exact:true}).selectOption('xai');
 const input=page.getByRole('textbox',{name:'Specific model',exact:true});
 await input.fill('grok');
 await expect(page.getByRole('option',{name:'grok-new-fixture',exact:true})).toBeVisible();
 await input.fill('private-model-not-in-catalog');
 await input.press('Escape');
 await expect(input).toHaveValue('private-model-not-in-catalog');
 for(const width of [1440,390]) {
  await page.setViewportSize({width,height:900});
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 }
});


test('playground searches all models and getting started downloads a secret-free setup kit', async ({page}) => {
 await login(page);
 const config={llm:{models:[
  {name:'claude-friendly',provider:'anthropic',params:{model:'claude-opus-5-5'}},
  {name:'codex/*',provider:'openai'},
  {name:'bedrock-friendly',provider:'bedrock',params:{model:'bedrock-opus-fixture'}}
 ],keys:[{name:'private-fixture-key',key:'never-export-this-secret'}]},binds:[]};
 await page.route('**/api/config',r=>r.fulfill({json:config}));
 await page.route('**/api/config/effective',r=>r.fulfill({json:config}));
 const catalog={syncedAt:'2026-10-12T00:00:00Z',intervalSeconds:3600,canSync:true,error:null,providers:[
  {provider:'anthropic',name:'Anthropic',models:['claude-opus-5-5']},
  {provider:'openai',name:'OpenAI',models:['gpt-new-fixture']},
  {provider:'xai',name:'xAI',models:['grok-unconfigured-fixture']},
  {provider:'aws.bedrock',name:'Amazon Bedrock',models:['bedrock-opus-fixture']},
  {provider:'opencode',name:'OpenCode',models:Array.from({length:130},(_,i)=>`zen-fixture-${i}`)}
 ]};
 await page.route('**/api/magpie/model-catalog',r=>r.fulfill({json:catalog}));
 await page.goto(`${base}/ui/llm/playground`);
 async function selectCatalog(search,option,label='Model') {
  await page.getByRole('combobox',{name:label,exact:true}).first().click();
  await page.getByRole('combobox',{name:`Search ${label}`,exact:true}).fill(search);
  await page.getByRole('option',{name:option}).click();
 }
 await selectCatalog('Opus 5.5',/^claude-opus-5-5/);
 await expect(page.getByRole('combobox',{name:'Model',exact:true})).toContainText('claude-friendly');
 await selectCatalog('Codex',/^gpt-new-fixture/);
 await expect(page.getByRole('textbox',{name:'Specific model',exact:true})).toHaveValue('gpt-new-fixture');
 let sentModel='';
 await page.route('**/v1/chat/completions',r=> { sentModel=r.request().postDataJSON().model; return r.fulfill({json:{choices:[{message:{role:'assistant',content:'ready'}}]}}); });
 await page.getByRole('textbox',{name:'User message',exact:true}).fill('Reply ready');
 await page.getByRole('button',{name:'Send',exact:true}).click();
 await expect(page.locator('.chat-message.assistant')).toContainText('ready');
 expect(sentModel).toBe('codex/gpt-new-fixture');
 await selectCatalog('zen-fixture-129',/^zen-fixture-129/);
 await expect(page.getByRole('link',{name:'Set up this model'})).toBeVisible();
 await selectCatalog('grok',/^grok-unconfigured-fixture/);
 await expect(page.getByText('Model responded successfully. You can now connect your app.',{exact:true})).toHaveCount(0);
 await page.getByRole('textbox',{name:'User message',exact:true}).fill('Reply ready');
 await expect(page.getByRole('button',{name:'Send',exact:true})).toBeDisabled();
 await expect(page.getByRole('button',{name:'Test connection',exact:true})).toBeDisabled();
 for(const width of [1440,390]) {
  await page.setViewportSize({width,height:900});
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 }
 await page.getByRole('link',{name:'Set up this model'}).click();
 const drawer=page.getByRole('dialog',{name:'Add model',exact:true});
 await expect(drawer).toContainText('grok-unconfigured-fixture');
 await page.goto(`${base}/ui/llm/client-setup`);
 await selectCatalog('grok',/^grok-unconfigured-fixture/);
 await expect(page.locator('.client-recipe-card')).toHaveCount(0);
 await expect(page.getByRole('button',{name: "I've connected my app",exact:true})).toHaveCount(0);
 await selectCatalog('Codex',/^gpt-new-fixture/);
 await expect(page.locator('.client-recipe-card')).toContainText('codex/gpt-new-fixture');
 await page.goto(`${base}/ui/llm/models#add=virtual`);
 const virtualDrawer=page.getByRole('dialog');
 await virtualDrawer.getByRole('textbox',{name:'Virtual model name',exact:false}).fill('catalog-test-virtual');
 for(const strategy of ['Weighted','Failover','Conditional']) {
  await virtualDrawer.getByRole('button',{name:strategy,exact:true}).click();
  const label=strategy==='Conditional'?'Target model':'Model';
  await selectCatalog('grok',/^grok-unconfigured-fixture/,label);
  await expect(virtualDrawer.getByRole('button',{name:'Save virtual model',exact:true})).toBeDisabled();
  await selectCatalog('Codex',/^gpt-new-fixture/,label);
  await expect(virtualDrawer.getByRole('textbox',{name:'Specific model',exact:true}).first()).toHaveValue('gpt-new-fixture');
 }
 await page.goto(`${base}/ui/help/start`);
 const prompt=page.getByRole('textbox',{name:'Bootstrap prompt',exact:true});
 await expect(prompt).toBeVisible();
 await expect(prompt).toContainText('Discover the models available to my gateway key');
 expect((await prompt.inputValue()).split(/\s+/).length).toBeLessThan(350);
 await expect(page.getByRole('button',{name:'Copy bootstrap prompt'})).toBeEnabled();
 await page.evaluate(()=>Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>{window.copiedBootstrap=text;}}}));
 await page.getByRole('button',{name:'Copy bootstrap prompt'}).click();
 expect(await page.evaluate(()=>window.copiedBootstrap)).toBe(await prompt.inputValue());
 await selectCatalog('grok',/^grok-unconfigured-fixture/);
 await expect(prompt).toContainText('grok-unconfigured-fixture');
 await expect(prompt).toContainText('no matching configured gateway route');
 await expect(page.getByRole('button',{name:'Copy bootstrap prompt'})).toBeEnabled();
 await selectCatalog('zen-fixture-129',/^zen-fixture-129/);
 await expect(prompt).toContainText('zen-fixture-129');
 await selectCatalog('bedrock-opus',/^bedrock-opus-fixture/);
 await expect(prompt).toContainText('Use gateway model "bedrock-friendly"');
 await selectCatalog('Codex',/^gpt-new-fixture/);
 await expect(prompt).toContainText('Use gateway model "codex/gpt-new-fixture"');
 await page.evaluate(()=>Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw new Error('blocked');}}}));
 await page.getByRole('button',{name:'Copy bootstrap prompt'}).click();
 await expect(page.getByRole('status')).toContainText('Prompt selected');
 expect(await prompt.evaluate(el=>el.selectionEnd-el.selectionStart)).toBe((await prompt.inputValue()).length);
 await page.getByText('Options and downloads',{exact:true}).click();
 await page.getByLabel('Client to configure').selectOption('Codex');
 await page.getByLabel('Gateway model name').fill('codex/gpt-new-fixture');
 await page.getByLabel('Gateway URL',{exact:true}).fill('https://gateway.example.test');
 for (const name of ['prompt.md','rollout.md']) {
  const pending=page.waitForEvent('download');
  await page.getByRole('button',{name:`Download ${name}`,exact:true}).click();
  const download=await pending;
  expect(download.suggestedFilename()).toBe(name);
  const text=require('node:fs').readFileSync(await download.path(),'utf8');
  expect(text).toContain('https://gateway.example.test');
  expect(text).not.toContain('never-export-this-secret');
  if(name==='prompt.md') { expect(text).toContain('codex/gpt-new-fixture'); expect(text).toContain('MAGPIE_API_KEY'); expect(text).toContain('wire_api = "responses"'); }
  else { expect(text).toContain('rollback'); expect(text).toContain('separate virtual API key'); }
 }
 for(const width of [1440,390]) {
  await page.setViewportSize({width,height:900});
  const axe=await new AxeBuilder({page}).withTags(tags).analyze();expect(axe.violations,JSON.stringify(axe.violations)).toEqual([]);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 }
 await page.getByLabel('Gateway URL',{exact:true}).fill('https://user:secret@example.test');
 await expect(page.getByRole('button',{name:'Download prompt.md',exact:true})).toBeDisabled();
});
