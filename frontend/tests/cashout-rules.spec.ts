import { test, expect } from '@playwright/test';

test.describe('Rules modal - special cards and cashout', () => {
  test.beforeEach(async ({ page }) => {
    // Fresh state so the client config is always fetched (and mockable).
    await page.addInitScript(() => localStorage.clear());

    // Mock the client config (PayPal donate URL is configurable and mockable).
    await page.route('**/api/config', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          paypal_donate_url: 'https://paypal-mock.example.com/donate',
          bot_thinking_delay_ms: 800,
          round_pause_delay_ms: 2500,
        }),
      }),
    );

    await page.route('**/api/anonymous', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          games_allowed: 5,
          games_played: 2,
          total_wins: 1,
          credits: 100,
        }),
      }),
    );
  });

  test('the rules modal exposes the special cards and cashout sections', async ({
    page,
  }) => {
    await page.goto('/');

    await page.getByRole('button', { name: 'Rules' }).click();

    await expect(page.getByRole('heading', { name: 'Special Cards' })).toBeVisible();
    await expect(page.getByText('Triple Seven — three or more 7s in hand')).toBeVisible();
    await expect(
      page.getByText('Sum under 21 — the five card ranks add up to less than 21'),
    ).toBeVisible();
    await expect(page.getByText('A Square — four cards of the same rank')).toBeVisible();

    await expect(page.getByRole('heading', { name: 'Cashout' })).toBeVisible();
    await expect(
      page.getByText(
        '250 credits = 1 EUR; the amount must be a whole multiple of 250 credits (minimum 250).',
      ),
    ).toBeVisible();
    await expect(page.getByText('You provide a PayPal email to receive your payout.')).toBeVisible();
  });

  test('the PayPal donate link resolves from the mocked client config', async ({
    page,
  }) => {
    await page.goto('/');

    const paypalLink = page.locator('a[href="https://paypal-mock.example.com/donate"]').first();
    await expect(paypalLink).toBeVisible();
  });
});
