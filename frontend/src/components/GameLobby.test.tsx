import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

const mockGet = vi.fn();
const mockPost = vi.fn();

vi.mock('axios', () => ({
  default: {
    get: (...args: unknown[]) => mockGet(...args),
    post: (...args: unknown[]) => mockPost(...args),
  },
}));

vi.mock('../stores/useAuthStore', () => ({
  useAuthStore: vi.fn(() => ({
    isAuthenticated: true,
    user: { pseudo: 'Alice', email: 'alice@test.com' },
    isLoaded: true,
  })),
}));

let mockOnMessage: ((event: { type: string; [key: string]: unknown }) => void) | null = null;
const mockSend = vi.fn();

vi.mock('../hooks/useWebSocket', () => ({
  useWebSocket: vi.fn(({ onMessage }: {
    gameId: string;
    playerId?: string;
    playerPosition?: number;
    onMessage?: (event: { type: string; [key: string]: unknown }) => void;
    onError?: (error: Event) => void;
    onClose?: (event: CloseEvent) => void;
    autoReconnect?: boolean;
  }) => {
    mockOnMessage = onMessage || null;
    return {
      isConnected: true,
      lastError: null,
      send: mockSend,
      reconnect: vi.fn(),
    };
  }),
  GameEvent: {} as never,
}));

import GameLobby from './GameLobby';

