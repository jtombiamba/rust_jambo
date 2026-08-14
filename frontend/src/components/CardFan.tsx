import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import Card from './Card';
import AnimatedCard from './AnimatedCard';
import './CardFan.css';

export interface CardFanProps {
  cards: number[];
  faceUp: boolean;
  overlapPercent?: number;
  maxWidth?: string;
  onCardClick?: (cardIndex: number) => void;
  selectedIndex?: number | null;
  compact?: boolean;
  playerType?: 'human' | 'bot';
  /** Unique player id, used to scope framer-motion layoutIds so cards from
   *  different players (especially bots with placeholder indices) don't collide. */
  playerId?: string;
}

const CardFan: React.FC<CardFanProps> = ({
  cards,
  faceUp,
  overlapPercent = 60,
  maxWidth,
  onCardClick,
  selectedIndex = null,
  compact = false,
  playerType,
  playerId,
}) => {
  if (cards.length === 0) {
    return <div className="text-gray-500 italic text-sm">No cards</div>;
  }

  if (compact && cards.length > 3 && playerType === 'bot' && !faceUp) {
    return (
      <div className="card-fan-compact" data-testid="card-fan-compact">
        <div className="card-fan-badge">{cards.length} cards</div>
        <div className="card-fan-mini">
          {cards.map((cardIndex, i) => (
            <motion.div
              key={`mini-${cardIndex}`}
              className="card-fan-mini-card"
              style={{ left: `${i * 12}px`, zIndex: i }}
              layout
              initial={{ opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.8 }}
              transition={{ duration: 0.2 }}
            >
              <Card index={cardIndex} faceUp={false} />
            </motion.div>
          ))}
        </div>
      </div>
    );
  }

  const cardWidth = 48;
  const step = cardWidth * (1 - overlapPercent / 100);
  const totalWidth = cards.length > 1
    ? cardWidth + step * (cards.length - 1)
    : cardWidth;

  return (
    <div
      className="card-fan-container"
      style={{ width: maxWidth || `${totalWidth}px`, minHeight: '80px' }}
      data-testid="card-fan"
    >
      <AnimatePresence>
        {cards.map((cardIndex, i) => (
          <motion.div
            key={`hand-card-${cardIndex}`}
            className={`card-fan-card ${selectedIndex === i ? 'selected' : ''}`}
            style={{
              left: `${step * i}px`,
              zIndex: i,
            }}
            layout
            initial={{ opacity: 0, scale: 0.8, y: 20 }}
            animate={{
              opacity: 1,
              scale: 1,
              y: 0,
            }}
            exit={{
              opacity: 0,
              scale: 0.8,
              y: -20,
              transition: { duration: 0.3, ease: [0.34, 1.56, 0.64, 1] },
            }}
            transition={{
              type: 'spring',
              stiffness: 300,
              damping: 25,
            }}
            data-testid={`card-fan-card-${i}`}
          >
            <AnimatedCard
              index={cardIndex}
              faceUp={faceUp}
              onClick={onCardClick ? () => onCardClick(cardIndex) : undefined}
              selected={selectedIndex === i}
              layoutId={`hand-card-${playerId ?? playerType ?? 'p'}-${cardIndex}`}
              // The outer motion.div above already animates the entrance
              // (opacity/scale/y). Disable the inner fade-in so cards appear
              // immediately instead of compounding a double fade-in delay.
              animateIn={false}
            />
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
};

export default CardFan;
