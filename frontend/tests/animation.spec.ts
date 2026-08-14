import { test, expect } from '@playwright/test';

/**
 * Animation tests for the game table.
 *
 * These tests verify that the Framer Motion animations introduced by the
 * `feature/animation_in_game` branch are respected:
 *   1. Card play animation — a card leaves the hand and appears in the deck
 *      slot (hand card count decreases, deck slot fills).
 *   2. Winner / collection animation — when a round completes, the
 *      `CardCollectionAnimation` overlay renders the played cards flying
 *      toward the winner's position.
 *   3. round_pause barrier — the last card of a round stays visible until the
 *      replay queue consumes the `round_pause` barrier (the "last card
 *      appearance" fix from `refactor/thinking_bot_in_frontend`).
 *   4. Bot-chain replay timing — bot cards are replayed with the configured
 *      `bot_thinking_delay_ms` spacing while animations are active.
 *
 * The WebSocket is mocked via `__mockWebSocket.simulateMessage`, mirroring the
 * pattern used in `quickie-bot-chain.spec.ts`.
 */

const gameId = 'test-animation-game';
const humanId = 'player-human';
const bot1Id = 'player-bot-1';
const bot2Id = 'player-bot-2';
const bot3Id = 'player-bot-3';

// Delays configured through /api/config (must match the mock below).
const BOT_THINKING_DELAY_MS = 800;
const ROUND_PAUSE_DELAY_MS = 2500;

