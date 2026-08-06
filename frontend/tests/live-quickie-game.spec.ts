import { test, expect} from '@playwright/test';

/**
 * LIVE quickie game test — runs against the real backend at
 * http://localhost:3000 (Docker stack). This is NOT a mocked test: it uses the
 * real /api/quickie endpoint, a real WebSocket connection, and real bot
 * scheduling with the backend's configured delays.
 *
 * Run with:
 *   npx playwright test --config=playwright.live.config.ts tests/live-quickie-game.spec.ts
 *
 * Prerequisites:
 *   - The Docker stack must be running and serving the app at localhost:3000.
 *   - The backend must have anonymous quickie games enabled.
 *
 * What this test verifies (per the requirement):
 *   1. At the launch of a round, ALL unplayed cards are displayed in the
 *      playerSlot for BOTH the human player (face-up) and the bot players
 *      (face-down placeholders). Each player starts a round with 5 cards.
 *   2. When a card is played, it appears properly on the deck slot.
 *   3. At the end of the round, after all cards of the round are displayed on
 *      the deck slot, the flying collection animation toward the winner is
 *      triggered AND the winner ring is shown.
 */

const CARDS_PER_PLAYER = 5; // backend/src/game/constants.rs
const NUM_PLAYERS = 4; // human + 3 bots in a quickie game

// Real backend delays (defaults from backend/src/game/constants.rs). These are
// read from the live /api/config at runtime so the waits stay in sync even if
// the deployment overrides them via env vars.
let BOT_THINKING_DELAY_MS = 800;
let ROUND_PAUSE_DELAY_MS = 2500;

