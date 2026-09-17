import { test, expect } from '@playwright/test';

const gameId = 'test-special-claim-game';
const humanId = 'player-human';

test.describe('Special cards claim flow', () => {
  test.beforeEach(async ({ page }) => {
    await page.route('**/api/config', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          paypal_donate_url: 'https://paypal.com/donate',
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

    await page.route('**/api/quickie', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          game_id: gameId,
          players: [
            { id: humanId, type: 'human', name: 'You', position: 0, cards: [0, 8, 16, 24, 5] },
            { id: 'player-bot-1', type: 'bot', name: 'Bot 1', position: 1, cards: [] },
            { id: 'player-bot-2', type: 'bot', name: 'Bot 2', position: 2, cards: [] },
            { id: 'player-bot-3', type: 'bot', name: 'Bot 3', position: 3, cards: [] },
          ],
          status: 'active',
          current_turn: 0,
          bet: 10,
        }),
      }),
    );

    await page.route(`**/api/game/${gameId}/claim-special`, (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true, winner_id: humanId, winner_position: 0 }),
      }),
    );

    await page.route(`**/api/game/${gameId}/decline-special`, (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true }),
      }),
    );

    await page.addInitScript(() => {
      const w = window as unknown as {
        __mockWebSocket: {
          instance: {
            onmessage: ((event: { data: string }) => void) | null;
          } | null;
          simulateMessage: (data: Record<string, unknown>) => void;
        };
        WebSocket: typeof WebSocket;
      };

      w.__mockWebSocket = {
        instance: null,
        simulateMessage: (data: Record<string, unknown>) => {
          const ws = w.__mockWebSocket.instance;
          if (ws && ws.onmessage) {
            ws.onmessage({ data: JSON.stringify(data) });
          }
        },
      };

      class MockWebSocket {
        url: string;
        readyState: number;
        onopen: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        onerror: ((error: Event) => void) | null = null;
        onclose: ((event: CloseEvent) => void) | null = null;

        static CONNECTING = 0;
        static OPEN = 1;
        static CLOSING = 2;
        static CLOSED = 3;

        constructor(url: string) {
          this.url = url;
          this.readyState = MockWebSocket.CONNECTING;
          w.__mockWebSocket.instance = this;
          setTimeout(() => {
            this.readyState = MockWebSocket.OPEN;
            if (this.onopen) this.onopen();
          }, 10);
        }

        send(_data: string) {
          // no-op
        }

        close() {
          this.readyState = MockWebSocket.CLOSED;
          if (this.onclose) this.onclose(new CloseEvent('close'));
        }
      }

      w.WebSocket = MockWebSocket as unknown as typeof WebSocket;
    });
  });

  const startGame = async (page: import('@playwright/test').Page) => {
    await page.goto('/');
    await expect(page.getByText('FapFap Card Game')).toBeVisible();
    await page.getByRole('button', { name: 'Start a quick game' }).click();
    await expect(page.getByText('Game Table')).toBeVisible();
  };

  const offerClaim = (page: import('@playwright/test').Page, specialCards: Record<string, boolean>) =>
    page.evaluate(
      ({ gid, hid, specials }) => {
        (window as unknown as {
          __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
        }).__mockWebSocket.simulateMessage({
          type: 'claim_offered',
          game_id: gid,
          player_id: hid,
          special_cards: specials,
        });
      },
      { gid: gameId, hid: humanId, specials: specialCards },
    );

  test('claim offered shows modal; claiming reveals cards and ends the game', async ({ page }) => {
    await startGame(page);

    await offerClaim(page, {
      check_triple_seven: false,
      check_sum_value_under_21: false,
      check_a_square: true,
    });

    await expect(page.getByText('Special Cards!')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Claim Victory' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Continue' })).toBeVisible();

    const claimRequest = page.waitForRequest(`**/api/game/${gameId}/claim-special`);
    await page.getByRole('button', { name: 'Claim Victory' }).click();
    const request = await claimRequest;
    expect(request.method()).toBe('POST');

    // Simulate the backend broadcasting the claim + game finish.
    await page.evaluate(
      ({ gid, hid }) => {
        const mock = (window as unknown as {
          __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
        }).__mockWebSocket;
        mock.simulateMessage({
          type: 'special_claim',
          game_id: gid,
          player_id: hid,
          cards: [0, 8, 16, 24, 5],
          winner_position: 0,
        });
        mock.simulateMessage({
          type: 'game_finished',
          game_id: gid,
          winner_id: hid,
          winner_name: 'You',
          winner_position: 0,
          status: 'finished',
          rounds_played: 1,
        });
      },
      { gid: gameId, hid: humanId },
    );

    await expect(page.getByText('Game Over!')).toBeVisible();
    await expect(page.getByText('Special Cards!')).toBeHidden();
  });

  test('declining the claim closes the modal and allows play', async ({ page }) => {
    await startGame(page);

    await offerClaim(page, {
      check_triple_seven: true,
      check_sum_value_under_21: false,
      check_a_square: false,
    });

    await expect(page.getByText('Special Cards!')).toBeVisible();

    const declineRequest = page.waitForRequest(`**/api/game/${gameId}/decline-special`);
    await page.getByRole('button', { name: 'Continue' }).click();
    const request = await declineRequest;
    expect(request.method()).toBe('POST');

    await expect(page.getByText('Special Cards!')).toBeHidden();
    await expect(page.getByText('Game Over!')).toBeHidden();
  });

  test('claim pending shows a waiting banner for other players', async ({ page }) => {
    await startGame(page);

    await page.evaluate((gid) => {
      (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket.simulateMessage({ type: 'claim_pending', game_id: gid });
    }, gameId);

    await expect(page.getByText('Waiting for a special-card decision...')).toBeVisible();
    await expect(page.getByText('Special Cards!')).toBeHidden();
  });

  test('claim modal is restored from a game_state_snapshot on reconnect', async ({ page }) => {
    await startGame(page);

    await page.evaluate(({ gid, hid }) => {
      (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket.simulateMessage({
        type: 'game_state_snapshot',
        game_id: gid,
        roll: 1,
        rank: 0,
        status: 'active',
        current_winning_card: null,
        current_winning_player_position: null,
        players: [
          { id: hid, name: 'You', position: 0, display_position: 0, player_type: 'human', cards_count: 5 },
          { id: 'player-bot-1', name: 'Bot 1', position: 1, display_position: 1, player_type: 'bot', cards_count: 5 },
          { id: 'player-bot-2', name: 'Bot 2', position: 2, display_position: 2, player_type: 'bot', cards_count: 5 },
          { id: 'player-bot-3', name: 'Bot 3', position: 3, display_position: 3, player_type: 'bot', cards_count: 5 },
        ],
        played_cards: [null, null, null, null],
        step_by_step: false,
        game_mode: 'multiplayer',
        claim_pending: true,
        claim_offered_to_me: true,
        special_cards: {
          check_triple_seven: false,
          check_sum_value_under_21: false,
          check_a_square: true,
        },
      });
    }, { gid: gameId, hid: humanId });

    await expect(page.getByText('Special Cards!')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Claim Victory' })).toBeVisible();
  });
});
