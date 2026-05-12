import React from 'react';
import PlayerSlot, { PlayerSlotProps } from './PlayerSlot';
import Card from './Card';
import WinnerRing from './WinnerRing';
import GameOverModal from './GameOverModal';
import { RoundWinner, GameOverData } from '../stores/useGameStore';

export interface GamePlayer {
  id: string;
  type: 'human' | 'bot';
  name: string;
  position: number;
  display_position: number;
  cards: number[];
}

export interface GameTableProps {
  players: GamePlayer[];
  currentTurn?: number;
  deckSlots?: (number | null)[];
  remainingCards?: Record<string, number>;
  roundWinner?: RoundWinner | null;
  gameOver?: GameOverData | null;
  onCardClick?: (playerId: string, cardIndex: number) => void;
  onPlayAgain?: () => void;
  onReturnToLobby?: () => void;
  onCloseGameOver?: () => void;
  showPlayAgain?: boolean;
}

function getPositionMap(numPlayers: number): Record<number, PlayerSlotProps['position']> {
  if (numPlayers <= 2) {
    return { 0: 'south', 1: 'north' };
  }
  if (numPlayers === 3) {
    return { 0: 'south', 1: 'east', 2: 'north' };
  }
  return { 0: 'south', 1: 'east', 2: 'north', 3: 'west' };
}

const GameTable: React.FC<GameTableProps> = ({
  players,
  currentTurn,
  deckSlots = [null, null, null, null],
  remainingCards = {},
  roundWinner = null,
  gameOver = null,
  onCardClick,
  onPlayAgain,
  onReturnToLobby,
  onCloseGameOver,
  showPlayAgain = true,
}) => {
  const numPlayers = players.length;
  const positionMap = getPositionMap(numPlayers);

  const getDisplayPos = (p: GamePlayer) => p.display_position ?? p.position;

  const sortedPlayers = [...players].sort((a, b) => getDisplayPos(a) - getDisplayPos(b));
  while (sortedPlayers.length < numPlayers) {
    sortedPlayers.push({
      id: `placeholder-${sortedPlayers.length}`,
      type: 'bot',
      name: 'Missing',
      position: sortedPlayers.length,
      display_position: sortedPlayers.length,
      cards: [],
    });
  }

  const isPlayerRoundWinner = (playerDisplayPosition: number) => {
    return roundWinner !== null && roundWinner.position === playerDisplayPosition;
  };

  return (
    <div className="container mx-auto p-2 sm:p-4 md:p-8">
      <h2 className="text-xl sm:text-2xl font-bold mb-4 sm:mb-6 text-center">Game Table</h2>

      <div className="relative min-h-[400px] sm:min-h-[500px] md:min-h-[600px]">
        {/* Mobile layout: stacked players + center deck */}
        <div className="md:hidden flex flex-col gap-4">
          {sortedPlayers.map((player) => {
            const displayPos = getDisplayPos(player);
            const position = positionMap[displayPos] || 'south';
            const isCurrentTurn = currentTurn !== undefined && displayPos === currentTurn;
            const isWinner = isPlayerRoundWinner(displayPos);

            return (
              <div key={player.id} className="relative">
                <PlayerSlot
                  playerId={player.id}
                  name={player.name}
                  position={position}
                  type={player.type}
                  cards={player.cards}
                  cardsFaceUp={player.type === 'human'}
                  remainingCount={remainingCards[player.id]}
                  isCurrentTurn={isCurrentTurn}
                  onCardClick={(cardIndex) => onCardClick?.(player.id, cardIndex)}
                />
                {isWinner && roundWinner && (
                  <WinnerRing
                    position={position}
                    isVisible={true}
                    winType={roundWinner.winType}
                    playerName={player.name}
                  />
                )}
              </div>
            )
          })}

          {/* Center deck */}
          <div className="flex flex-col items-center justify-center py-4 border-t-2 border-dashed border-gray-300">
            <div className="text-base sm:text-lg font-semibold mb-3">Deck</div>
            <div className="flex gap-2 sm:gap-4 flex-wrap justify-center">
              {deckSlots.map((card, idx) => (
                <div
                  key={idx}
                  className="w-12 h-[4.5rem] sm:w-14 sm:h-20 border-2 border-dashed border-gray-400 rounded-lg flex items-center justify-center bg-gray-100"
                  data-testid={`deck-slot-${idx}`}
                >
                  {card !== null ? (
                    <Card index={card} faceUp={true} />
                  ) : (
                    <div className="text-gray-400 text-[10px] sm:text-xs">Slot {idx + 1}</div>
                  )}
                </div>
              ))}
            </div>
            {currentTurn !== undefined && (
              <div className="mt-3 text-base sm:text-lg font-semibold text-red-600">
                Turn: Player {currentTurn}
              </div>
            )}
          </div>
        </div>

        {/* Desktop layout: grid with 4 positions */}
        <div className="hidden md:grid grid-cols-3 grid-rows-3 gap-8">
          {sortedPlayers.map((player) => {
            const displayPos = getDisplayPos(player);
            const position = positionMap[displayPos] || 'south';
            const isCurrentTurn = currentTurn !== undefined && displayPos === currentTurn;
            const isWinner = isPlayerRoundWinner(displayPos);

            let gridClass = '';
            switch (position) {
              case 'south':
                gridClass = 'col-start-2 row-start-3';
                break;
              case 'north':
                gridClass = 'col-start-2 row-start-1';
                break;
              case 'east':
                gridClass = 'col-start-3 row-start-2';
                break;
              case 'west':
                gridClass = 'col-start-1 row-start-2';
                break;
              default:
                gridClass = 'col-start-2 row-start-2';
            }

            return (
              <div key={player.id} className={`relative ${gridClass}`}>
                <PlayerSlot
                  playerId={player.id}
                  name={player.name}
                  position={position}
                  type={player.type}
                  cards={player.cards}
                  cardsFaceUp={player.type === 'human'}
                  remainingCount={remainingCards[player.id]}
                  isCurrentTurn={isCurrentTurn}
                  onCardClick={(cardIndex) => onCardClick?.(player.id, cardIndex)}
                />
                {isWinner && roundWinner && (
                  <WinnerRing
                    position={position}
                    isVisible={true}
                    winType={roundWinner.winType}
                    playerName={player.name}
                  />
                )}
              </div>
            );
          })}

          <div className="col-start-2 row-start-2 flex flex-col items-center justify-center">
            <div className="text-lg font-semibold mb-4">Deck</div>
            <div className="flex gap-4">
              {deckSlots.map((card, idx) => (
                <div
                  key={idx}
                  className="w-16 h-24 border-2 border-dashed border-gray-400 rounded-lg flex items-center justify-center bg-gray-100"
                  data-testid={`deck-slot-${idx}`}
                >
                  {card !== null ? (
                    <Card index={card} faceUp={true} />
                  ) : (
                    <div className="text-gray-400">Slot {idx + 1}</div>
                  )}
                </div>
              ))}
            </div>
            {currentTurn !== undefined && (
              <div className="mt-4 text-lg font-semibold text-red-600">
                Turn: Player {currentTurn}
              </div>
            )}
          </div>
        </div>
      </div>

      {gameOver && gameOver.isGameOver && (
        <GameOverModal
          isOpen={true}
          onClose={onCloseGameOver || (() => {})}
          winner={gameOver.winner}
          gameResult={gameOver.result}
          onPlayAgain={onPlayAgain || (() => {})}
          onReturnToLobby={onReturnToLobby || (() => {})}
          showPlayAgain={showPlayAgain}
        />
      )}
    </div>
  );
};

export default GameTable;
