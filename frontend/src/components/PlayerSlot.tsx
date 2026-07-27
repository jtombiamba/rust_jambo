import React from 'react';
import { AnimatePresence } from 'framer-motion';
import AnimatedCard from './AnimatedCard';
import CardFan from './CardFan';

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
  /** Whether this bot is currently thinking (bot chain replay in progress) */
  isThinking?: boolean;
  /** Optional callback when a card is clicked (only relevant for human players) */
  onCardClick?: (cardIndex: number) => void;
  /** Whether to use compact mode (for mobile portrait) */
  compact?: boolean;
  /** Screen orientation */
  orientation?: 'portrait' | 'landscape';
  /** Currently selected card index (for human player's hand) */
  selectedCardIndex?: number | null;
  /** Whether cards should overlap (mobile) or be spread without overlap (desktop) */
  overlapCards?: boolean;
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
  isThinking = false,
  onCardClick,
  compact = false,
  selectedCardIndex = null,
  overlapCards = true,
}) => {
  const displayCards = cards.length > 0
    ? cards
    : (remainingCount !== undefined && remainingCount > 0
        ? Array.from({ length: remainingCount }, (_, i) => i)
        : []);

  const handleCardClick = cards.length > 0 && onCardClick
    ? (cardIndex: number) => onCardClick(cardIndex)
    : undefined;

  const ringClass = isCurrentTurn
    ? 'ring-4 ring-red-500 ring-offset-2'
    : '';

  const renderCards = () => {
    if (displayCards.length === 0) {
      return <div className="text-gray-500 italic text-sm">No cards</div>;
    }

    if (overlapCards) {
      return (
        <CardFan
          cards={displayCards}
          faceUp={cardsFaceUp}
          overlapPercent={compact ? 65 : 55}
          onCardClick={handleCardClick}
          selectedIndex={selectedCardIndex}
          compact={compact}
          playerType={type}
        />
      );
    }

    return (
      <div className="flex flex-wrap gap-1 sm:gap-2 justify-center">
        <AnimatePresence mode="popLayout">
          {displayCards.map((cardIndex) => (
            <AnimatedCard
              key={cardIndex}
              index={cardIndex}
              faceUp={cardsFaceUp}
              onClick={handleCardClick ? () => handleCardClick(cardIndex) : undefined}
              selected={selectedCardIndex === cardIndex || (cards[0] === cardIndex && selectedCardIndex === 0)}
              layoutId={`hand-card-${cardIndex}`}
            />
          ))}
        </AnimatePresence>
      </div>
    );
  };

  return (
    <div
      className={`flex flex-col items-center p-2 sm:p-4 ${positionStyles[position]} ${ringClass} rounded-lg`}
      data-testid={`player-slot-${playerId}`}
    >
      <div className={`${compact ? 'text-sm' : 'text-base sm:text-lg'} font-semibold mb-1 sm:mb-2 flex items-center gap-1`}>
        {name} {type === 'bot' && '🤖'}
        {isThinking && (
          <span className="inline-block w-2 h-2 bg-yellow-400 rounded-full animate-pulse" title="thinking..." />
        )}
      </div>
      <div className="flex justify-center w-full">
        {renderCards()}
      </div>
      <div className={`mt-1 sm:mt-2 ${compact ? 'text-[10px]' : 'text-xs sm:text-sm'} text-gray-600`}>
        {position.toUpperCase()} – {type}
      </div>
    </div>
  );
};

export default PlayerSlot;
