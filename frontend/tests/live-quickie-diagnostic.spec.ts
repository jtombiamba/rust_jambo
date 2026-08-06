import { test, expect } from '@playwright/test';

/**
 * LIVE diagnostic for bot-thinking-frontend-migration.
 *
 * Launches a real browser at localhost:3000, starts a quickie game, WAITS for
 * the human's turn, plays the human's card, and records:
 *   (a) when each WebSocket game event ARRIVES (backend emit timing)
 *   (b) when each card is APPLIED to the deck (frontend replay timing)
 *   (c) the snapshot rank + human position/display_position (turn validation)
 *   (d) the play request payload + response (to diagnose 403s)
 *
 * Both baselines are page-load so they can be correlated directly.
 *
 * If the migration is correct:
 *   - All bot events arrive in a burst (<100ms apart) — backend has no sleep
 *   - Cards are applied to the deck one at a time with ~800ms gaps — frontend replay
 *
 * Run: npx playwright test tests/live-quickie-diagnostic.spec.ts --config=playwright.live.config.ts
 */

const BASE = 'http://localhost:3000';

test('LIVE: quickie bot chain — event arrival vs card application timing', async ({ page }) => {
  test.setTimeout(180000);

  // Hook WebSocket to timestamp incoming game events (page-load baseline)
  await page.addInitScript(() => {
    const w = window as unknown as {
      __wsEvents: { t: number; type: string; player_id?: string; next_turn?: string; rank?: number | null }[];
      __wsStart: number;
      __deckEvents: { t: number; slot: number; card: string }[];
      __deckSeen: Set<string>;
      __deckClears: { t: number; slot: number }[];
      __deckFilled: Set<number>;
      __winnerEvents: { t: number; text: string }[];
      __players: Record<string, string>;
      __snapshots: { t: number; rank: number | null; players: { id: string; name: string; pos: number; dpos: number; type: string }[] }[];
      __playResults: { t: number; ok: boolean; status?: number; body?: string; player_id?: string; card_index?: number }[];
    };
    w.__wsEvents = [];
    w.__wsStart = Date.now();
    w.__deckEvents = [];
    w.__deckSeen = new Set();
    w.__deckClears = [];
    w.__deckFilled = new Set();
    w.__winnerEvents = [];
    w.__players = {};
    w.__snapshots = [];
    w.__playResults = [];

    // Patch WebSocket to log incoming messages
    const OrigWS = window.WebSocket;
    class LoggingWS extends OrigWS {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(url, protocols);
        this.addEventListener('message', (ev: MessageEvent) => {
          try {
            const data = JSON.parse(String(ev.data));
            if (data && data.type) {
              w.__wsEvents.push({
                t: Date.now() - w.__wsStart,
                type: data.type,
                player_id: data.player_id,
                next_turn: data.next_turn,
                rank: data.rank,
              });
              // Capture player list from snapshot to identify the human
              if (data.type === 'game_state_snapshot' && Array.isArray(data.players)) {
                const players = data.players.map((p: { id: string; name?: string; position: number; display_position: number; player_type?: string; }) => ({
                  id: p.id,
                  name: p.name ?? '',
                  pos: p.position,
                  dpos: p.display_position,
                  type: p.player_type ?? '',
                }));
                w.__snapshots.push({ t: Date.now() - w.__wsStart, rank: data.rank ?? null, players });
                for (const p of players) {
                  w.__players[p.id] = `${p.name}|${p.type}|pos=${p.pos}|dpos=${p.dpos}`;
                }
              }
            }
          } catch {
            // ignore
          }
        });
      }
    }
    // @ts-expect-error -- replacing native WebSocket with logging subclass
    window.WebSocket = LoggingWS;

    // Intercept fetch to capture the play request/response
    const origFetch = window.fetch.bind(window);
    window.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.includes('/api/game/') && url.endsWith('/play')) {
        let body: Record<string, unknown> = {};
        try { body = JSON.parse(String(init?.body ?? '{}')); } catch { /* ignore */ }
        const res = await origFetch(input, init);
        const resClone = res.clone();
        let respBody = '';
        try { respBody = await resClone.text(); } catch { /* ignore */ }
        w.__playResults.push({
          t: Date.now() - w.__wsStart,
          ok: res.ok,
          status: res.status,
          body: respBody,
          player_id: body.player_id,
          card_index: body.card_index,
        });
        return res;
      }
      return origFetch(input, init);
    };

    // Poll deck slots to record fill transitions (page-load baseline)
    const interval = setInterval(() => {
      const now = Date.now() - w.__wsStart;
      for (let i = 0; i < 4; i++) {
        const el = document.querySelector(`[data-testid="deck-slot-${i}"]`);
        if (!el) continue;
        const card = el.querySelector('[data-testid^="card-"]');
        if (card) {
          const key = `${i}:${card.getAttribute('data-testid')}`;
          if (!w.__deckSeen.has(key)) {
            w.__deckSeen.add(key);
            w.__deckEvents.push({ t: now, slot: i, card: card.getAttribute('data-testid') || '' });
          }
          w.__deckFilled.add(i);
        } else if (w.__deckFilled.has(i)) {
          // Slot went from filled -> empty (deck clear)
          w.__deckFilled.delete(i);
          w.__deckClears.push({ t: now, slot: i });
        }
      }
      // Detect round winner ring (WinnerRing renders with class "winner-ring")
      const winnerEl = document.querySelector('.winner-ring .winner-ring-win-type');
      if (winnerEl) {
        const txt = (winnerEl.textContent || '').trim();
        const last = w.__winnerEvents[w.__winnerEvents.length - 1];
        if (!last || last.text !== txt) {
          w.__winnerEvents.push({ t: now, text: txt });
        }
      }
    }, 30);
    (w as unknown as { __deckInterval?: number }).__deckInterval = interval as unknown as number;
  });

  console.log('=== Navigating to', BASE, '===');
  await page.goto(BASE, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);

  await page.getByRole('button', { name: /start a quick game|start quick game/i }).first().click();
  console.log('=== Clicked start game ===');
  await page.getByText('Game Table').waitFor({ timeout: 15000 });
  console.log('=== Game Table visible ===');
  await page.waitForTimeout(1000);

  // Find the human's player slot (contains "SOUTH – human" or type human)
  const humanSlot = page.locator('[data-testid^="player-slot-"]').filter({ hasText: 'human' }).first();
  const humanCards = humanSlot.locator('[data-testid^="card-"]');

  // Wait for the human's turn: the human slot itself gets the red ring class (ring-red-500)
  console.log('=== Waiting for human turn (red ring on human slot) ===');
  const humanTurnSlot = page
    .locator('[data-testid^="player-slot-"].ring-red-500')
    .filter({ hasText: 'human' })
    .first();
  try {
    await humanTurnSlot.waitFor({ timeout: 45000 });
    console.log('=== Human turn detected ===');
  } catch {
    console.log('=== WARNING: human turn not detected via ring; proceeding anyway ===');
  }
  await page.waitForTimeout(500);

  const count = await humanCards.count();
  console.log('=== Human card count:', count, '===');

  // Play the human's cards across rounds until a round completes. The human
  // plays one card per turn; after each human card the bots chain, then it's
  // the human's turn again (or the round completes). We keep playing the
  // human's first available card whenever it's the human's turn, and stop once
  // a round_completed WS event has been observed (so we can correlate the
  // deck-clear + winner timing against the round_pause barrier).
  console.log('=== Playing human cards until a round completes (up to 60s) ===');
  const deadline = Date.now() + 60000;
  let played = 0;
  let roundCompleted = false;
  while (Date.now() < deadline && !roundCompleted) {
    // Check whether a round_completed event has arrived
    roundCompleted = await page.evaluate(() => {
      const w = window as unknown as { __wsEvents: { type: string }[] };
      return w.__wsEvents.some((e) => e.type === 'round_completed');
    });
    if (roundCompleted) break;

    // Is it the human's turn? The human slot gets the red ring (ring-red-500).
    const humanTurn = await page
      .locator('[data-testid^="player-slot-"].ring-red-500')
      .filter({ hasText: 'human' })
      .count();
    if (humanTurn > 0) {
      const cards = page
        .locator('[data-testid^="player-slot-"]')
        .filter({ hasText: 'human' })
        .first()
        .locator('[data-testid^="card-"]');
      const n = await cards.count();
      if (n > 0) {
        const clickT = await page.evaluate(() => Date.now() - (window as unknown as { __wsStart: number }).__wsStart);
        console.log(`=== Human card click #${played + 1} at t=${clickT}ms ===`);
        await cards.first().click();
        played++;
        // Wait for the bot chain to apply (3 bots * 800ms + round_pause)
        await page.waitForTimeout(4000);
        continue;
      }
    }
    await page.waitForTimeout(300);
  }
  console.log(`=== Stopped after ${played} human card plays; roundCompleted=${roundCompleted} ===`);
  // Give the round_pause barrier time to clear the deck + show the winner
  await page.waitForTimeout(3000);

  // Collect data
  const data = await page.evaluate(() => {
    const w = window as unknown as {
      __wsEvents: { t: number; type: string; player_id?: string; next_turn?: string; rank?: number | null }[];
      __deckEvents: { t: number; slot: number; card: string }[];
      __deckClears: { t: number; slot: number }[];
      __winnerEvents: { t: number; text: string }[];
      __deckInterval?: number;
      __players: Record<string, string>;
      __snapshots: { t: number; rank: number | null; players: { id: string; name: string; pos: number; dpos: number; type: string }[] }[];
      __playResults: { t: number; ok: boolean; status?: number; body?: string; player_id?: string; card_index?: number }[];
    };
    if (w.__deckInterval) clearInterval(w.__deckInterval);
    return { ws: w.__wsEvents, deck: w.__deckEvents, clears: w.__deckClears, winners: w.__winnerEvents, players: w.__players, snapshots: w.__snapshots, plays: w.__playResults };
  });

  console.log('=== PLAYERS (id -> name|type|pos|dpos) ===');
  for (const [id, info] of Object.entries(data.players)) {
    console.log(`  ${id} -> ${info}`);
  }

  console.log('=== SNAPSHOTS (t, rank, players pos/dpos) ===');
  for (const s of data.snapshots) {
    const human = s.players.find((p) => p.type === 'human');
    console.log(`  t=${s.t}ms rank=${s.rank} human(pos=${human?.pos},dpos=${human?.dpos})`);
  }

  console.log('=== PLAY REQUESTS / RESPONSES ===');
  for (const p of data.plays) {
    console.log(`  t=${p.t}ms ok=${p.ok} status=${p.status} player=${p.player_id} card=${p.card_index} body=${p.body}`);
  }

  console.log('=== WEB SOCKET EVENT ARRIVAL TIMES (ms after page load) ===');
  for (const e of data.ws) {
    console.log(`  t=${e.t}ms type=${e.type} player=${e.player_id ?? ''} next=${e.next_turn ?? ''} rank=${e.rank ?? ''}`);
  }

  console.log('=== DECK CARD APPLICATION TIMES (ms after page load) ===');
  for (const e of data.deck) {
    console.log(`  t=${e.t}ms slot=${e.slot} card=${e.card}`);
  }

  console.log('=== GAPS BETWEEN DECK CARD APPLICATIONS ===');
  for (let i = 1; i < data.deck.length; i++) {
    console.log(`  gap ${i - 1}->${i}: ${data.deck[i].t - data.deck[i - 1].t}ms`);
  }

  console.log('=== DECK CLEAR EVENTS (slot went filled -> empty) ===');
  for (const e of data.clears) {
    console.log(`  t=${e.t}ms slot=${e.slot} cleared`);
  }

  console.log('=== ROUND WINNER DISPLAY EVENTS ===');
  for (const e of data.winners) {
    console.log(`  t=${e.t}ms text="${e.text}"`);
  }

  // Correlate: for each round_completed WS event, show when deck cleared and when last card applied
  console.log('=== ROUND COMPLETION TIMELINE (round_completed vs deck clear vs last card) ===');
  for (const e of data.ws) {
    if (e.type === 'round_completed') {
      const clearAfter = data.clears.filter((c) => c.t >= e.t).map((c) => c.t - e.t);
      const lastCardBefore = data.deck.filter((d) => d.t <= e.t + 100).pop();
      console.log(`  round_completed at t=${e.t}ms; deck clears at +${clearAfter.join(',+')}ms; last card applied at t=${lastCardBefore?.t ?? '?'}ms (${lastCardBefore?.card ?? ''})`);
    }
  }

  await page.screenshot({ path: 'tests/live-quickie-screenshot.png', fullPage: true });
  console.log('=== Screenshot saved ===');

  expect(data.deck.length).toBeGreaterThanOrEqual(1);
});
