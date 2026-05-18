import React from 'react';

export interface CardProps {
  /** Card index (0–31) */
  index: number;
  /** Whether the card is face-up (default true) */
  faceUp?: boolean;
  /** Optional click handler */
  onClick?: () => void;
  /** Whether the card is currently selected */
  selected?: boolean;
}

const Card: React.FC<CardProps> = ({ index, faceUp = true, onClick, selected = false }) => {
  const suit = ['Hearts', 'Spades', 'Diamonds', 'Clubs'][Math.floor(index / 8)];
  const rank = (index % 8) + 3;

  const suitSymbol = {
    Hearts: '♥',
    Spades: '♠',
    Diamonds: '♦',
    Clubs: '♣',
  }[suit];

  const color = suit === 'Hearts' || suit === 'Diamonds' ? 'text-red-600' : 'text-black';

  return (
    <div
      className={`relative w-10 h-14 sm:w-12 sm:h-[4.5rem] md:w-16 md:h-24 rounded-lg border border-gray-300 shadow-md flex items-center justify-center cursor-pointer transition-transform active:scale-95 ${
        faceUp ? 'bg-white' : 'bg-blue-800'
      } ${selected ? 'ring-3 ring-red-500 scale-110' : ''}`}
      onClick={onClick}
      role="button"
      tabIndex={0}
      data-testid={`card-${index}${selected ? '-selected' : ''}`}
    >
      {faceUp ? (
        <>
          <div className={`absolute top-0.5 left-0.5 sm:top-1 sm:left-1 flex flex-col items-center leading-none ${color}`}>
            <span className="text-[8px] sm:text-[10px] md:text-xl font-bold">{rank}</span>
            <span className="text-[8px] sm:text-[10px] md:text-xl -mt-0.5">{suitSymbol}</span>
          </div>
          <div className={`absolute bottom-0.5 right-0.5 sm:bottom-1 sm:right-1 flex flex-col items-center leading-none ${color} rotate-180`}>
            <span className="text-[8px] sm:text-[10px] md:text-xl font-bold">{rank}</span>
            <span className="text-[8px] sm:text-[10px] md:text-xl -mt-0.5">{suitSymbol}</span>
          </div>
          <div className="flex flex-col items-center">
            <div className={`text-base sm:text-xl md:text-[40px] font-bold ${color} leading-none`}>{suitSymbol}</div>
            <div className="text-sm sm:text-lg md:text-2xl font-semibold mt-0.5">{rank}</div>
          </div>
        </>
      ) : (
        <div className="w-full h-full rounded-lg bg-gradient-to-br from-blue-900 to-blue-700 flex items-center justify-center">
          <div className="text-white text-base sm:text-xl md:text-3xl">🂠</div>
        </div>
      )}
    </div>
  );
};

export default Card;
