import { describe, it, expect } from 'vitest';
import type { GameEvent, GameStartedPlayer } from './useWebSocket';

const VALID_EVENT_TYPES = [
  'card_played',
  'round_completed',
  'game_finished',
  'turn_changed',
  'player_joined',
  'game_cancelled',
  'game_ready',
  'game_started',
  'player_disconnected',
  'player_reconnected',
];

function isValidGameEvent(data: { type?: string }): boolean {
  return !!(data.type && VALID_EVENT_TYPES.includes(data.type));
}

function createGameStartedEvent(): GameEvent {
  return {
    type: 'game_started',
    game_id: '550e8400-e29b-41d4-a716-446655440000',
    players: [{
      id: '11111111-1111-1111-1111-111111111111',
      name: 'Alice',
      position: 0,
      display_position: 0,
      cards_count: 5,
    }],
    current_turn: '11111111-1111-1111-1111-111111111111',
  };
}

describe('useWebSocket - GameEvent validation', () => {
  describe('Message type validation', () => {
    it('accepts game_started event type', () => {
      expect(isValidGameEvent({ type: 'game_started' })).toBe(true);
    });

    it('accepts all known lobby events', () => {
      for (const type of ['player_joined', 'game_ready', 'game_cancelled', 'game_started']) {
        expect(isValidGameEvent({ type })).toBe(true);
      }
    });

    it('accepts all known gameplay events', () => {
      for (const type of ['card_played', 'round_completed', 'game_finished', 'turn_changed']) {
        expect(isValidGameEvent({ type })).toBe(true);
      }
    });

    it('accepts connection events', () => {
      expect(isValidGameEvent({ type: 'player_disconnected' })).toBe(true);
      expect(isValidGameEvent({ type: 'player_reconnected' })).toBe(true);
    });

    it('rejects unknown event types', () => {
      expect(isValidGameEvent({ type: 'unknown_event' })).toBe(false);
    });

    it('rejects messages without a type field', () => {
      expect(isValidGameEvent({})).toBe(false);
    });
  });

  describe('GameEvent type structure', () => {
    it('creates a valid game_started event with correct shape', () => {
      const event = createGameStartedEvent();
      expect(event.type).toBe('game_started');
      expect(event.game_id).toBeTruthy();

      if (event.type === 'game_started') {
        expect(event.players).toHaveLength(1);
        expect(event.players[0].name).toBe('Alice');
        expect(event.players[0].cards_count).toBe(5);
        expect(event.current_turn).toBeTruthy();
      }
    });

    it('GameStartedPlayer type has correct fields', () => {
      const player: GameStartedPlayer = {
        id: 'p1',
        name: 'Test',
        position: 0,
        display_position: 0,
        cards_count: 5,
      };
      expect(typeof player.id).toBe('string');
      expect(typeof player.name).toBe('string');
      expect(typeof player.position).toBe('number');
      expect(typeof player.display_position).toBe('number');
      expect(typeof player.cards_count).toBe('number');
    });
  });

  describe('Valid event types list', () => {
    it('contains exactly 10 event types', () => {
      expect(VALID_EVENT_TYPES).toHaveLength(10);
    });

    it('includes game_started in the valid types', () => {
      expect(VALID_EVENT_TYPES).toContain('game_started');
    });

    it('has no duplicates', () => {
      const unique = new Set(VALID_EVENT_TYPES);
      expect(unique.size).toBe(VALID_EVENT_TYPES.length);
    });
  });
});
