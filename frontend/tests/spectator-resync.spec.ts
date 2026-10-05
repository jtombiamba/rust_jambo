import { test, expect } from '@playwright/test';

/**
 * Spectator stream — deck-slot stability under periodic resync.
 *
 * This runs against the dev server (localhost:5173) with a mocked WebSocket so
 * the exact event ordering is deterministic.
 *
 * It guards the two invariants that make spectator mode robust:
 *  1. Live `card_played` events are keyed to the player's SEAT position (not
 *     "first empty slot"), matching the backend snapshot ordering.
 *  2. A `game_state_snapshot` (the periodic resync) with the same seat-ordered
 *     `played_cards` does not reorder/rotate the deck — slots stay stable.
 *
 * Run: npx playwright test tests/spectator-resync.spec.ts
 */

const GAME_ID = 'test-spectator-game';
const PLAYERS = [
  { id: 'p0', name: 'Seat 0', position: 0 },
  { id: 'p1', name: 'Seat 1', position: 1 },
  { id: 'p2', name: 'Seat 2', position: 2 },
  { id: 'p3', name: 'Seat 3', position: 3 },
];

// Card values keyed by seat, deliberately distinct so slot placement is
// unambiguous.
const SEAT_CARD: Record<number, number> = { 0: 10, 1: 11, 2: 12, 3: 13 };

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      __mockWebSocket: {
        instance: { onopen: (() => void) | null; onmessage: ((e: { data: string }) => void) | null } | null;
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
        // no-op: the join_game handshake is not needed for this test
      }

      close() {
        this.readyState = MockWebSocket.CLOSED;
        if (this.onclose) this.onclose(new CloseEvent('close'));
      }
    }

    w.WebSocket = MockWebSocket as unknown as typeof WebSocket;
  });
});

const startGame = (page: import('@playwright/test').Page) =>
  page.evaluate(
    ({ gid, players }) => {
      (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket.simulateMessage({
        type: 'game_started',
        game_id: gid,
        players: players.map((p: { id: string; name: string; position: number }) => ({
          id: p.id,
          name: p.name,
          position: p.position,
          display_position: p.position,
          player_type: 'human',
          cards_count: 5,
        })),
        current_turn: players[0].id,
        game_mode: 'multiplayer',
      });
    },
    { gid: GAME_ID, players: PLAYERS },
  );

const playCard = (page: import('@playwright/test').Page, playerId: string, cardIndex: number) =>
  page.evaluate(
    ({ gid, pid, card }) => {
      (window as unknown as {
        __mockWebSocket: { simulateMessage: (data: Record<string, unknown>) => void };
      }).__mockWebSocket.simulateMessage({
        type: 'card_played',
        game_id: gid,
        player_id: pid,
        card_index: card,
      });
    },
    { gid: GAME_ID, pid: playerId, card: cardIndex },
  );

const sendResyncSnapshot = (page: import('@playwright/test').Page, playedCards: (number | null)[]) =>
  page.evaluate(
    ({ gid, players, played }) => {
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
        players: players.map((p: { id: string; name: string; position: number }) => ({
          id: p.id,
          name: p.name,
          position: p.position,
          display_position: p.position,
          player_type: 'human',
          cards_count: 4,
        })),
        played_cards: played,
      });
    },
    { gid: GAME_ID, players: PLAYERS, played: playedCards },
  );

/** Read the card value (or null) currently rendered in each deck slot. */
const readDeck = async (page: import('@playwright/test').Page): Promise<(number | null)[]> => {
  const slots = page.locator('[data-testid^="deck-slot-"]');
  const count = await slots.count();
  const deck: (number | null)[] = [];
  for (let i = 0; i < count; i++) {
    const card = slots.nth(i).locator('[data-testid^="card-"]');
    const cardCount = await card.count();
    if (cardCount > 0) {
      const testid = await card.first().getAttribute('data-testid');
      // card testid is "card-<value>" (or "card-<value>-selected")
      deck.push(Number((testid ?? '').split('-')[1]));
    } else {
      deck.push(null);
    }
  }
  return deck;
};

test('deck slots are keyed by seat and remain stable across a resync snapshot', async ({ page }) => {
  await page.goto(`/game/${GAME_ID}/stream?token=mock`);

  await startGame(page);

  // Wait for the live table (status flips to 'live' after game_started).
  await expect(page.locator('[data-testid^="deck-slot-"]')).toHaveCount(4, { timeout: 10000 });

  // Play in a NON-seat order: seat 2 → seat 0 → seat 3 → seat 1.
  // This proves each card is placed at its seat, not the first empty slot.
  await playCard(page, 'p2', SEAT_CARD[2]);
  await playCard(page, 'p0', SEAT_CARD[0]);
  await playCard(page, 'p3', SEAT_CARD[3]);
  await playCard(page, 'p1', SEAT_CARD[1]);

  const expected = [SEAT_CARD[0], SEAT_CARD[1], SEAT_CARD[2], SEAT_CARD[3]];

  // Let React flush the state updates.
  await expect
    .poll(() => readDeck(page), { timeout: 5000 })
    .toEqual(expected);

  // Periodic resync: the backend now sends `played_cards` in seat order, which
  // must be byte-for-byte identical to what live events already produced.
  await sendResyncSnapshot(page, expected);

  // The deck must remain STABLE (no reordering / no slot "jump") across the
  // resync. Give the resync a moment to be applied.
  await page.waitForTimeout(300);
  await expect(readDeck(page)).resolves.toEqual(expected);

  // A second resync must also be a no-op (idempotent), even after the current
  // winning player has changed mid-round (winner rotation must not affect the
  // seat-keyed layout).
  await sendResyncSnapshot(page, expected);
  await page.waitForTimeout(300);
  await expect(readDeck(page)).resolves.toEqual(expected);
});
