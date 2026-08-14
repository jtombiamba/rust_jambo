import { test, expect } from '@playwright/test';

test('WebSocket updates when card is played', async ({ page }) => {
  const gameId = 'test-game-id';
  const humanPlayerId = 'player-human';
  const botPlayerId = 'player-bot-1';

  // Mock HTTP APIs
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
        game_id: gameId,
        players: [
          {
            id: humanPlayerId,
            type: 'human',
            name: 'You',
            position: 0,
            cards: [0, 1, 2, 3, 4],
          },
          {
            id: botPlayerId,
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

  // Mock the play_card API endpoint (POST /api/game/{gameId}/play)
  await page.route(`**/api/game/${gameId}/play`, (route) => {
    // Simulate a successful card play
    const request = route.request();
    const method = request.method();
    if (method === 'POST') {
      // Return a success response (the actual response from backend is not used by frontend
      // beyond the HTTP status; the UI updates via WebSocket)
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ success: true }),
      });
    } else {
      route.fallback();
    }
  });

  // Inject mock WebSocket before loading the page
  // The code inside addInitScript runs in the browser context, not Node.js,
  // so it needs window casts that TypeScript can't fully type-check.
  await page.addInitScript(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const w = window as any;

    // Store the latest WebSocket instance and a method to simulate messages
    w.__mockWebSocket = {
      instance: null,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      simulateMessage: (data: any) => {
        const ws = w.__mockWebSocket.instance;
        if (ws && ws.onmessage) {
          const event = { data: JSON.stringify(data) };
          ws.onmessage(event);
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
        // Store this instance globally
        w.__mockWebSocket.instance = this;

        // Simulate connection after a tiny delay
        setTimeout(() => {
          this.readyState = MockWebSocket.OPEN;
          if (this.onopen) this.onopen();
        }, 10);
      }

      send(data: string) {
        // Optional: log outgoing messages
        console.log('MockWebSocket send:', data);
      }

      close() {
        this.readyState = MockWebSocket.CLOSED;
        if (this.onclose) this.onclose(new CloseEvent('close'));
      }
    }

    // Replace the global WebSocket
    w.WebSocket = MockWebSocket;
  });

  // Navigate to the app
  await page.goto('http://localhost:5173');

  // Wait for dashboard and start game
  await expect(page.getByText('FapFap Card Game')).toBeVisible();
  await page.getByRole('button', { name: 'Start a quick game' }).click();
  await expect(page.getByText('Game Table')).toBeVisible();

  // Ensure the human player has 5 cards visible
  const humanCards = page.locator('[data-testid="player-slot-player-human"] [data-testid^="card-"]');
  await expect(humanCards).toHaveCount(5);

  // Click the first card (index 0)
  await humanCards.first().click();

  // Simulate a WebSocket card_played event from the server
  await page.evaluate(({ gameId, humanPlayerId, botPlayerId }: Record<string, string>) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).__mockWebSocket.simulateMessage({
      type: 'card_played',
      game_id: gameId,
      player_id: humanPlayerId,
      card_index: 0,
      next_turn: botPlayerId, // turn passes to bot
    });
  }, { gameId, humanPlayerId, botPlayerId });

  // Verify that the played card disappears from the human's hand
  // (should now have 4 cards)
  await expect(humanCards).toHaveCount(4);

  // Verify that the turn indicator moves to the bot (red ring around bot slot)
  // The bot slot should have the CSS class indicating current turn
  const botSlot = page.locator('[data-testid="player-slot-player-bot-1"]');
  await expect(botSlot).toHaveClass(/ring-4 ring-red-500 ring-offset-2/);

  // Verify that the deck slot for this round is filled with the played card
  // Deck slots are displayed as Card components with faceUp false; we can check
  // that the first deck slot is not empty (i.e., not showing "Slot 1")
  const firstDeckSlot = page.locator('[data-testid="deck-slot-0"]');
  await expect(firstDeckSlot).not.toContainText('Slot 1');
});

test('WebSocket updates for round completion', async ({ page: _page }) => {
  // Similar setup but simulate RoundCompleted event
  // This test is a placeholder for now
});
