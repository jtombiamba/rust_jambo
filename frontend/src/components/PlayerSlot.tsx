import React from 'react';
import Card from './Card';

export interface PlayerSlotProps {
  name: string;
  playerId: string;
  position: 'north' | 'south' | 'east' | 'west';
  type: 'human' | 'bot';
  /** Array of card indices (0–31) */
  cards: number[];
  /** Whether cards are face-up (true for human, false for bots) */
  cardsFaceUp: boolean;
  /** Number of remaining cards (used for bot placeholder count) */
  remainingCount?: number;
  /** Whether this player currently has the turn */
  isCurrentTurn?: boolean;
  /** Optional callback when a card is clicked (only relevant for human players) */
  onCardClick?: (cardIndex: number) => void;
}

const positionStyles: Record<PlayerSlotProps['position'], string> = {
  north: 'col-start-2 row-start-1 justify-center',
  south: 'col-start-2 row-start-3 justify-center',
  east: 'col-start-3 row-start-2 justify-end',
  west: 'col-start-1 row-start-2 justify-start',
};

const PlayerSlot: React.FC<PlayerSlotProps> = ({
  name,
  playerId,
  position,
  type,
  cards,
  cardsFaceUp,
  remainingCount,
  isCurrentTurn = false,
  onCardClick,
}) => {
  // For bots with empty cards, show placeholder face‑down cards based on remainingCount
  const displayCards = cards.length > 0
    ? cards
    : (type === 'bot' && remainingCount !== undefined
        ? Array.from({ length: remainingCount }, (_, i) => i)
        : []);

  // Only allow clicks on real human cards, not bot placeholders
  const handleCardClick = type === 'human' && cards.length > 0 && onCardClick
    ? (cardIndex: number) => onCardClick(cardIndex)
    : undefined;

  const ringClass = isCurrentTurn
    ? 'ring-4 ring-red-500 ring-offset-2'
    : '';

  return (
    <div
      className={`flex flex-col items-center p-4 ${positionStyles[position]} ${ringClass} rounded-lg`}
      data-testid={`player-slot-${playerId}`}
    >
      <div className="text-lg font-semibold mb-2">
        {name} {type === 'bot' && '🤖'}
      </div>
      <div className="flex flex-wrap gap-2 justify-center">
        {displayCards.length > 0 ? (
          displayCards.map((cardIndex) => (
            <Card
              key={cardIndex}
              index={cardIndex}
              faceUp={cardsFaceUp}
              onClick={handleCardClick ? () => handleCardClick(cardIndex) : undefined}
            />
          ))
        ) : (
          <div className="text-gray-500 italic">No cards</div>
        )}
      </div>
      <div className="mt-2 text-sm text-gray-600">
        {position.toUpperCase()} – {type}
      </div>
    </div>
  );
};

export default PlayerSlot;
