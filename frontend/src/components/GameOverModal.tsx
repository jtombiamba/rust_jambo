import React, { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
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
  /** If part of a game run, the next game action */
  onPlayNext?: () => void;
  /** If part of a game run, whether this is the last game */
  isLastGame?: boolean;
  /** Current game index in run (1-based) */
  runGameIndex?: number;
  /** Total games in run */
  runTotalGames?: number;
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
  onPlayNext,
  isLastGame,
  runGameIndex,
  runTotalGames,
}) => {
  const { t } = useTranslation();

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
        return t('game.kora');
      case 'doubleKora':
        return t('game.doubleKora');
      default:
        return t('game.gameOver');
    }
  };

  // Get result description based on status
  const getResultDescription = () => {
    switch (gameResult.status) {
      case 'kora':
        return t('game.koraDescription');
      case 'doubleKora':
        return t('game.doubleKoraDescription');
      default:
        return t('game.gameConcluded');
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
                <div className="winner-label winner-label-animate">{t('game.winsTheGame')}</div>
                <div className="winner-position winner-position-animate">{t('game.position', { pos: winner.position })}</div>
              </>
            ) : (
              <div className="no-winner">{t('game.noWinner')}</div>
            )}
          </div>

          {/* Action buttons */}
          <div className="action-buttons buttons-animate">
            {onPlayNext && (
            <button
              className="btn-primary"
              onClick={onPlayNext}
              autoFocus
            >
              {isLastGame ? 'Finish Run' : `Play Next (${runGameIndex ?? '?'}/${runTotalGames ?? '?'})`}
            </button>
            )}
            {showPlayAgain && !onPlayNext && (
            <button
              className="btn-primary"
              onClick={() => {
                onPlayAgain()
                onClose()
              }}
              autoFocus
            >
              {t('game.playAgain')}
            </button>
            )}
            <button
              className="btn-secondary"
              onClick={onReturnToLobby}
            >
              {t('game.returnToLobby')}
            </button>
            <button
              className="btn-tertiary"
              onClick={onClose}
            >
              {t('common.close')}
            </button>
          </div>

        </div>
      </div>
    </>
  );
};

export default GameOverModal;
