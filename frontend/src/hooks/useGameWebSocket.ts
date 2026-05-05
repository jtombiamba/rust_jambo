import { useEffect, useRef} from 'react';
import { useGameStore, GameResult } from '../stores/useGameStore';
import { useWebSocket, GameEvent } from './useWebSocket';
import { updateStatsAfterGame } from '../utils/storage';

/**
 * A hook that connects WebSocket events to the game store.
 * It automatically subscribes to the game's WebSocket channel and updates the store accordingly.
 *
 * @param gameId The game ID to connect to.
 */
export function useGameWebSocket(gameId: string | null) {
  const {
    applyCardPlayed,
    setCurrentTurn,
    setGameStatus,
    setRoundWinner,
    clearRoundWinner,
    clearDeckSlots,
    setGameOver,
    players,
    bet,
  } = useGameStore();

  const roundWinnerTimerRef = useRef<ReturnType<typeof setTimeout>>();
  const deckClearTimerRef = useRef<ReturnType<typeof setTimeout>>();

  const { isConnected, lastError, send } = useWebSocket({
    gameId: gameId || '',
    onMessage: (event: GameEvent) => {
      switch (event.type) {
        case 'card_played':
          applyCardPlayed(event.player_id, event.card_index, event.next_turn);
          // Clear round winner when new card is played (new round starts)
          clearRoundWinner();
          break;
        case 'round_completed': {
          // Clear deck slots after 1 second delay for human eye to read result
          if (deckClearTimerRef.current) {
            clearTimeout(deckClearTimerRef.current);
          }
          deckClearTimerRef.current = setTimeout(() => {
            clearDeckSlots();
          }, 1000);

          // Set round winner for visualization
          const winType = (event.win_type as 'normal' | 'kora' | 'doubleKora') || 'normal';
          setRoundWinner({
            playerId: event.winner_id,
            position: event.winner_position,
            winType,
          });

          // Auto-clear winner after 3 seconds
          if (roundWinnerTimerRef.current) {
            clearTimeout(roundWinnerTimerRef.current);
          }
          roundWinnerTimerRef.current = setTimeout(() => {
            clearRoundWinner();
          }, 3000);
          break;
        }
        case 'game_finished': {
          setGameStatus(event.status);

          // Set game over state
          const winner = event.winner_id ? players.find(p => p.id === event.winner_id) : null;
          const gameResult: GameResult = {
            status: event.status as 'finished' | 'kora' | 'doubleKora',
            finalScore: event.final_score,
            roundsPlayed: event.rounds_played,
          };

          setGameOver({
            isGameOver: true,
            winner: winner || null,
            result: gameResult,
          });

          // Update localStorage with game result
          const humanPlayer = players.find(p => p.type === 'human');
          if (humanPlayer) {
            const won = humanPlayer.id === event.winner_id;
            updateStatsAfterGame(bet, won, event.status as 'finished' | 'kora' | 'doubleKora');
          }
          break;
        }
        case 'turn_changed': {
          // Find player position by ID
          const player = players.find((p) => p.id === event.current_turn);
          if (player) {
            setCurrentTurn(player.position);
          }
          break;
        }
        default: {
          const _exhaustive: never = event;
          console.warn('Unhandled game event type:', (_exhaustive as { type: string }).type);
        }
      }
    },
    onError: (error) => {
      console.error('WebSocket error:', error);
    },
    onClose: (event) => {
      console.log('WebSocket closed:', event);
    },
    autoReconnect: true,
  });

  // Send a ping every 30 seconds to keep connection alive
  useEffect(() => {
    if (!isConnected) return;
    const interval = setInterval(() => {
      send({ type: 'ping' });
    }, 30000);
    return () => clearInterval(interval);
  }, [isConnected, send]);

  // Cleanup timers on unmount
  useEffect(() => {
    return () => {
      if (roundWinnerTimerRef.current) {
        clearTimeout(roundWinnerTimerRef.current);
      }
      if (deckClearTimerRef.current) {
        clearTimeout(deckClearTimerRef.current);
      }
    };
  }, []);

  return { isConnected, lastError, send };
}

export default useGameWebSocket;
