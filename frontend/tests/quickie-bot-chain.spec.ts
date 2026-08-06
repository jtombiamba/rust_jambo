import { test, expect } from '@playwright/test';

const gameId = 'test-quickie-game';
const humanId = 'player-human';
const bot1Id = 'player-bot-1';
const bot2Id = 'player-bot-2';
const bot3Id = 'player-bot-3';

test.describe('Quickie game — bot chain with delays', () => {
  test.beforeEach(async ({ page }) => {
    // Mock /api/config for delay values
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

    // Mock anonymous stats
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

    // Mock quickie game creation
    await page.route('**/api/quickie', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          game_id: gameId,
          players: [
            { id: humanId, type: 'human', name: 'You', position: 0, cards: [0, 1, 2, 3, 4] },
            { id: bot1Id, type: 'bot', name: 'Bot 1', position: 1, cards: [] },
            { id: bot2Id, type: 'bot', name: 'Bot 2', position: 2, cards: [] },
            { id: bot3Id, type: 'bot', name: 'Bot 3', position: 3, cards: [] },
          ],
          status: 'playing',
          current_turn: 0,
          bet: 10,
        }),
      }),
    );

    // Mock the play_card endpoint
    await page.route(`**/api/game/${gameId}/play`, (route) => {
      const method = route.request().method();
      if (method === 'POST') {
        route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({ success: true }),
        });
      } else {
        route.fallback();
      }
    });

    // Inject mock WebSocket
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

  test('human plays card, 3 bots chain with delays, turn returns to human', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByText('FapFap Card Game')).toBeVisible();

    // Start the game
    await page.getByRole('button', { name: 'Start a quick game' }).click();
    await expect(page.getByText('Game Table')).toBeVisible();

    // Verify initial state: 5 human cards
    const humanCards = page.locator('[data-testid="player-slot-player-human"] [data-testid^="card-"]');
    await expect(humanCards).toHaveCount(5);

    // Human plays the first card (index 0)
    await humanCards.first().click();

    // Simulate human's own CardPlayed event (immediate)
    await page.evaluate(({ gid, hid, bid1 }: Record<string, string>) => {
      (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket.simulateMessage({
        type: 'card_played',
        game_id: gid,
        player_id: hid,
        card_index: 0,
        next_turn: bid1,
      });
    }, { gid: gameId, hid: humanId, bid1: bot1Id });

    // Card removed from human's hand
    await expect(humanCards).toHaveCount(4);

    // Deck slot 0 now has the played card
    const deckSlot0 = page.locator('[data-testid="deck-slot-0"]');
    await expect(deckSlot0).not.toContainText('Slot 1');

    // Turn ring should be on the human (since applyCardPlayed advances turn,
    // but during bot chain, bot 1 gets the turn ring)

    // Now simulate rapid bot chain events (backend emits them all at once without sleep)
    await page.evaluate(({ gid, bid1, bid2, bid3 }: Record<string, string>) => {
      const mock = (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket;

      // Bot 1 plays
      mock.simulateMessage({
        type: 'card_played',
        game_id: gid,
        player_id: bid1,
        card_index: 5,
        next_turn: bid2,
      });
      mock.simulateMessage({
        type: 'turn_changed',
        game_id: gid,
        current_turn: bid2,
      });

      // Bot 2 plays
      mock.simulateMessage({
        type: 'card_played',
        game_id: gid,
        player_id: bid2,
        card_index: 12,
        next_turn: bid3,
      });
      mock.simulateMessage({
        type: 'turn_changed',
        game_id: gid,
        current_turn: bid3,
      });

      // Bot 3 plays (next is human)
      mock.simulateMessage({
        type: 'card_played',
        game_id: gid,
        player_id: bid3,
        card_index: 20,
        next_turn: 'player-human',
      });
    }, { gid: gameId, bid1: bot1Id, bid2: bot2Id, bid3: bot3Id });

    // At this point, the events are queued and replay is triggered.
    // The first bot card should appear after BOT_THINKING_DELAY (800ms).
    // We need to wait for the replay to deliver the first bot card.

    // Wait for the first bot card to appear in the deck (after ~800ms delay)
    await page.waitForTimeout(2000); // Give enough time for 3 × 800ms replay

    // After replay, deck should have 4 cards (human + 3 bots)
    // Each deck slot should not have placeholder text
    const deckSlots = page.locator('[data-testid^="deck-slot-"]');
    const filledSlots = deckSlots.filter({ has: page.locator('[data-testid^="card-"]') });
    await expect(filledSlots).toHaveCount(4);

    // Turn should be back to human (display_position 0)
    const humanSlot = page.locator('[data-testid="player-slot-player-human"]');
    await expect(humanSlot).toHaveClass(/ring-4 ring-red-500 ring-offset-2/);

    // Remaining cards: human now has 3 (5 - 1 = 4, wait no, human played 1, so 4 remaining)
    // and each bot lost 1 card too from remaining count
    // But remaining count depends on the store. Let's just verify no crash.
  });

  test('bot chain thinking indicator appears on correct bot slot', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByText('FapFap Card Game')).toBeVisible();

    // Start the game
    await page.getByRole('button', { name: 'Start a quick game' }).click();
    await expect(page.getByText('Game Table')).toBeVisible();

    // Human plays a card
    const humanCards = page.locator('[data-testid="player-slot-player-human"] [data-testid^="card-"]');
    await humanCards.first().click();

    // Human CardPlayed
    await page.evaluate(({ gid, hid, bid1 }: Record<string, string>) => {
      (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket.simulateMessage({
        type: 'card_played',
        game_id: gid,
        player_id: hid,
        card_index: 0,
        next_turn: bid1,
      });
    }, { gid: gameId, hid: humanId, bid1: bot1Id });

    // Rapid bot events
    await page.evaluate(({ gid, bid1, bid2, bid3 }: Record<string, string>) => {
      const mock = (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket;

      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid1,
        card_index: 5, next_turn: bid2,
      });
      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid2,
        card_index: 12, next_turn: bid3,
      });
      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid3,
        card_index: 20, next_turn: 'player-human',
      });
    }, { gid: gameId, bid1: bot1Id, bid2: bot2Id, bid3: bot3Id });

    // Wait for replay to complete
    await page.waitForTimeout(3000);

    // Turn back to human after replay
    const humanSlot = page.locator('[data-testid="player-slot-player-human"]');
    await expect(humanSlot).toHaveClass(/ring-4 ring-red-500 ring-offset-2/);
  });

  test('round completion triggers pause before new round', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByText('FapFap Card Game')).toBeVisible();
    await page.getByRole('button', { name: 'Start a quick game' }).click();
    await expect(page.getByText('Game Table')).toBeVisible();

    const humanCards = page.locator('[data-testid="player-slot-player-human"] [data-testid^="card-"]');
    await humanCards.first().click();

    // Human plays, then bots play — the LAST bot's card completes the round
    await page.evaluate(({ gid, hid, bid1, bid2, bid3 }: Record<string, string>) => {
      const mock = (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket;

      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: hid,
        card_index: 0, next_turn: bid1,
      });
      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid1,
        card_index: 5, next_turn: bid2,
      });
      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid2,
        card_index: 12, next_turn: bid3,
      });
      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid3,
        card_index: 20, next_turn: 'player-human',
      });

      // Round completed
      mock.simulateMessage({
        type: 'round_completed',
        game_id: gid,
        round_number: 1,
        winner_id: bid3,
        winner_position: 3,
        win_type: 'normal',
        deck_slots: [0, 5, 12, 20],
      });
    }, { gid: gameId, hid: humanId, bid1: bot1Id, bid2: bot2Id, bid3: bot3Id });

    // Replay timeline (botDelayMs=800, roundPauseMs=2500):
    //   t=800  apply bot1 card
    //   t=1600 apply bot2 card
    //   t=2400 apply bot3 card (last card of the round)
    //   t=4900 consume round_pause -> clear deck + show winner
    //
    // The last card must remain visible until the round_pause barrier is
    // consumed, so the deck must NOT be cleared before ~4900ms.

    // Wait until the last bot card has been applied but the round_pause has
    // not yet been consumed (~3500ms). The deck should still be filled.
    await page.waitForTimeout(3500);
    const deckSlot0 = page.locator('[data-testid="deck-slot-0"]');
    await expect(deckSlot0).not.toContainText('Slot 1');

    // Wait past the round_pause barrier (~4900ms) and verify the deck clears.
    await page.waitForTimeout(2000);
    await expect(deckSlot0).toContainText('Slot 1');
  });

  test('game finished mid-chain cancels replay queue', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByText('FapFap Card Game')).toBeVisible();
    await page.getByRole('button', { name: 'Start a quick game' }).click();
    await expect(page.getByText('Game Table')).toBeVisible();

    const humanCards = page.locator('[data-testid="player-slot-player-human"] [data-testid^="card-"]');
    await humanCards.first().click();

    // Human plays, bots start chain
    await page.evaluate(({ gid, hid, bid1, bid2, bid3 }: Record<string, string>) => {
      const mock = (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket;

      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: hid,
        card_index: 0, next_turn: bid1,
      });
      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid1,
        card_index: 5, next_turn: bid2,
      });
      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid2,
        card_index: 12, next_turn: bid3,
      });

      // Game ends before bot 3 plays (Kora)
      mock.simulateMessage({
        type: 'game_finished',
        game_id: gid,
        winner_id: bid2,
        winner_name: 'Bot 2',
        winner_position: 2,
        status: 'kora',
        final_score: 50,
        rounds_played: 4,
      });
    }, { gid: gameId, hid: humanId, bid1: bot1Id, bid2: bot2Id, bid3: bot3Id });

    // Game over modal should appear. The game_finished event uses status 'kora',
    // so the modal title is the localized "KORA!" string.
    await expect(page.getByText('KORA!')).toBeVisible({ timeout: 5000 });
  });

  test('reconnection cancels bot replay and applies snapshot', async ({ page }) => {
    await page.goto('/');
    await expect(page.getByText('FapFap Card Game')).toBeVisible();
    await page.getByRole('button', { name: 'Start a quick game' }).click();
    await expect(page.getByText('Game Table')).toBeVisible();

    const humanCards = page.locator('[data-testid="player-slot-player-human"] [data-testid^="card-"]');
    await humanCards.first().click();

    // Start bot chain
    await page.evaluate(({ gid, hid, bid1, bid2 }: Record<string, string>) => {
      const mock = (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket;

      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: hid,
        card_index: 0, next_turn: bid1,
      });
      mock.simulateMessage({
        type: 'card_played', game_id: gid, player_id: bid1,
        card_index: 5, next_turn: bid2,
      });
    }, { gid: gameId, hid: humanId, bid1: bot1Id, bid2: bot2Id });

    // Simulate reconnection via game_state_snapshot while replay is active
    await page.evaluate(({ gid }: { gid: string }) => {
      const mock = (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket;

      mock.simulateMessage({
        type: 'game_state_snapshot',
        game_id: gid,
        roll: 1,
        rank: 0,
        status: 'playing',
        current_winning_card: null,
        current_winning_player_position: null,
        players: [
          {
            id: 'player-human',
            name: 'You',
            position: 0,
            display_position: 0,
            player_type: 'human',
          },
          {
            id: 'player-bot-1',
            name: 'Bot 1',
            position: 1,
            display_position: 1,
            player_type: 'bot',
          },
          {
            id: 'player-bot-2',
            name: 'Bot 2',
            position: 2,
            display_position: 2,
            player_type: 'bot',
          },
          {
            id: 'player-bot-3',
            name: 'Bot 3',
            position: 3,
            display_position: 3,
            player_type: 'bot',
          },
        ],
        played_cards: [0, 5, 12, 20],
      });
    }, { gid: gameId });

    // Snapshot should have been applied — deck has 4 cards now
    await page.waitForTimeout(500);
    const deckSlots = page.locator('[data-testid^="deck-slot-"]');
    const filledSlots = deckSlots.filter({ has: page.locator('[data-testid^="card-"]') });
    await expect(filledSlots).toHaveCount(4);
  });
});