test.describe('Live quickie game — full round on the real backend', () => {
  test('cards shown at launch, played to deck, winner animation + ring at round end', async ({ page }) => {
    // --- Read the real delay config so our waits match the live backend. ---
    const config = await page.request.get('/api/config');
    if (config.ok()) {
      const body = await config.json();
      if (typeof body.bot_thinking_delay_ms === 'number') {
        BOT_THINKING_DELAY_MS = body.bot_thinking_delay_ms;
      }
      if (typeof body.round_pause_delay_ms === 'number') {
        ROUND_PAUSE_DELAY_MS = body.round_pause_delay_ms;
      }
    }
    console.log(`[live] delays: botThinking=${BOT_THINKING_DELAY_MS}ms roundPause=${ROUND_PAUSE_DELAY_MS}ms`);

    // --- Start a quickie game against the real backend. ---
    await page.goto('/');
    await expect(page.getByText('FapFap Card Game')).toBeVisible({ timeout: 15000 });
    await page.getByRole('button', { name: 'Start a quick game' }).click();
    await expect(page.getByText('Game Table')).toBeVisible({ timeout: 15000 });

    // The quickie game has 4 players: 1 human + 3 bots.
    const playerSlots = page.locator('[data-testid^="player-slot-"]');
    await expect(playerSlots).toHaveCount(NUM_PLAYERS, { timeout: 15000 });

    // --- 1. At round launch, ALL unplayed cards are displayed in each playerSlot. ---
    // The human's cards are face-up (real indices); bots' cards are face-down
    // placeholders. Either way, each player must show CARDS_PER_PLAYER cards.
    const slotCounts = await playerSlots.evaluateAll((slots) =>
      slots.map((slot) => slot.querySelectorAll('[data-testid^="card-"]').length),
    );
    console.log(`[live] cards per player slot at launch: ${JSON.stringify(slotCounts)}`);
    for (const count of slotCounts) {
      expect(count, 'each playerSlot must display all unplayed cards at round launch').toBe(CARDS_PER_PLAYER);
    }

    // Identify the human slot. In the live flow the human is the ONLY player
    // whose cards are face-up (bg-white, showing rank/suit). Bots' cards are
    // face-down placeholders (bg-blue-800, showing 🂠). Both render as
    // <div role="button">, so we distinguish by the face-up background class.
    const humanSlot = page.locator('[data-testid^="player-slot-"]').filter({
      has: page.locator('[data-testid^="card-"].bg-white'),
    }).first();
    await expect(humanSlot).toHaveCount(1, { timeout: 15000 });

    // --- 2. When a card is played, it appears properly on the deck slot. ---
    // In the live environment the AI workers play bot turns almost instantly,
    // racing ahead of the human. The backend rejects a play with NotYourTurn if
    // the backend's rank has already advanced past the human (gameplay.rs:58),
    // so a single click can be silently dropped. We therefore attempt to play
    // the human's first card with a bounded, force-click retry (force bypasses
    // the actionability wait that hangs while the hand re-renders). The turn
    // returns to the human after the bots play, so a retry eventually lands on
    // the human's turn.
    const handCards = humanSlot.locator('[data-testid^="card-"]');
    let playedCardTestId: string | null = null;
    const playDeadline = Date.now() + 30000;
    while (Date.now() < playDeadline) {
      const currentCount = await handCards.count();
      if (currentCount < CARDS_PER_PLAYER) {
        break; // A card was already played.
      }
      const firstCard = handCards.first();
      playedCardTestId = await firstCard.getAttribute('data-testid');
      try {
        await firstCard.click({ force: true, timeout: 2000 });
      } catch {
        // Element detached/re-rendered mid-click — retry on the next iteration.
      }
      await page.waitForTimeout(600);
    }

    // The human's hand must have shrunk (a card was played).
    await expect(handCards).toHaveCount(CARDS_PER_PLAYER - 1, { timeout: 15000 });
    expect(playedCardTestId, 'human must have played a card').not.toBeNull();

    // The played card must appear on the deck. NOTE: in the live game the deck
    // slot assignment is NOT deterministic — the backend fills deck_slots by
    // player position (events.rs) and the bots may play before the human's
    // event is applied, so the human's card can land in any of the 4 slots.
    // We therefore assert the played card appears in SOME deck slot.
    const allDeckSlots = page.locator('[data-testid^="deck-slot-"]');
    const playedCardOnDeck = allDeckSlots.locator(`[data-testid="${playedCardTestId}"]`);
    await expect(playedCardOnDeck.first()).toBeVisible({ timeout: 15000 });

    // --- 3. Wait for the round to complete on the real backend. ---
    // After the last card, the backend emits round_completed and the frontend
    // consumes the round_pause barrier (round_pause_delay_ms) before showing the
    // winner + collection animation.
    //
    // Timeline (worst case):
    //   human card  -> deck slot (already done)
    //   +1*delay    -> bot 1 card -> deck slot
    //   +2*delay    -> bot 2 card -> deck slot
    //   +3*delay    -> bot 3 card -> deck slot (round complete)
    //   +roundPause -> winner ring + collection animation (~800ms window)
    //
    // We wait until all 4 deck slots are filled (all cards of the round are
    // displayed on the deck), then poll for the winner ring and the collection
    // overlay. The collection animation is SHORT-LIVED (~800ms), so we must
    // poll for it right when it appears (after the round_pause barrier) rather
    // than waiting a fixed amount that overshoots its window.
    const deckSlots = page.locator('[data-testid^="deck-slot-"]');
    const filledSlots = deckSlots.filter({ has: page.locator('[data-testid^="card-"]') });
    await expect(filledSlots).toHaveCount(NUM_PLAYERS, { timeout: 60000 });

    // The CardCollectionAnimation overlay (full-bleed, pointer-events-none,
    // z-50) renders the played cards flying toward the winner. It appears right
    // after the round_pause barrier and lasts ~800ms, so poll for it with a
    // timeout that covers the barrier delay.
    const collectionOverlay = page.locator('.absolute.inset-0.pointer-events-none.z-50');
    await expect(collectionOverlay).toBeVisible({ timeout: ROUND_PAUSE_DELAY_MS + 3000 });
    // At least the first played card should be flying in the overlay.
    await expect(collectionOverlay.locator(`[data-testid="${playedCardTestId}"]`)).toBeVisible({ timeout: 3000 });

    // The winner ring is the durable signal that the round winner is declared.
    // It appears at the same time as the collection animation and lasts ~3000ms.
    const winnerRing = page.locator('.winner-ring');
    await expect(winnerRing.first()).toBeVisible({ timeout: 5000 });
  });
});
