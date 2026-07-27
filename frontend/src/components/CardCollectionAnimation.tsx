import React, { useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import Card from './Card';

export interface CardCollectionAnimationProps {
  cards: (number | null)[];
  winnerPosition: 'north' | 'south' | 'east' | 'west' | null;
  onAnimationComplete?: () => void;
}

const CardCollectionAnimation: React.FC<CardCollectionAnimationProps> = ({
  cards,
  winnerPosition,
  onAnimationComplete,
}) => {
  const [visibleCards, setVisibleCards] = useState<number[]>([]);

  useEffect(() => {
    const validCards = cards.filter((c): c is number => c !== null);
    setVisibleCards(validCards);

    const timer = setTimeout(() => {
      setVisibleCards([]);
      onAnimationComplete?.();
    }, 800);

    return () => clearTimeout(timer);
  }, [cards, onAnimationComplete]);

  if (!winnerPosition || visibleCards.length === 0) {
    return null;
  }

  const getWinnerTarget = () => {
    switch (winnerPosition) {
      case 'north':
        return { x: 0, y: -200 };
      case 'south':
        return { x: 0, y: 200 };
      case 'east':
        return { x: 200, y: 0 };
      case 'west':
        return { x: -200, y: 0 };
      default:
        return { x: 0, y: 0 };
    }
  };

  const target = getWinnerTarget();

  return (
    <div className="absolute inset-0 pointer-events-none z-50 flex items-center justify-center">
      <AnimatePresence>
        {visibleCards.map((cardIndex, idx) => (
          <motion.div
            key={`collection-${cardIndex}-${idx}`}
            className="absolute"
            initial={{
              x: 0,
              y: 0,
              scale: 1,
              opacity: 1,
            }}
            animate={{
              x: target.x,
              y: target.y,
              scale: [1, 1.2, 0.8],
              opacity: [1, 1, 0],
              rotate: [0, 10, -10, 0],
            }}
            exit={{
              opacity: 0,
              scale: 0,
            }}
            transition={{
              duration: 0.6,
              delay: idx * 0.05,
              ease: [0.34, 1.56, 0.64, 1],
            }}
            style={{
              filter: 'drop-shadow(0 0 10px rgba(255, 215, 0, 0.8))',
            }}
          >
            <Card index={cardIndex} faceUp={true} />
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
};

export default CardCollectionAnimation;
