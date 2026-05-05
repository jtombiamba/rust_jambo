import React from 'react';

export interface CardProps {
  /** Card index (0–31) */
  index: number;
  /** Whether the card is face-up (default true) */
  faceUp?: boolean;
  /** Optional click handler */
  onClick?: () => void;
}

/**
 * A playing card component.
 * When face-up, shows the card's suit and rank.
 * When face-down, shows a generic card back.
 */
const Card: React.FC<CardProps> = ({ index, faceUp = true, onClick }) => {
  const suit = ['Hearts', 'Spades', 'Diamonds', 'Clubs'][Math.floor(index / 8)];
  const rank = (index % 8) + 3; // ranks 3–10

  const suitSymbol = {
    Hearts: '♥',
    Spades: '♠',
    Diamonds: '♦',
    Clubs: '♣',
  }[suit];

  const color = suit === 'Hearts' || suit === 'Diamonds' ? 'text-red-600' : 'text-black';

  return (
    <div
      className={`relative w-16 h-24 rounded-lg border border-gray-300 shadow-md flex items-center justify-center cursor-pointer transition-transform hover:scale-105 ${
        faceUp ? 'bg-white' : 'bg-blue-800'
      }`}
      onClick={onClick}
      role="button"
      tabIndex={0}
    >
      {faceUp ? (
        <div className="flex flex-col items-center">
          <div className={`text-2xl font-bold ${color}`}>{suitSymbol}</div>
          <div className="text-lg font-semibold mt-1">{rank}</div>
          <div className="text-xs mt-2 text-gray-600">{suit}</div>
        </div>
      ) : (
        <div className="w-full h-full rounded-lg bg-gradient-to-br from-blue-900 to-blue-700 flex items-center justify-center">
          <div className="text-white text-3xl">🂠</div>
        </div>
      )}
    </div>
  );
};

export default Card;
