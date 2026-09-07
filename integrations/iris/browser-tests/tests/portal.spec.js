import { test, expect } from '@playwright/test';
async function login(page, name = 'Hunter', password = 'fixture-member-only') {
  await page.goto('/');
  await page.locator('#name').fill(name);
  await page.locator('#pw').fill(password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
}
test.beforeEach(async ({ request }) => { await request.post('/__reset'); });
test('wrong passwords cannot enter the room', async ({ page }) => {
  await login(page, 'Hunter', 'wrong-password');
  await expect(page.locator('#gateTrouble')).toContainText('not one this room knows');
  await expect(page.locator('#app')).toBeHidden();
});
test('a member cannot choose the owner identity', async ({ page, request }) => {
  await login(page, 'JJ');
  await expect(page.locator('#gateTrouble')).toContainText('belongs to Hunter');
  await expect(page.locator('#app')).toBeHidden();
  const response = await request.post('/people/let-in', {
    headers: { Authorization: 'Bearer fixture-member-only' }, data: { id: 1 },
  });
  expect(response.status()).toBe(403);
});
test('messages reach another browser and HTML stays literal', async ({ page, browser }) => {
  const other = await browser.newContext();
  try {
    const owner = await other.newPage();
    await login(owner, 'JJ', 'fixture-owner-only');
    await login(page);
    const text = '<img src=x onerror="window.injected=true"> browser test';
    await page.locator('#text').fill(text);
    await page.locator('#send button').click();
    await expect(owner.locator('.said .text')).toHaveText(' ' + text, { timeout: 10000 });
    await expect(owner.locator('.said .who')).toHaveText('Hunter');
    await expect(owner.locator('#room img')).toHaveCount(0);
    expect(await owner.evaluate(() => window.injected)).toBeUndefined();
  } finally { await other.close(); }
});
test('remembered login survives reload and sign out clears it', async ({ page }) => {
  await page.goto('/');
  await page.locator('#remember').check();
  await page.locator('#pw').fill('fixture-member-only');
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page.locator('#app')).toBeVisible();
  await page.reload();
  await expect(page.locator('#asWho')).toHaveText('signed in as Hunter');
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await page.reload();
  await expect(page.locator('#app')).toBeHidden();
  expect(await page.evaluate(() => localStorage.length + sessionStorage.length)).toBe(0);
});
test('the owner admits a new member through the browser', async ({ page, browser }) => {
  await page.goto('/');
  await page.getByRole('link', { name: 'Ask to join instead' }).click();
  await page.locator('#askName').fill('Browser Guest');
  await page.locator('#askPw').fill('fixture-guest-only');
  await page.getByRole('button', { name: 'Send the request' }).click();
  await expect(page.locator('#gateTrouble')).toContainText('Asked. Sign in here');
  const context = await browser.newContext();
  try {
    const owner = await context.newPage();
    await login(owner, 'JJ', 'fixture-owner-only');
    await owner.locator('#doorBtn').click();
    await expect(owner.locator('.askedName')).toHaveText('Browser Guest');
    await owner.getByRole('button', { name: 'Let in', exact: true }).click();
    await expect(owner.locator('.askedName')).toHaveCount(0);
    await login(page, 'Browser Guest', 'fixture-guest-only');
    await expect(page.locator('#asWho')).toHaveText('signed in as Browser Guest');
    await expect(page.locator('#doorBtn')).toBeHidden();
  } finally { await context.close(); }
});
test('overlapping polls display each message once', async ({ page, request }) => {
  await request.post('/say', { headers: { Authorization: 'Bearer fixture-member-only' },
    data: { text: 'one stored message' } });
  let release;
  let started;
  const paused = new Promise(resolve => { started = resolve; });
  const resume = new Promise(resolve => { release = resolve; });
  let reads = 0;
  await page.route('**/read?after=0', async route => {
    reads++;
    const response = await route.fetch();
    if (reads === 1) { started(); await resume; }
    await route.fulfill({ response });
  });
  await login(page);
  await paused;
  // The interval makes another real request while the first response is delayed.
  await expect(page.locator('#room .said')).toHaveCount(1, { timeout: 10000 });
  const response = page.waitForResponse(r => r.url().includes('/read?after=0'));
  release();
  await (await response).finished();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await expect(page.locator('#room .said')).toHaveCount(1);
});
