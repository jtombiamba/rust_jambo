import React from 'react';
import Card from './Card';
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
}) => {
  if (cards.length === 0) {
    return <div className="text-gray-500 italic text-sm">No cards</div>;
  }

  if (compact && cards.length > 3 && playerType === 'bot' && !faceUp) {
    return (
      <div className="card-fan-compact" data-testid="card-fan-compact">
        <div className="card-fan-badge">{cards.length} cards</div>
        <div className="card-fan-mini">
          {cards.slice(-3).map((cardIndex, i) => (
            <div
              key={i}
              className="card-fan-mini-card"
              style={{ left: `${i * 12}px`, zIndex: i }}
            >
              <Card index={cardIndex} faceUp={false} />
            </div>
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
      {cards.map((cardIndex, i) => (
        <div
          key={`${cardIndex}-${i}`}
          className={`card-fan-card ${selectedIndex === i ? 'selected' : ''}`}
          style={{
            left: `${step * i}px`,
            zIndex: i,
            transition: 'transform 0.2s ease, translate 0.2s ease',
          }}
          data-testid={`card-fan-card-${i}`}
        >
          <Card
            index={cardIndex}
            faceUp={faceUp}
            onClick={onCardClick ? () => onCardClick(cardIndex) : undefined}
            selected={selectedIndex === i}
          />
        </div>
      ))}
    </div>
  );
};

export default CardFan;
