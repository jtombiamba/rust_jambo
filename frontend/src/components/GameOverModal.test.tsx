import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import GameOverModal from './GameOverModal';
import type { Player, GameResult } from '../stores/useGameStore';

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key,
  }),
}));

const winner: Player = {
  id: 'p1',
  type: 'human',
  name: 'Alice',
  position: 1,
  display_position: 1,
  cards: [],
  cards_count: 0,
};

const gameResult: GameResult = {
  status: 'finished',
  roundsPlayed: 1,
};

const baseProps = {
  isOpen: true,
  onClose: vi.fn(),
  winner,
  gameResult,
  onPlayAgain: vi.fn(),
  onReturnToLobby: vi.fn(),
};

describe('GameOverModal', () => {
  it('shows the Play Again button when showPlayAgain is true', () => {
    render(<GameOverModal {...baseProps} showPlayAgain />);
    expect(screen.getByText('game.playAgain')).toBeInTheDocument();
  });

  it('hides the Play Again button when showPlayAgain is false', () => {
    render(<GameOverModal {...baseProps} showPlayAgain={false} />);
    expect(screen.queryByText('game.playAgain')).not.toBeInTheDocument();
  });

  it('hides the Play Again button when onPlayNext is provided', () => {
    render(<GameOverModal {...baseProps} onPlayNext={vi.fn()} />);
    expect(screen.queryByText('game.playAgain')).not.toBeInTheDocument();
  });
});
