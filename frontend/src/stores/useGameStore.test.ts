import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { useGameStore } from './useGameStore';

const makePlayer = (id: string, position: number, display_position?: number) => ({
  id,
  type: 'human' as const,
  name: `Player ${position}`,
  position,
  display_position: display_position ?? position,
  cards: [0, 1, 2, 3, 4],
});

const makeBotPlayer = (id: string, position: number, display_position?: number) => ({
  id,
  type: 'bot' as const,
  name: `Bot ${position}`,
  position,
  display_position: display_position ?? position,
  cards: [],
});

describe('useGameStore', () => {
  beforeEach(() => {
    useGameStore.getState().resetGame();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('setGame', () => {
    it('creates deckSlots from player count when no deckSlots provided', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10);
      const state = useGameStore.getState();
      expect(state.deckSlots).toEqual([null, null]);
    });

    it('creates deckSlots of correct size for 3 players', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1), makePlayer('c', 2)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10);
      const state = useGameStore.getState();
      expect(state.deckSlots).toEqual([null, null, null]);
    });

    it('uses provided deckSlots when length matches player count', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1), makePlayer('c', 2)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10, [5, null, 12]);
      const state = useGameStore.getState();
      expect(state.deckSlots).toEqual([5, null, 12]);
    });

    it('falls back to empty slots when deckSlots length mismatches', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10, [5, 12, 7]); // 3 slots, 2 players
      const state = useGameStore.getState();
      expect(state.deckSlots).toEqual([null, null]);
    });

    it('falls back to empty slots when deckSlots is null', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10, null);
      const state = useGameStore.getState();
      expect(state.deckSlots).toEqual([null, null]);
    });
  });

  describe('clearDeckSlots', () => {
    it('resets to nulls matching current player count (2 players)', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10, [8, 10]);
      expect(useGameStore.getState().deckSlots).toEqual([8, 10]);

      useGameStore.getState().clearDeckSlots();
      expect(useGameStore.getState().deckSlots).toEqual([null, null]);
    });

    it('resets to nulls matching current player count (3 players)', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1), makePlayer('c', 2)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10, [2, 5, 8]);
      expect(useGameStore.getState().deckSlots).toEqual([2, 5, 8]);

      useGameStore.getState().clearDeckSlots();
      expect(useGameStore.getState().deckSlots).toEqual([null, null, null]);
    });

    it('does not produce hardcoded 4 slots for 2-player game', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10);
      useGameStore.getState().clearDeckSlots();
      expect(useGameStore.getState().deckSlots.length).toBe(2);
    });

    it('does not produce hardcoded 4 slots for 3-player game', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1), makePlayer('c', 2)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10);
      useGameStore.getState().clearDeckSlots();
      expect(useGameStore.getState().deckSlots.length).toBe(3);
    });

    it('handles empty player list gracefully', () => {
      useGameStore.getState().clearDeckSlots();
      expect(useGameStore.getState().deckSlots).toEqual([]);
    });
  });

  describe('resetGame', () => {
    it('clears deckSlots to empty array', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1)];
      useGameStore.getState().setGame('g1', players, 'active', 0, 10, [3, 7]);
      useGameStore.getState().resetGame();
      expect(useGameStore.getState().deckSlots).toEqual([]);
      expect(useGameStore.getState().gameId).toBeNull();
    });

    it('resets bot chain state', () => {
      const players = [makePlayer('a', 0), makePlayer('b', 1)];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'a' });
      useGameStore.setState({ isBotChainActive: true, isReplayingBots: true });
      store.resetGame();
      const state = useGameStore.getState();
      expect(state.pendingBotMoves).toEqual([]);
      expect(state.isBotChainActive).toBe(false);
      expect(state.isReplayingBots).toBe(false);
    });
  });

  describe('bot delays', () => {
    it('has sensible defaults', () => {
      const state = useGameStore.getState();
      expect(state.botThinkingDelayMs).toBe(800);
      expect(state.roundPauseDelayMs).toBe(2500);
    });

    it('setBotDelays updates both values', () => {
      useGameStore.getState().setBotDelays(500, 1000);
      const state = useGameStore.getState();
      expect(state.botThinkingDelayMs).toBe(500);
      expect(state.roundPauseDelayMs).toBe(1000);
    });
  });

  describe('pending events queue', () => {
    it('addPendingEvent appends to queue', () => {
      const store = useGameStore.getState();
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'c' });
      store.addPendingEvent({ kind: 'round_pause', winner: null });
      const state = useGameStore.getState();
      expect(state.pendingBotMoves).toHaveLength(2);
      expect(state.pendingBotMoves[0].kind).toBe('bot_play');
      expect(state.pendingBotMoves[1].kind).toBe('round_pause');
    });

    it('clearPendingEvents empties queue', () => {
      const store = useGameStore.getState();
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'c' });
      store.clearPendingEvents();
      expect(useGameStore.getState().pendingBotMoves).toEqual([]);
    });

    it('cancelBotReplay clears queue and stops replay', () => {
      const players = [makePlayer('a', 0), makeBotPlayer('b', 1)];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'a' });
      useGameStore.setState({ isBotChainActive: true, isReplayingBots: true, botReplayTimerId: setTimeout(() => {}, 999) });
      store.cancelBotReplay();
      const state = useGameStore.getState();
      expect(state.pendingBotMoves).toEqual([]);
      expect(state.isBotChainActive).toBe(false);
      expect(state.isReplayingBots).toBe(false);
      expect(state.botReplayTimerId).toBeNull();
    });
  });

  describe('bot replay', () => {
    it('does not start replay if queue is empty', () => {
      const store = useGameStore.getState();
      store.startBotReplay(800, 2500);
      expect(useGameStore.getState().isReplayingBots).toBe(false);
    });

    it('does not start replay if already replaying', () => {
      const players = [makePlayer('a', 0), makeBotPlayer('b', 1)];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'a' });
      useGameStore.setState({ isReplayingBots: true });
      store.startBotReplay(800, 2500);
      expect(useGameStore.getState().pendingBotMoves).toHaveLength(1);
    });

    it('applies pending bot_play via flushPendingEvents', () => {
      const players = [
        makePlayer('a', 0),
        makeBotPlayer('b', 1),
        makeBotPlayer('c', 2),
      ];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'c' });
      store.addPendingEvent({ kind: 'round_pause', winner: null });
      store.addPendingEvent({ kind: 'bot_play', playerId: 'c', cardIndex: 12, nextTurnPlayerId: 'a' });
      store.flushPendingEvents();
      const state = useGameStore.getState();
      expect(state.pendingBotMoves).toEqual([]);
      expect(state.isBotChainActive).toBe(false);
      expect(state.isReplayingBots).toBe(false);
      expect(state.currentTurn).toBe(0);
    });

    it('replays bot_play via startBotReplay with timer', () => {
      const players = [makePlayer('a', 0), makeBotPlayer('b', 1)];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'a' });
      store.startBotReplay(100, 500);

      expect(useGameStore.getState().isReplayingBots).toBe(true);

      vi.runAllTimers();

      expect(useGameStore.getState().isReplayingBots).toBe(false);
      expect(useGameStore.getState().pendingBotMoves).toEqual([]);
    });

    it('handles round_pause in replay queue', () => {
      const players = [makePlayer('a', 0), makeBotPlayer('b', 1)];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'a' });
      store.addPendingEvent({ kind: 'round_pause', winner: null });
      store.startBotReplay(100, 500);

      vi.runAllTimers();

      expect(useGameStore.getState().isReplayingBots).toBe(false);
      expect(useGameStore.getState().pendingBotMoves).toEqual([]);
    });

    it('round_pause sets winner only after last card is applied, keeping deck for collection animation', () => {
      const players = [makePlayer('a', 0), makeBotPlayer('b', 1)];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);

      // Fill the deck with the last card of the round (bot plays it).
      store.applyCardPlayed('b', 5, 'a');
      expect(useGameStore.getState().deckSlots).toEqual([5, null]);

      // Queue the round_pause carrying the winner.
      store.addPendingEvent({
        kind: 'round_pause',
        winner: { playerId: 'b', position: 1, winType: 'normal' },
      });
      store.startBotReplay(100, 500);

      // Before the round_pause is consumed, the deck is still filled and no
      // winner is shown yet.
      expect(useGameStore.getState().deckSlots).toEqual([5, null]);
      expect(useGameStore.getState().roundWinner).toBeNull();

      // Advance past the initial botDelayMs (100ms) so the round_pause is
      // consumed: the winner is set, but the deck is NOT cleared here so the
      // CardCollectionAnimation can animate the played cards toward the winner.
      // The deck is cleared externally via clearDeckSlots() once the animation
      // completes (wired through onDeckAnimationComplete in App.tsx).
      vi.advanceTimersByTime(100);
      let state = useGameStore.getState();
      expect(state.deckSlots).toEqual([5, null]);
      expect(state.roundWinner).toEqual({ playerId: 'b', position: 1, winType: 'normal' });

      // Simulate the collection animation completing: the deck clears.
      state.clearDeckSlots();
      expect(useGameStore.getState().deckSlots).toEqual([null, null]);

      // After the roundPauseMs (500ms) the winner is cleared automatically.
      vi.advanceTimersByTime(500);
      state = useGameStore.getState();
      expect(state.roundWinner).toBeNull();
      expect(state.isReplayingBots).toBe(false);
      expect(state.pendingBotMoves).toEqual([]);
    });

    it('flushPendingEvents applies all bot_play moves immediately', () => {
      const players = [
        makePlayer('a', 0),
        makeBotPlayer('b', 1),
        makeBotPlayer('c', 2),
      ];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'c' });
      store.addPendingEvent({ kind: 'round_pause', winner: null });
      store.addPendingEvent({ kind: 'bot_play', playerId: 'c', cardIndex: 12, nextTurnPlayerId: 'a' });
      store.flushPendingEvents();
      const state = useGameStore.getState();
      expect(state.pendingBotMoves).toEqual([]);
      expect(state.isBotChainActive).toBe(false);
      expect(state.isReplayingBots).toBe(false);
    });

    it('applies a deferred game-over only after the bot replay queue drains', () => {
      const players = [
        makePlayer('a', 0),
        makeBotPlayer('b', 1),
        makeBotPlayer('c', 2),
      ];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);

      // Simulate the last round: bots are queued and a game-over is deferred
      // (game_finished arrived while the bot chain was still buffering).
      store.addPendingEvent({ kind: 'bot_play', playerId: 'b', cardIndex: 5, nextTurnPlayerId: 'c' });
      store.addPendingEvent({ kind: 'bot_play', playerId: 'c', cardIndex: 12, nextTurnPlayerId: 'a' });
      store.addPendingEvent({ kind: 'round_pause', winner: null });
      store.setPendingGameOver({
        isGameOver: true,
        winner: makePlayer('a', 0),
        result: { status: 'finished', roundsPlayed: 3 },
      });

      // Before replay runs, the game-over must NOT be shown yet.
      expect(useGameStore.getState().gameOver).toBeNull();
      expect(useGameStore.getState().pendingGameOver).not.toBeNull();

      store.startBotReplay(100, 500);
      expect(useGameStore.getState().isReplayingBots).toBe(true);

      // Run all timers so the bots replay and the queue drains.
      vi.runAllTimers();

      const state = useGameStore.getState();
      expect(state.isReplayingBots).toBe(false);
      expect(state.pendingBotMoves).toEqual([]);
      // The deferred game-over is now applied.
      expect(state.pendingGameOver).toBeNull();
      expect(state.gameOver?.isGameOver).toBe(true);
    });

    it('cancelBotReplay applies a deferred game-over so it is never lost', () => {
      const players = [makePlayer('a', 0), makeBotPlayer('b', 1)];
      const store = useGameStore.getState();
      store.setGame('g1', players, 'active', 0, 10);
      store.setPendingGameOver({
        isGameOver: true,
        winner: makePlayer('a', 0),
        result: { status: 'finished', roundsPlayed: 3 },
      });

      store.cancelBotReplay();

      const state = useGameStore.getState();
      expect(state.pendingGameOver).toBeNull();
      expect(state.gameOver?.isGameOver).toBe(true);
    });
  });
});