describe('GameLobby', () => {
  let onBack: ReturnType<typeof vi.fn>;
  let onGameStart: ReturnType<typeof vi.fn>;

  const LOBBY_RESPONSE = {
    status: 'pending',
    bet: 10,
    max_players: 4,
    players: [
      { name: 'HostPlayer', position: 0, is_current_user: true },
    ],
    invite_expires_at: new Date(Date.now() + 600000).toISOString(),
  };

  beforeEach(() => {
    onBack = vi.fn();
    onGameStart = vi.fn();
    mockOnMessage = null;
    mockGet.mockReset();
    mockPost.mockReset();
    mockSend.mockReset();

    mockGet.mockResolvedValue({ data: { ...LOBBY_RESPONSE } });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  function renderLobby(gameId = 'test-game-id') {
    return render(
      <GameLobby gameId={gameId} onBack={onBack} onGameStart={onGameStart} />
    );
  }

  describe('Initial load', () => {
    it('fetches lobby data on mount', async () => {
      renderLobby();
      await waitFor(() => {
        expect(mockGet).toHaveBeenCalledWith('/api/me/games/test-game-id');
      });
    });

    it('displays bet from fetched data', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('Bet: 10 credits')).toBeInTheDocument();
      });
    });

    it('displays player count', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText(/Waiting for players.../)).toBeInTheDocument();
      });
    });

    it('shows host player in the lobby', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('HostPlayer')).toBeInTheDocument();
      });
    });
  });

  describe('WebSocket subscriptions', () => {
    it('subscribes to WebSocket with correct gameId', async () => {
      const { useWebSocket } = await import('../hooks/useWebSocket');
      renderLobby('ws-game-id');
      await waitFor(() => {
        expect(useWebSocket).toHaveBeenCalledWith(
          expect.objectContaining({ gameId: 'ws-game-id' })
        );
      });
    });

    it('subscribes with autoReconnect enabled', async () => {
      const { useWebSocket } = await import('../hooks/useWebSocket');
      renderLobby();
      await waitFor(() => {
        expect(useWebSocket).toHaveBeenCalledWith(
          expect.objectContaining({ autoReconnect: true })
        );
      });
    });
  });

  describe('player_joined event', () => {
    it('adds a new player to the lobby on player_joined', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('HostPlayer')).toBeInTheDocument();
      });

      act(() => {
        mockOnMessage?.({
          type: 'player_joined',
          game_id: 'g1',
          player_id: 'p2',
          user_id: 'u2',
          pseudo: 'NewPlayer',
          position: 1,
          player_count: 2,
          max_players: 4,
        });
      });

      await waitFor(() => {
        expect(screen.getByText('NewPlayer')).toBeInTheDocument();
      });
    });

    it('does not add duplicate players (idempotency)', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('HostPlayer')).toBeInTheDocument();
      });

      act(() => {
        mockOnMessage?.({
          type: 'player_joined',
          game_id: 'g1',
          player_id: 'p1',
          user_id: 'u1',
          pseudo: 'DuplicatePlayer',
          position: 1,
          player_count: 2,
          max_players: 4,
        });
      });

      await waitFor(() => {
        expect(screen.getByText('DuplicatePlayer')).toBeInTheDocument();
      });

      act(() => {
        mockOnMessage?.({
          type: 'player_joined',
          game_id: 'g1',
          player_id: 'p1_again',
          user_id: 'u1_again',
          pseudo: 'DuplicateAgain',
          position: 1,
          player_count: 3,
          max_players: 4,
        });
      });

      await waitFor(() => {
        const allDuplicate = screen.getAllByText('DuplicatePlayer');
        expect(allDuplicate).toHaveLength(1);
      });
    });

    it('updates player count display', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('HostPlayer')).toBeInTheDocument();
      });

      act(() => {
        mockOnMessage?.({
          type: 'player_joined',
          game_id: 'g1',
          player_id: 'p2',
          user_id: 'u2',
          pseudo: 'NewPlayer',
          position: 1,
          player_count: 2,
          max_players: 4,
        });
      });

      await waitFor(() => {
        expect(screen.getByText(/2\/4/)).toBeInTheDocument();
      });
    });
  });

  describe('game_ready event', () => {
    it('shows ready state message', async () => {
      renderLobby();

      act(() => {
        mockOnMessage?.({
          type: 'player_joined',
          game_id: 'g1',
          player_id: 'p2',
          user_id: 'u2',
          pseudo: 'Player2',
          position: 1,
          player_count: 2,
          max_players: 4,
        });
      });

      act(() => {
        mockOnMessage?.({
          type: 'game_ready',
          game_id: 'g1',
        });
      });

      await waitFor(() => {
        expect(screen.getByText('All players have joined. Start the game!')).toBeInTheDocument();
      });
    });
  });

  describe('game_cancelled event', () => {
    it('shows toast and calls onBack', async () => {
      renderLobby();
      await waitFor(() => {
        expect(mockGet).toHaveBeenCalled();
      });

      act(() => {
        mockOnMessage?.({
          type: 'game_cancelled',
          game_id: 'g1',
          reason: 'Not enough players',
        });
      });

      await waitFor(() => {
        expect(screen.getByText('Not enough players')).toBeInTheDocument();
      });

      expect(onBack).toHaveBeenCalled();
    });

    it('uses default reason when none provided', async () => {
      renderLobby();
      await waitFor(() => {
        expect(mockGet).toHaveBeenCalled();
      });

      act(() => {
        mockOnMessage?.({
          type: 'game_cancelled',
          game_id: 'g1',
          reason: '',
        });
      });

      await waitFor(() => {
        expect(screen.getByText('Game has been cancelled.')).toBeInTheDocument();
      });
    });
  });

  describe('game_started event', () => {
    it('calls onGameStart with converted game data', async () => {
      renderLobby();
      // Wait for lobby data to fully load (including bet state update)
      await waitFor(() => {
        expect(screen.getByText('Bet: 10 credits')).toBeInTheDocument();
      });

      act(() => {
        mockOnMessage?.({
          type: 'game_started',
          game_id: 'g1',
          players: [
            {
              id: 'p1',
              name: 'Alice',
              position: 0,
              display_position: 0,
              cards_count: 5,
              player_type: 'human',
            },
            {
              id: 'p2',
              name: 'Bob',
              position: 1,
              display_position: 1,
              cards_count: 5,
              player_type: 'bot',
            },
          ],
          current_turn: 'p1',
        });
      });

      await waitFor(() => {
        expect(onGameStart).toHaveBeenCalledWith({
          game_id: 'g1',
          players: [
            {
              id: 'p1',
              type: 'human',
              name: 'Alice',
              position: 0,
              display_position: 0,
              cards: [],
              cards_count: 5,
            },
            {
              id: 'p2',
              type: 'bot',
              name: 'Bob',
              position: 1,
              display_position: 1,
              cards: [],
              cards_count: 5,
            },
          ],
          status: 'active',
          current_turn: 0,
          bet: 10,
        });
      });
    });

    it('maps current_turn UUID to display_position', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('Bet: 10 credits')).toBeInTheDocument();
      });

      act(() => {
        mockOnMessage?.({
          type: 'game_started',
          game_id: 'g1',
          players: [
            { id: 'p1', name: 'Alice', position: 0, display_position: 0, cards_count: 5, player_type: 'human' },
            { id: 'p2', name: 'Bob', position: 1, display_position: 1, cards_count: 5, player_type: 'bot' },
            { id: 'p3', name: 'Charlie', position: 2, display_position: 2, cards_count: 5, player_type: 'bot' },
          ],
          current_turn: 'p3',
        });
      });

      await waitFor(() => {
        expect(onGameStart).toHaveBeenCalledWith(
          expect.objectContaining({ current_turn: 2 })
        );
      });
    });

    it('defaults current_turn to 0 when turn player not found', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('Bet: 10 credits')).toBeInTheDocument();
      });

      act(() => {
        mockOnMessage?.({
          type: 'game_started',
          game_id: 'g1',
          players: [
            { id: 'p1', name: 'Alice', position: 0, display_position: 0, cards_count: 5, player_type: 'human' },
          ],
          current_turn: 'non-existent-id',
        });
      });

      await waitFor(() => {
        expect(onGameStart).toHaveBeenCalledWith(
          expect.objectContaining({ current_turn: 0 })
        );
      });
    });

    it('uses bet from lobby state', async () => {
      mockGet.mockResolvedValue({
        data: { ...LOBBY_RESPONSE, bet: 50 },
      });

      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('Bet: 50 credits')).toBeInTheDocument();
      });

      act(() => {
        mockOnMessage?.({
          type: 'game_started',
          game_id: 'g1',
          players: [
            { id: 'p1', name: 'Alice', position: 0, display_position: 0, cards_count: 5, player_type: 'human' },
          ],
          current_turn: 'p1',
        });
      });

      await waitFor(() => {
        expect(onGameStart).toHaveBeenCalledWith(
          expect.objectContaining({ bet: 50 })
        );
      });
    });
  });

  describe('Start Game button', () => {
    it('shows Start Game button for creator when status is ready', async () => {
      mockGet.mockResolvedValue({
        data: {
          ...LOBBY_RESPONSE,
          status: 'ready',
          players: [
            { name: 'HostPlayer', position: 0, is_current_user: true },
            { name: 'Player2', position: 1, is_current_user: false },
          ],
        },
      });

      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('Start Game')).toBeInTheDocument();
      });
    });

    it('calls start endpoint on button click', async () => {
      mockGet.mockResolvedValue({
        data: {
          ...LOBBY_RESPONSE,
          status: 'ready',
          players: [
            { name: 'HostPlayer', position: 0, is_current_user: true },
            { name: 'Player2', position: 1, is_current_user: false },
          ],
        },
      });
      mockPost.mockResolvedValue({ data: { game_id: 'g1', status: 'active', players: [], bet: 10, current_turn: 0 } });

      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('Start Game')).toBeInTheDocument();
      });

      await userEvent.click(screen.getByText('Start Game'));

      await waitFor(() => {
        expect(mockPost).toHaveBeenCalledWith('/api/games/test-game-id/start');
      });
    });
  });

  describe('Back button', () => {
    it('calls onBack when clicking Back button', async () => {
      renderLobby();
      await waitFor(() => {
        expect(screen.getByText('Back to Dashboard')).toBeInTheDocument();
      });

      await userEvent.click(screen.getByText('Back to Dashboard'));
      expect(onBack).toHaveBeenCalled();
    });
  });
});
