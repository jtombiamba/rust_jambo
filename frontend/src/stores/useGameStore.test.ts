import { describe, it, expect, beforeEach } from 'vitest';
import { useGameStore } from './useGameStore';

const makePlayer = (id: string, position: number, display_position?: number) => ({
  id,
  type: 'human' as const,
  name: `Player ${position}`,
  position,
  display_position: display_position ?? position,
  cards: [0, 1, 2, 3, 4],
});

describe('useGameStore', () => {
  beforeEach(() => {
    useGameStore.getState().resetGame();
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
  });
});
