import { test, expect } from '@playwright/test';

test('start a game and see the game table', async ({ page }) => {
  // Mock the API responses
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
    })
  );

  await page.route('**/api/quickie', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        game_id: 'test-game-id',
        players: [
          {
            id: 'player-human',
            type: 'human',
            name: 'You',
            position: 0,
            cards: [0, 1, 2, 3, 4],
          },
          {
            id: 'player-bot-1',
            type: 'bot',
            name: 'Bot 1',
            position: 1,
            cards: [],
          },
          {
            id: 'player-bot-2',
            type: 'bot',
            name: 'Bot 2',
            position: 2,
            cards: [],
          },
          {
            id: 'player-bot-3',
            type: 'bot',
            name: 'Bot 3',
            position: 3,
            cards: [],
          },
        ],
        status: 'playing',
        current_turn: 0,
        bet: 10,
      }),
    })
  );

  await page.goto('http://localhost:5173');

  // Wait for dashboard to load
  await expect(page.getByText('FapFap Card Game')).toBeVisible();

  // Click start game button
  await page.getByRole('button', { name: 'Start a game' }).click();

  // Wait for game table to appear
  await expect(page.getByText('Game Table')).toBeVisible();

  // Ensure player slots are rendered
  await expect(page.getByText('You')).toBeVisible();
  await expect(page.getByText('Bot 1')).toBeVisible();

  // Ensure deck slots are present
  await expect(page.getByText('Slot 1')).toBeVisible();
});