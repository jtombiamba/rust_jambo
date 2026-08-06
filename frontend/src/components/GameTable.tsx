import React, { useState, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import PlayerSlot, { PlayerSlotProps } from './PlayerSlot';
import Card from './Card';
import WinnerRing from './WinnerRing';
import GameOverModal from './GameOverModal';
import GameRules from './GameRules';
import { RoundWinner, GameOverData, useStepByStepPhase, useGameStore } from '../stores/useGameStore';

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
  onAdvanceBot?: () => void;
  onEvaluateRound?: () => void;
}

type LayoutMode = 'mobile-portrait' | 'mobile-landscape' | 'desktop';

function getPositionMap(numPlayers: number): Record<number, PlayerSlotProps['position']> {
  if (numPlayers <= 2) {
    return { 0: 'south', 1: 'north' };
  }
  if (numPlayers === 3) {
    return { 0: 'south', 1: 'east', 2: 'north' };
  }
  return { 0: 'south', 1: 'east', 2: 'north', 3: 'west' };
}

function getPlayerPositions(numPlayers: number): PlayerSlotProps['position'][] {
  if (numPlayers <= 2) return ['south', 'north'];
  if (numPlayers === 3) return ['south', 'east', 'north'];
  return ['south', 'east', 'north', 'west'];
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
  onAdvanceBot,
  onEvaluateRound,
}) => {
  const { t } = useTranslation();
  const phase = useStepByStepPhase();
  const isReplayingBots = useGameStore((s) => s.isReplayingBots);
  const isBotChainActive = useGameStore((s) => s.isBotChainActive);

  const getLayoutMode = (): LayoutMode => {
    if (typeof window === 'undefined') return 'desktop';
    const w = window.innerWidth;
    if (w >= 768) return 'desktop';
    return window.innerHeight > window.innerWidth ? 'mobile-portrait' : 'mobile-landscape';
  };

  const [layoutMode, setLayoutMode] = useState<LayoutMode>(getLayoutMode);
  const [rulesOpen, setRulesOpen] = useState(false);

  useEffect(() => {
    const handleResize = () => {
      setLayoutMode(getLayoutMode());
    };
    window.addEventListener('resize', handleResize);
    const mq = window.matchMedia('(orientation: portrait)');
    const handleOrientation = () => {
      setLayoutMode(getLayoutMode());
    };
    mq.addEventListener('change', handleOrientation);
    return () => {
      window.removeEventListener('resize', handleResize);
      mq.removeEventListener('change', handleOrientation);
    };
  }, []);

  const numPlayers = players.length;
  const positionMap = getPositionMap(numPlayers);
  const allPositions = getPlayerPositions(numPlayers);

  const getDisplayPos = (p: GamePlayer) => p.display_position ?? p.position;

  const sortedPlayers = [...players].sort((a, b) => getDisplayPos(a) - getDisplayPos(b));

  const findPlayerByPosition = (pos: PlayerSlotProps['position']) => {
    return sortedPlayers.find((p) => {
      const dp = getDisplayPos(p);
      return positionMap[dp] === pos;
    }) || null;
  };

  const isPlayerRoundWinner = (playerDisplayPosition: number) => {
    return roundWinner !== null && roundWinner.position === playerDisplayPosition;
  };

  const renderPlayerSlot = (player: GamePlayer, compact = false) => {
    const displayPos = getDisplayPos(player);
    const position = positionMap[displayPos] || 'south';
    const isCurrentTurn = currentTurn !== undefined && displayPos === currentTurn;
    const isBotThinking = player.type === 'bot'
      && (isReplayingBots || isBotChainActive)
      && isCurrentTurn;
    const isWinner = isPlayerRoundWinner(displayPos);

    return (
      <div key={player.id} className="relative">
        <PlayerSlot
          playerId={player.id}
          name={player.name}
          position={position}
          type={player.type}
          cards={player.cards}
          cardsFaceUp={player.cards.length > 0}
          remainingCount={remainingCards[player.id]}
          isCurrentTurn={isCurrentTurn}
          isThinking={isBotThinking}
          onCardClick={(cardIndex) => isReplayingBots ? undefined : onCardClick?.(player.id, cardIndex)}
          compact={compact}
          orientation={layoutMode === 'mobile-portrait' ? 'portrait' : 'landscape'}
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
  };

  const renderDeckOverlapping = (size: 'small' | 'medium') => {
    const step = size === 'small' ? 12 : 16;
    const placeholderClass = size === 'small'
      ? 'w-8 h-10 border border-dashed border-gray-400 rounded bg-gray-100'
      : 'w-10 h-14 sm:w-12 sm:h-[4.5rem] border-2 border-dashed border-gray-400 rounded-lg flex items-center justify-center bg-gray-100';
    const labelClass = size === 'small' ? 'hidden' : 'text-gray-400 text-[10px]';
    const testIdPrefix = size === 'small' ? 'deck-slot-landscape' : 'deck-slot';

    return (
      <div className="relative flex items-center justify-center min-w-[80px] min-h-[50px]">
        {deckSlots.map((card, idx) => (
          <div
            key={idx}
            className="absolute"
            style={{ left: `${idx * step}px`, zIndex: idx }}
            data-testid={`${testIdPrefix}-${idx}`}
          >
            {card !== null ? (
              <Card index={card} faceUp={true} />
            ) : (
              <div className={placeholderClass}>
                <div className={labelClass}>S{idx + 1}</div>
              </div>
            )}
          </div>
        ))}
      </div>
    );
  };

  return (
    <div className="container mx-auto p-2 sm:p-4 md:p-8">
      <div className="flex items-center justify-between mb-4 sm:mb-6">
        <h2 className="text-xl sm:text-2xl font-bold">{t('game.gameTable')}</h2>
        <button
          onClick={() => setRulesOpen(true)}
          className="px-3 py-1.5 text-sm font-medium bg-blue-100 text-blue-700 rounded-lg hover:bg-blue-200 transition-colors"
        >
          {t('dashboard.rules')}
        </button>
      </div>

      <div className="relative min-h-[400px] sm:min-h-[500px] md:min-h-[600px]">

        {/* Mobile portrait layout */}
        {layoutMode === 'mobile-portrait' && (
          <div className="flex flex-col gap-4">
            {(() => {
              const northPlayer = findPlayerByPosition('north');
              if (northPlayer) {
                return renderPlayerSlot(northPlayer, true);
              }
              return null;
            })()}

            <div className="flex items-center justify-center gap-2 py-2 border-y-2 border-dashed border-gray-300">
              {(() => {
                const westPlayer = findPlayerByPosition('west');
                if (westPlayer) {
                  return (
                    <div className="flex flex-col items-center">
                      <div className="text-xs font-semibold">{westPlayer.name} 🤖</div>
                      <div className="text-[10px] text-gray-600">
                        {remainingCards[westPlayer.id] ?? westPlayer.cards.length} {t('common.cards')}
                      </div>
                    </div>
                  );
                }
                return null;
              })()}

              <div className="flex flex-col items-center mx-2">
                <div className="text-xs sm:text-sm font-semibold mb-1">{t('common.deck')}</div>
                {renderDeckOverlapping('medium')}
              </div>

              {(() => {
                const eastPlayer = findPlayerByPosition('east');
                if (eastPlayer) {
                  return (
                    <div className="flex flex-col items-center">
                      <div className="text-xs font-semibold">{eastPlayer.name} 🤖</div>
                      <div className="text-[10px] text-gray-600">
                        {remainingCards[eastPlayer.id] ?? eastPlayer.cards.length} {t('common.cards')}
                      </div>
                    </div>
                  );
                }
                return null;
              })()}
            </div>

            {(() => {
              const southPlayer = findPlayerByPosition('south');
              if (southPlayer) {
                return renderPlayerSlot(southPlayer, false);
              }
              return null;
            })()}

            {currentTurn !== undefined && (
              <div className="text-center text-base sm:text-lg font-semibold text-red-600">
                {t('common.turn', { player: currentTurn })}
              </div>
            )}
          </div>
        )}

        {/* Mobile landscape layout */}
        {layoutMode === 'mobile-landscape' && (
          <div className="flex flex-col gap-3">
            <div className="flex items-start justify-around gap-1 py-2 border-b-2 border-dashed border-gray-300">
              {allPositions.map((pos) => {
                const player = findPlayerByPosition(pos);
                if (!player) return null;

                return (
                  <div key={player.id} className="flex flex-col items-center relative">
                    {pos === 'south' && (
                      <div className="flex flex-col items-center mb-1">
                        <div className="text-[10px] font-semibold">{t('common.deck')}</div>
                        {renderDeckOverlapping('small')}
                      </div>
                    )}

                    {renderPlayerSlot(player, true)}
                    {isPlayerRoundWinner(getDisplayPos(player)) && roundWinner && (
                      <WinnerRing
                        position={pos}
                        isVisible={true}
                        winType={roundWinner.winType}
                        playerName={player.name}
                      />
                    )}
                  </div>
                );
              })}

              {!findPlayerByPosition('south') && (
                <div className="flex flex-col items-center">
                  <div className="text-[10px] font-semibold">{t('common.deck')}</div>
                  {renderDeckOverlapping('small')}
                </div>
              )}
            </div>

            {(() => {
              const southPlayer = findPlayerByPosition('south');
              if (southPlayer) {
                return renderPlayerSlot(southPlayer, false);
              }
              return null;
            })()}

            {currentTurn !== undefined && (
              <div className="text-center text-sm sm:text-base font-semibold text-red-600">
                {t('common.turn', { player: currentTurn })}
              </div>
            )}
          </div>
        )}

        {/* Desktop layout */}
        {layoutMode === 'desktop' && (
          <div className="grid grid-cols-3 grid-rows-3 gap-8">
            {sortedPlayers.map((player) => {
              const displayPos = getDisplayPos(player);
              const position = positionMap[displayPos] || 'south';
              const isCurrentTurn = currentTurn !== undefined && displayPos === currentTurn;
              const isBotThinking = player.type === 'bot'
                && (isReplayingBots || isBotChainActive)
                && isCurrentTurn;
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
                    cardsFaceUp={player.cards.length > 0}
                    remainingCount={remainingCards[player.id]}
                    isCurrentTurn={isCurrentTurn}
                    isThinking={isBotThinking}
                    onCardClick={(cardIndex) => isReplayingBots ? undefined : onCardClick?.(player.id, cardIndex)}
                    overlapCards={false}
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
              <div className="text-lg font-semibold mb-4">{t('common.deck')}</div>
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
                      <div className="text-gray-400">{t('common.slot')} {idx + 1}</div>
                    )}
                  </div>
                ))}
              </div>
              {currentTurn !== undefined && (
                <div className="mt-4 text-lg font-semibold text-red-600">
                  {t('common.turn', { player: currentTurn })}
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {phase !== 'idle' && (
        <div className="container mx-auto px-2 sm:px-4 md:px-8 mt-4">
          <div className="p-3 bg-blue-50 border border-blue-200 rounded-lg">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium text-blue-600 bg-blue-100 px-2 py-0.5 rounded">
                  {t('game.stepByStep')}
                </span>
                {phase === 'bot_turn' && (
                  <span className="text-sm text-blue-700">{t('game.waitingForBot')}</span>
                )}
                {phase === 'evaluate_round' && (
                  <span className="text-sm text-blue-700">{t('game.allCardsPlayed')}</span>
                )}
              </div>
              {phase === 'bot_turn' && onAdvanceBot && (
                <button
                  className="px-4 py-2 bg-green-600 text-white text-sm font-semibold rounded-lg hover:bg-green-700"
                  onClick={onAdvanceBot}
                >
                  {t('game.playNextBot')}
                </button>
              )}
              {phase === 'evaluate_round' && onEvaluateRound && (
                <button
                  className="px-4 py-2 bg-purple-600 text-white text-sm font-semibold rounded-lg hover:bg-purple-700"
                  onClick={onEvaluateRound}
                >
                  {t('game.evaluateRound')}
                </button>
              )}
            </div>
          </div>
        </div>
      )}

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

      <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />
    </div>
  );
};

export default GameTable;
