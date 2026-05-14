import { useEffect, useRef} from 'react';
import { useGameStore, GameResult } from '../stores/useGameStore';
import { useWebSocket, GameEvent } from './useWebSocket';
import { updateAnonymousStatsAfterGame } from '../utils/storage';
import { useAuthStore } from '../stores/useAuthStore';
import { useToast } from '../components/useToast';

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

  const { showToast } = useToast();
  const roundWinnerTimerRef = useRef<ReturnType<typeof setTimeout>>();
  const deckClearTimerRef = useRef<ReturnType<typeof setTimeout>>();

  const { isConnected, lastError, send } = useWebSocket({
    gameId: gameId || '',
    onMessage: (event: GameEvent) => {
      switch (event.type) {
        case 'card_played': {
          applyCardPlayed(event.player_id, event.card_index, event.next_turn);
          clearRoundWinner();
          break;
        }
        case 'round_completed': {
          if (deckClearTimerRef.current) {
            clearTimeout(deckClearTimerRef.current);
          }
          deckClearTimerRef.current = setTimeout(() => {
          clearDeckSlots();
          }, 800);

          const winType = (event.win_type as 'normal' | 'kora' | 'doubleKora') || 'normal';
          const winnerPlayer = players.find((p) => p.id === event.winner_id);
          const winnerDisplayPos = winnerPlayer?.display_position ?? event.winner_position;
          setRoundWinner({
            playerId: event.winner_id,
            position: winnerDisplayPos,
            winType,
          });

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

          const { isAuthenticated } = useAuthStore.getState();
          if (!isAuthenticated) {
            const humanPlayer = players.find(p => p.type === 'human');
            if (humanPlayer) {
              const won = humanPlayer.id === event.winner_id;
              updateAnonymousStatsAfterGame(bet, won, event.status as 'finished' | 'kora' | 'doubleKora');
            }
          }
          break;
        }
        case 'turn_changed': {
          const player = players.find((p) => p.id === event.current_turn);
          if (player) {
            setCurrentTurn(player.display_position ?? player.position);
          }
          break;
        }
        case 'player_disconnected': {
          const player = players.find(p => p.id === event.player_id);
          const name = player?.name || `Player ${event.player_position}`;
          showToast(`${name} disconnected`, 'warning');
          break;
        }
        case 'player_reconnected': {
          const player = players.find(p => p.id === event.player_id);
          const name = player?.name || `Player ${event.player_position}`;
          showToast(`${name} reconnected`, 'success');
          break;
        }
        case 'player_joined': {
          showToast(`${event.pseudo} joined the game`, 'info');
          break;
        }
        case 'game_cancelled': {
          showToast(`Game cancelled: ${event.reason}`, 'warning');
          break;
        }
        case 'game_ready': {
          showToast('All players ready!', 'success');
          break;
        }
        default:
          console.warn('Unhandled game event type:', (event as { type: string }).type);
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

  useEffect(() => {
    if (!isConnected) return;
    const interval = setInterval(() => {
      send({ type: 'ping' });
    }, 30000);
    return () => clearInterval(interval);
  }, [isConnected, send]);

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
