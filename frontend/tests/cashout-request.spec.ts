import { test, expect } from '@playwright/test';

const USER_ID = 'user-1';

test.describe('Cashout request from profile', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.clear();
      const NoopWS = class {
        static CONNECTING = 0;
        static OPEN = 1;
        static CLOSING = 2;
        static CLOSED = 3;
        readyState = 3;
        onopen = null;
        onmessage = null;
        onclose = null;
        onerror = null;
        send() {}
        close() {}
      };
      (window as unknown as { WebSocket: unknown }).WebSocket = NoopWS;
    });

    await page.route('**/api/auth/me', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          id: USER_ID,
          pseudo: 'Tester',
          email: 'tester@example.com',
          language: 'en',
        }),
      }),
    );

    await page.route('**/api/config', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          paypal_donate_url: 'https://paypal.com/donate',
          bot_thinking_delay_ms: 800,
          round_pause_delay_ms: 2500,
          cashout_enabled: true,
          cashout_min_credits: 250,
          cashout_credits_per_eur: 250,
          cashout_max_eur_cents: 2000,
        }),
      }),
    );

    await page.route('**/api/me/profile', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          credit: 1000,
          game_played: 3,
          wins: 1,
          kora_wins: 0,
          frozen_until: null,
          cashout_locked: false,
        }),
      }),
    );

    await page.route('**/api/me/games**', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ games: [], total: 0, page: 1, per_page: 10 }),
      }),
    );

    await page.route('**/api/me/invitations', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ invitations: [] }),
      }),
    );

    await page.route('**/api/me/rooms', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([]),
      }),
    );

    await page.route(/\/api\/me\/cashout/, (route) => {
      if (route.request().method() === 'POST') {
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            id: 'cashout-1',
            credits: 250,
            amount_eur_cents: 100,
            status: 'requested',
          }),
        });
      }
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ items: [], total: 0, page: 1, per_page: 100 }),
      });
    });
  });

  test('requests a cashout from the profile', async ({ page }) => {
    await page.goto('/');

    await expect(
      page.getByRole('button', { name: 'Request Cashout' }),
    ).toBeVisible();
    await page.getByRole('button', { name: 'Request Cashout' }).click();

    await page.getByLabel('Credits to withdraw').fill('250');
    await page.getByLabel('PayPal email').fill('payer@example.com');
    await page.getByRole('button', { name: 'Submit request' }).click();

    await expect(page.getByText('Cashout request submitted')).toBeVisible();
  });

  test('shows the cashout history in the profile', async ({ page }) => {
    await page.route(/\/api\/me\/cashout/, (route) => {
      if (route.request().method() === 'POST') {
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            id: 'cashout-1',
            credits: 250,
            amount_eur_cents: 100,
            status: 'requested',
          }),
        });
      }
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          items: [
            {
              id: 'cashout-1',
              credits: 250,
              amount_eur_cents: 100,
              status: 'requested',
              paypal_email: 'payer@example.com',
              created_at: '2026-09-23T10:00:00Z',
              processed_at: null,
            },
          ],
          total: 1,
          page: 1,
          per_page: 100,
        }),
      });
    });

    await page.goto('/');

    await page.getByRole('button', { name: 'Cashout History' }).click();

    await expect(page.getByText('1.00 EUR')).toBeVisible();
    await expect(page.getByText('requested')).toBeVisible();
    await expect(page.getByText('payer@example.com')).toBeVisible();
  });
});
