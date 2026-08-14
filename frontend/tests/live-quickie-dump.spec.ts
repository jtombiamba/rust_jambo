import { test } from '@playwright/test';

const BASE = 'http://localhost:3000';

test('LIVE: dump game table DOM structure', async ({ page }) => {
  test.setTimeout(60000);
  await page.goto(BASE, { waitUntil: 'networkidle' });
  await page.waitForTimeout(1500);

  await page.getByRole('button', { name: /start a quick game|start quick game/i }).first().click();
  await page.waitForTimeout(1500);
  await page.getByText('Game Table').waitFor({ timeout: 15000 });
  await page.waitForTimeout(1000);

  // Dump all player slots and their inner structure
  const slots = await page.locator('[data-testid^="player-slot-"]').evaluateAll((els) =>
    els.map((el) => {
      const testid = el.getAttribute('data-testid');
      const cards = Array.from(el.querySelectorAll('[data-testid^="card-"]')).map((c) => c.getAttribute('data-testid'));
      const text = (el.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 120);
      return { testid, cards, text };
    })
  );
  console.log('=== PLAYER SLOTS ===');
  console.log(JSON.stringify(slots, null, 2));

  // Dump deck slots
  const deck = await page.locator('[data-testid^="deck-slot-"]').evaluateAll((els) =>
    els.map((el) => {
      const testid = el.getAttribute('data-testid');
      const cards = Array.from(el.querySelectorAll('[data-testid^="card-"]')).map((c) => c.getAttribute('data-testid'));
      const text = (el.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 60);
      return { testid, cards, text };
    })
  );
  console.log('=== DECK SLOTS ===');
  console.log(JSON.stringify(deck, null, 2));

  // Dump the "Turn:" indicator
  const turnText = await page.locator('body').innerText().then((t) => {
    const m = t.match(/Turn: Player \d+/);
    return m ? m[0] : 'no turn indicator';
  });
  console.log('=== TURN:', turnText, '===');
});
