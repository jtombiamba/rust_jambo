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
      store.addPendingEvent({ kind: 'round_pause' });
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
      store.addPendingEvent({ kind: 'round_pause' });
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
      store.addPendingEvent({ kind: 'round_pause' });
      store.startBotReplay(100, 500);

      vi.runAllTimers();

      expect(useGameStore.getState().isReplayingBots).toBe(false);
      expect(useGameStore.getState().pendingBotMoves).toEqual([]);
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
      store.addPendingEvent({ kind: 'round_pause' });
      store.addPendingEvent({ kind: 'bot_play', playerId: 'c', cardIndex: 12, nextTurnPlayerId: 'a' });
      store.flushPendingEvents();
      const state = useGameStore.getState();
      expect(state.pendingBotMoves).toEqual([]);
      expect(state.isBotChainActive).toBe(false);
      expect(state.isReplayingBots).toBe(false);
    });
  });
});
