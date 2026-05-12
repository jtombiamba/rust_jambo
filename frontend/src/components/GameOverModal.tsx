import React, { useEffect } from 'react';
import { Player, GameResult } from '../stores/useGameStore';
import './GameOverModal.css';

export interface GameOverModalProps {
  /** Whether the modal is open */
  isOpen: boolean;
  /** Function to close the modal */
  onClose: () => void;
  /** Winner player information */
  winner: Player | null;
  /** Game result details */
  gameResult: GameResult;
  /** Callback for Play Again action */
  onPlayAgain: () => void;
  /** Callback for Return to Lobby action */
  onReturnToLobby: () => void;
  /** Whether to show the Play Again button (hidden for human-only multiplayer) */
  showPlayAgain?: boolean;
}

/**
 * Modal that appears when a game finishes, announcing the winner and game result.
 * Shows different styling based on win type (normal, kora, doubleKora).
 */
const GameOverModal: React.FC<GameOverModalProps> = ({
  isOpen,
  onClose,
  winner,
  gameResult,
  onPlayAgain,
  onReturnToLobby,
  showPlayAgain = true,
}) => {
  // Close modal on Escape key press
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleEscape);
    return () => window.removeEventListener('keydown', handleEscape);
  }, [isOpen, onClose]);

  // Auto-close after 10 seconds
  useEffect(() => {
    if (!isOpen) return;
    const timer = setTimeout(() => {
      onClose();
    }, 10000);
    return () => clearTimeout(timer);
  }, [isOpen, onClose]);

  if (!isOpen) {
    return null;
  }

  // Determine modal styling based on game result status
  const modalClass = `game-over-modal ${
    gameResult.status === 'kora' ? 'game-over-modal-kora' :
    gameResult.status === 'doubleKora' ? 'game-over-modal-double-kora' : ''
  }`;

  // Get result title based on status
  const getResultTitle = () => {
    switch (gameResult.status) {
      case 'kora':
        return 'KORA!';
      case 'doubleKora':
        return 'DOUBLE KORA!';
      default:
        return 'Game Over!';
    }
  };

  // Get result description based on status
  const getResultDescription = () => {
    switch (gameResult.status) {
      case 'kora':
        return 'A spectacular Kora victory!';
      case 'doubleKora':
        return 'An incredible Double Kora achievement!';
      default:
        return 'The game has concluded.';
    }
  };

  return (
    <>
      {/* Backdrop */}
      <div className="game-over-backdrop" onClick={onClose} />

      {/* Modal */}
      <div className={modalClass} role="dialog" aria-labelledby="game-over-title">
        <div className="game-over-content">
          {/* Header with trophy icon */}
          <div className="result-header">
            <div className="trophy-icon">
              {gameResult.status === 'doubleKora' ? '🏆🏆' :
               gameResult.status === 'kora' ? '🏆' : '🎯'}
            </div>
            <h2 id="game-over-title" className="result-title">
              {getResultTitle()}
            </h2>
            <p className="result-description">{getResultDescription()}</p>
          </div>

          {/* Winner announcement */}
          <div className="winner-announcement winner-declare">
            {winner ? (
              <>
                <div className="winner-name winner-name-animate">{winner.name}</div>
                <div className="winner-label winner-label-animate">Wins the Game!</div>
                <div className="winner-position winner-position-animate">Position: {winner.position}</div>
              </>
            ) : (
              <div className="no-winner">No winner determined</div>
            )}
          </div>

          {/* Game statistics */}
          <div className="game-stats stats-animate">
            <div className="stat-item">
              <div className="stat-label">Rounds Played</div>
              <div className="stat-value">{gameResult.roundsPlayed}</div>
            </div>
            {gameResult.finalScore !== undefined && (
              <div className="stat-item">
                <div className="stat-label">Final Score</div>
                <div className="stat-value">{gameResult.finalScore}</div>
              </div>
            )}
            <div className="stat-item">
              <div className="stat-label">Game Type</div>
              <div className="stat-value">
                {gameResult.status === 'finished' ? 'Standard' :
                 gameResult.status === 'kora' ? 'Kora' : 'Double Kora'}
              </div>
            </div>
          </div>

          {/* Action buttons */}
          <div className="action-buttons buttons-animate">
            {showPlayAgain && (
            <button
              className="btn-primary"
              onClick={onPlayAgain}
              autoFocus
            >
              Play Again
            </button>
            )}
            <button
              className="btn-secondary"
              onClick={onReturnToLobby}
            >
              Return to Lobby
            </button>
            <button
              className="btn-tertiary"
              onClick={onClose}
            >
              Close
            </button>
          </div>

          {/* Auto-close notice */}
          <div className="auto-close-notice">
            This modal will auto-close in 10 seconds
          </div>
        </div>
      </div>
    </>
  );
};

export default GameOverModal;
