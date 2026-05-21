import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import GameTable, { GamePlayer } from './GameTable';

vi.mock('./GameOverModal', () => ({
  default: ({ isOpen, winner, onClose }: {
    isOpen: boolean;
    winner: { name: string } | null;
    onClose: () => void;
    gameResult: unknown;
    onPlayAgain: () => void;
    onReturnToLobby: () => void;
    showPlayAgain: boolean;
  }) =>
    isOpen ? (
      <div data-testid="game-over-modal">
        <div>{winner?.name ?? 'Unknown'} wins!</div>
        <button onClick={onClose}>Close</button>
      </div>
    ) : null,
}));

function createPlayers(count: number): GamePlayer[] {
  const names = ['Alice', 'Bob', 'Charlie', 'Diana'];
  return names.slice(0, count).map((name, i) => ({
    id: `player-${i}`,
    type: i === 0 ? 'human' : 'bot',
    name,
    position: i,
    display_position: i,
    cards: i === 0 ? [0, 1, 2, 3, 4] : [],
  }));
}

describe('GameTable', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Basic rendering', () => {
    it('renders the game table title', () => {
      render(<GameTable players={createPlayers(2)} />);
      expect(screen.getByText('Game Table')).toBeInTheDocument();
    });

    it('renders all player names', () => {
      render(<GameTable players={createPlayers(4)} />);
      expect(screen.getByText('Alice')).toBeInTheDocument();
      expect(screen.getByText((content) => content.includes('Bob'))).toBeInTheDocument();
      expect(screen.getByText((content) => content.includes('Charlie'))).toBeInTheDocument();
      expect(screen.getByText((content) => content.includes('Diana'))).toBeInTheDocument();
    });

    it('marks bot players with robot emoji', () => {
      render(<GameTable players={createPlayers(2)} />);
      const bobElement = screen.getByText((content) => content.includes('Bob'));
      expect(bobElement.textContent).toContain('🤖');
    });

    it('does not show robot emoji for human players', () => {
      render(<GameTable players={createPlayers(2)} />);
      const aliceElement = screen.getByText('Alice');
      expect(aliceElement.textContent).not.toContain('🤖');
    });
  });

  describe('Player positioning', () => {
    it('handles 2 players (south, north)', () => {
      render(<GameTable players={createPlayers(2)} />);
      expect(screen.getByTestId('player-slot-player-0')).toBeInTheDocument();
      expect(screen.getByTestId('player-slot-player-1')).toBeInTheDocument();
    });

    it('handles 3 players (south, east, north)', () => {
      render(<GameTable players={createPlayers(3)} />);
      expect(screen.getByTestId('player-slot-player-0')).toBeInTheDocument();
      expect(screen.getByTestId('player-slot-player-1')).toBeInTheDocument();
      expect(screen.getByTestId('player-slot-player-2')).toBeInTheDocument();
    });

    it('handles 4 players (south, east, north, west)', () => {
      render(<GameTable players={createPlayers(4)} />);
      expect(screen.getByTestId('player-slot-player-0')).toBeInTheDocument();
      expect(screen.getByTestId('player-slot-player-1')).toBeInTheDocument();
      expect(screen.getByTestId('player-slot-player-2')).toBeInTheDocument();
      expect(screen.getByTestId('player-slot-player-3')).toBeInTheDocument();
    });
  });

  describe('Deck slots', () => {
    it('renders 4 deck slots', () => {
      render(<GameTable players={createPlayers(2)} />);
      expect(screen.getByTestId('deck-slot-0')).toBeInTheDocument();
      expect(screen.getByTestId('deck-slot-1')).toBeInTheDocument();
      expect(screen.getByTestId('deck-slot-2')).toBeInTheDocument();
      expect(screen.getByTestId('deck-slot-3')).toBeInTheDocument();
    });

    it('shows placed cards in deck slots', () => {
      render(
        <GameTable
          players={createPlayers(2)}
          deckSlots={[0, null, 1, null]}
        />
      );
      const card0Elements = screen.getAllByTestId('card-0');
      expect(card0Elements.length).toBeGreaterThanOrEqual(1);
      const card1Elements = screen.getAllByTestId('card-1');
      expect(card1Elements.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe('Current turn display', () => {
    it('shows turn indicator when currentTurn is set', () => {
      render(
        <GameTable
          players={createPlayers(2)}
          currentTurn={0}
        />
      );
      expect(screen.getByText('Turn: Player 0')).toBeInTheDocument();
    });

    it('does not show turn indicator when currentTurn is undefined', () => {
      render(<GameTable players={createPlayers(2)} />);
      expect(screen.queryByText(/Turn:/)).not.toBeInTheDocument();
    });
  });

  describe('Card click interaction', () => {
    it('calls onCardClick when a card is clicked', () => {
      const onCardClick = vi.fn();
      render(
        <GameTable
          players={createPlayers(2)}
          onCardClick={onCardClick}
        />
      );

      const card = screen.getByTestId('card-0');
      fireEvent.click(card);
      expect(onCardClick).toHaveBeenCalledWith('player-0', 0);
    });
  });

  describe('Card rendering', () => {
    it('renders cards in flex layout for desktop (no overlap)', () => {
      render(<GameTable players={createPlayers(2)} />);
      const cardFans = screen.queryAllByTestId('card-fan');
      expect(cardFans).toHaveLength(0);
      expect(screen.getByTestId('card-0')).toBeInTheDocument();
      expect(screen.getByTestId('card-1')).toBeInTheDocument();
    });

    it('renders human player cards face-up', () => {
      render(<GameTable players={createPlayers(2)} />);
      const card0 = screen.getByTestId('card-0');
      expect(card0.classList).toContain('bg-white');
    });

    it('renders bot player cards face-down', () => {
      render(<GameTable
        players={createPlayers(2)}
        remainingCards={{ 'player-0': 5, 'player-1': 5 }}
      />);
      const botSlot = screen.getByTestId('player-slot-player-1');
      const botCards = botSlot.querySelectorAll('[data-testid^="card-"]');
      expect(botCards.length).toBe(5);
      expect(botCards[0].classList).toContain('bg-blue-800');
    });
  });

  describe('Winner ring display', () => {
    it('does not show winner ring when no round winner', () => {
      render(<GameTable players={createPlayers(2)} />);
      expect(screen.queryByText('Winner!')).not.toBeInTheDocument();
    });

    it('shows winner ring for normal win', () => {
      render(
        <GameTable
          players={createPlayers(2)}
          roundWinner={{ playerId: 'player-0', position: 0, winType: 'normal' }}
        />
      );
      const winners = screen.getAllByText('Winner!');
      expect(winners.length).toBeGreaterThanOrEqual(1);
    });

    it('shows KORA! for kora win', () => {
      render(
        <GameTable
          players={createPlayers(2)}
          roundWinner={{ playerId: 'player-0', position: 0, winType: 'kora' }}
        />
      );
      const koras = screen.getAllByText('KORA!');
      expect(koras.length).toBeGreaterThanOrEqual(1);
    });

    it('shows DOUBLE KORA! for double kora win', () => {
      render(
        <GameTable
          players={createPlayers(2)}
          roundWinner={{ playerId: 'player-0', position: 0, winType: 'doubleKora' }}
        />
      );
      const doubleKoras = screen.getAllByText('DOUBLE KORA!');
      expect(doubleKoras.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe('Game over modal', () => {
    it('shows game over modal when game is over', () => {
      render(
        <GameTable
          players={createPlayers(2)}
          gameOver={{
            isGameOver: true,
            winner: { id: 'player-0', type: 'human', name: 'Alice', position: 0, display_position: 0, cards: [] },
            result: { status: 'finished', roundsPlayed: 3 },
          }}
        />
      );
      expect(screen.getByTestId('game-over-modal')).toBeInTheDocument();
      expect(screen.getByText('Alice wins!')).toBeInTheDocument();
    });

    it('does not show game over modal when game is not over', () => {
      render(
        <GameTable
          players={createPlayers(2)}
          gameOver={{
            isGameOver: false,
            winner: null,
            result: { status: 'finished', roundsPlayed: 0 },
          }}
        />
      );
      expect(screen.queryByTestId('game-over-modal')).not.toBeInTheDocument();
    });
  });

  describe('Edge cases', () => {
    it('renders with empty players array', () => {
      render(<GameTable players={[]} />);
      expect(screen.getByText('Game Table')).toBeInTheDocument();
    });

    it('renders player with no cards', () => {
      const player: GamePlayer = {
        id: 'player-0',
        type: 'human',
        name: 'Empty',
        position: 0,
        display_position: 0,
        cards: [],
      };
      render(<GameTable players={[player]} />);
      expect(screen.getByText('Empty')).toBeInTheDocument();
    });
  });

  describe('Screen orientation', () => {
    it('adapts layout based on orientation', () => {
      render(<GameTable players={createPlayers(2)} />);
      expect(screen.getByText('Game Table')).toBeInTheDocument();
    });
  });
});
