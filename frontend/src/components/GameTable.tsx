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
  position: number; // 0: south, 1: east, 2: north, 3: west
  cards: number[];
}

export interface GameTableProps {
  players: GamePlayer[];
  currentTurn?: number; // position of player whose turn it is
  /** Array of four deck slots, each containing a card index or null */
  deckSlots?: (number | null)[];
  /** Number of remaining cards per player (playerId -> count) */
  remainingCards?: Record<string, number>;
  /** Round winner information for visualization */
  roundWinner?: RoundWinner | null;
  /** Game over information for modal display */
  gameOver?: GameOverData | null;
  /** Callback when a player card is clicked */
  onCardClick?: (playerId: string, cardIndex: number) => void;
  /** Callback for Play Again action in game over modal */
  onPlayAgain?: () => void;
  /** Callback for Return to Lobby action in game over modal */
  onReturnToLobby?: () => void;
  /** Callback to close game over modal */
  onCloseGameOver?: () => void;
}

const positionMap: Record<number, PlayerSlotProps['position']> = {
  0: 'south',
  1: 'east',
  2: 'north',
  3: 'west',
};

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
}) => {
  // Ensure we have exactly four players, sorted by position
  const sortedPlayers = [...players].sort((a, b) => a.position - b.position);
  while (sortedPlayers.length < 4) {
    // Placeholder for missing players (should not happen)
    sortedPlayers.push({
      id: `placeholder-${sortedPlayers.length}`,
      type: 'bot',
      name: 'Missing',
      position: sortedPlayers.length,
      cards: [],
    });
  }

  // Determine if a player is the round winner
  const isPlayerRoundWinner = (playerPosition: number) => {
    return roundWinner !== null && roundWinner.position === playerPosition;
  };

  return (
    <div className="container mx-auto p-8">
      <h2 className="text-2xl font-bold mb-6 text-center">Game Table</h2>
      <div className="relative grid grid-cols-3 grid-rows-3 gap-8 min-h-[600px]">
        {/* Player slots at each side */}
        {sortedPlayers.map((player) => {
          const position = positionMap[player.position] || 'south';
          const isCurrentTurn = currentTurn !== undefined && player.position === currentTurn;
          const isWinner = isPlayerRoundWinner(player.position);

          // Determine grid positioning based on position
          let gridClass = '';
          switch (position) {
            case 'south':
              gridClass = 'col-start-2 row-start-3'; // bottom center
              break;
            case 'north':
              gridClass = 'col-start-2 row-start-1'; // top center
              break;
            case 'east':
              gridClass = 'col-start-3 row-start-2'; // right middle
              break;
            case 'west':
              gridClass = 'col-start-1 row-start-2'; // left middle
              break;
            default:
              gridClass = 'col-start-2 row-start-2'; // center (fallback)
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

              {/* Winner ring for round winner */}
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

        {/* Central deck area */}
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

      {/* Game Over Modal */}
      {gameOver && gameOver.isGameOver && (
        <GameOverModal
          isOpen={true}
          onClose={onCloseGameOver || (() => {})}
          winner={gameOver.winner}
          gameResult={gameOver.result}
          onPlayAgain={onPlayAgain || (() => {})}
          onReturnToLobby={onReturnToLobby || (() => {})}
        />
      )}
    </div>
  );
};

export default GameTable;
