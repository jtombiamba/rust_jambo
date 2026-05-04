import React from 'react';
import './WinnerRing.css';

export interface WinnerRingProps {
  /** Position of the player slot (north, south, east, west) */
  position: 'north' | 'south' | 'east' | 'west';
  /** Whether the ring should be visible */
  isVisible: boolean;
  /** Type of win (normal, kora, doubleKora) */
  winType?: 'normal' | 'kora' | 'doubleKora';
  /** Optional player name to display */
  playerName?: string;
}

/**
 * A visual indicator that highlights the player who won the current round.
 * Shows a glowing ring around the player's slot with animations based on win type.
 */
const WinnerRing: React.FC<WinnerRingProps> = ({
  position,
  isVisible,
  winType = 'normal',
  playerName,
}) => {
  if (!isVisible) {
    return null;
  }

  // Determine CSS classes based on win type
  const ringClass = `winner-ring winner-ring-${position} ${
    winType === 'kora' ? 'winner-ring-kora' : 
    winType === 'doubleKora' ? 'winner-ring-double-kora' : ''
  }`;

  // Determine label text based on win type
  const winLabel = winType === 'normal' ? 'Winner!' :
                   winType === 'kora' ? 'KORA!' :
                   'DOUBLE KORA!';

  return (
    <div className={ringClass}>
      {/* Ring border */}
      <div className="winner-ring-border"></div>
      
      {/* Win type label */}
      <div className="winner-ring-label">
        {playerName && <div className="winner-ring-player-name">{playerName}</div>}
        <div className="winner-ring-win-type">{winLabel}</div>
      </div>
    </div>
  );
};

export default WinnerRing;