test.describe('Game table animations', () => {
  test.beforeEach(async ({ page }) => {
    // Mock /api/config for delay values
    await page.route('**/api/config', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          paypal_donate_url: 'https://paypal.com/donate',
          bot_thinking_delay_ms: BOT_THINKING_DELAY_MS,
          round_pause_delay_ms: ROUND_PAUSE_DELAY_MS,
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

  async function startGame(page: import('@playwright/test').Page) {
    await page.goto('/');
    await expect(page.getByText('FapFap Card Game')).toBeVisible();
    await page.getByRole('button', { name: 'Start a quick game' }).click();
    await expect(page.getByText('Game Table')).toBeVisible();
  }

  async function simulate(page: import('@playwright/test').Page, data: Record<string, unknown>) {
    await page.evaluate((payload) => {
      (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket.simulateMessage(payload);
    }, data);
  }

  test('card play animation: hand card moves to deck slot', async ({ page }) => {
    await startGame(page);

    // Human starts with 5 face-up cards (card indices 0..4).
    const humanSlot = page.locator(`[data-testid="player-slot-${humanId}"]`);
    await expect(humanSlot.locator('[data-testid^="card-"]')).toHaveCount(5);

    // Deck slot 0 starts as an empty placeholder.
    const deckSlot0 = page.locator('[data-testid="deck-slot-0"]');
    await expect(deckSlot0).toContainText('Slot 1');

    // Human plays card index 0 (click the first card in the hand).
    await humanSlot.locator('[data-testid="card-0"]').click();

    // Confirm the human's own CardPlayed event (immediate, no bot chain yet).
    await simulate(page, {
      type: 'card_played',
      game_id: gameId,
      player_id: humanId,
      card_index: 0,
      next_turn: bot1Id,
    });

    // The card leaves the hand (5 -> 4) and appears in deck slot 0.
    await expect(humanSlot.locator('[data-testid^="card-"]')).toHaveCount(4);
    await expect(deckSlot0.locator('[data-testid="card-0"]')).toBeVisible();
    // The placeholder is replaced by the animated card.
    await expect(deckSlot0).not.toContainText('Slot 1');
  });

  test('winner collection animation renders toward the winner position', async ({ page }) => {
    await startGame(page);

    const humanSlot = page.locator(`[data-testid="player-slot-${humanId}"]`);
    await humanSlot.locator('[data-testid="card-0"]').click();

    // Human plays, then the 3 bots play — the last bot's card completes the round.
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: humanId,
      card_index: 0, next_turn: bot1Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot1Id,
      card_index: 5, next_turn: bot2Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot2Id,
      card_index: 12, next_turn: bot3Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot3Id,
      card_index: 20, next_turn: humanId,
    });

    // Round completed — winner is bot 3 (position 3 -> 'west').
    await simulate(page, {
      type: 'round_completed',
      game_id: gameId,
      round_number: 1,
      winner_id: bot3Id,
      winner_position: 3,
      win_type: 'normal',
      deck_slots: [0, 5, 12, 20],
    });

    // Replay timeline (botDelayMs=800, roundPauseMs=2500):
    //   t=800  apply bot1 card
    //   t=1600 apply bot2 card
    //   t=2400 apply bot3 card (last card of the round)
    //   t=4900 consume round_pause -> show winner + collection animation
    //
    // The round_pause barrier is consumed at ~4900ms and the collection
    // animation runs for ~800ms (until ~5700ms). Wait until the animation is
    // actively rendering (shortly after the barrier) so the flying cards are
    // present in the overlay.
    await page.waitForTimeout(ROUND_PAUSE_DELAY_MS + BOT_THINKING_DELAY_MS * 3 + 200);

    // The winner ring is the durable signal that the round winner is declared.
    const winnerRing = page.locator('.winner-ring');
    await expect(winnerRing.first()).toBeVisible();

    // The CardCollectionAnimation overlay is a full-bleed, pointer-events-none
    // overlay (z-50) that renders the played cards flying toward the winner.
    // It must contain the played card indices (0, 5, 12, 20) while animating.
    const collectionOverlay = page.locator('.absolute.inset-0.pointer-events-none.z-50');
    await expect(collectionOverlay).toBeVisible();
    await expect(collectionOverlay.locator('[data-testid="card-0"]')).toBeVisible();
    await expect(collectionOverlay.locator('[data-testid="card-5"]')).toBeVisible();
    await expect(collectionOverlay.locator('[data-testid="card-12"]')).toBeVisible();
    await expect(collectionOverlay.locator('[data-testid="card-20"]')).toBeVisible();
  });

  test('round_pause barrier keeps last card visible before deck clears', async ({ page }) => {
    await startGame(page);

    const humanSlot = page.locator(`[data-testid="player-slot-${humanId}"]`);
    await humanSlot.locator('[data-testid="card-0"]').click();

    // Human plays, then bots play — the LAST bot's card completes the round.
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: humanId,
      card_index: 0, next_turn: bot1Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot1Id,
      card_index: 5, next_turn: bot2Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot2Id,
      card_index: 12, next_turn: bot3Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot3Id,
      card_index: 20, next_turn: humanId,
    });

    // Round completed.
    await simulate(page, {
      type: 'round_completed',
      game_id: gameId,
      round_number: 1,
      winner_id: bot3Id,
      winner_position: 3,
      win_type: 'normal',
      deck_slots: [0, 5, 12, 20],
    });

    // Replay timeline:
    //   t=800  apply bot1 card
    //   t=1600 apply bot2 card
    //   t=2400 apply bot3 card (last card of the round)
    //   t=4900 consume round_pause -> clear deck + show winner
    //
    // The last card must remain visible until the round_pause barrier is
    // consumed, so the deck must NOT be cleared before ~4900ms.
    await page.waitForTimeout(BOT_THINKING_DELAY_MS * 3 + 1100); // ~3500ms

    const deckSlot0 = page.locator('[data-testid="deck-slot-0"]');
    // The last card (index 20) is still in the deck — not cleared yet.
    await expect(deckSlot0.locator('[data-testid="card-0"]')).toBeVisible();

    // Wait past the round_pause barrier (~4900ms) and verify the deck clears.
    await page.waitForTimeout(ROUND_PAUSE_DELAY_MS + 500);
    await expect(deckSlot0).toContainText('Slot 1');
  });

  test('bot-chain replay timing is respected with animations', async ({ page }) => {
    await startGame(page);

    const humanSlot = page.locator(`[data-testid="player-slot-${humanId}"]`);
    await humanSlot.locator('[data-testid="card-0"]').click();

    // Human plays, then the 3 bots play in a rapid burst (no backend sleep).
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: humanId,
      card_index: 0, next_turn: bot1Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot1Id,
      card_index: 5, next_turn: bot2Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot2Id,
      card_index: 12, next_turn: bot3Id,
    });
    await simulate(page, {
      type: 'card_played', game_id: gameId, player_id: bot3Id,
      card_index: 20, next_turn: humanId,
    });

    // The events are queued and replayed with bot_thinking_delay_ms spacing.
    // After ~1.5 * delay, only the first bot card should have been applied.
    await page.waitForTimeout(Math.floor(BOT_THINKING_DELAY_MS * 1.5));

    const deckSlot0 = page.locator('[data-testid="deck-slot-0"]');
    const deckSlot1 = page.locator('[data-testid="deck-slot-1"]');
    // Human card is in slot 0; bot 1's card should be in slot 1 by now.
    await expect(deckSlot0.locator('[data-testid="card-0"]')).toBeVisible();
    await expect(deckSlot1.locator('[data-testid="card-5"]')).toBeVisible();

    // After the full chain (3 * delay), all 4 cards are in the deck.
    await page.waitForTimeout(BOT_THINKING_DELAY_MS * 3 + 500);
    const deckSlots = page.locator('[data-testid^="deck-slot-"]');
    const filledSlots = deckSlots.filter({ has: page.locator('[data-testid^="card-"]') });
    await expect(filledSlots).toHaveCount(4);

    // Turn returns to the human.
    await expect(humanSlot).toHaveClass(/ring-4 ring-red-500 ring-offset-2/);
  });
});
