import { useCallback, useEffect, useRef, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import GameTable from './GameTable';
import { getWsUrl } from '../utils/runtimeConfig';
import { RoundWinner, GameOverData, GameResult, Player } from '../stores/useGameStore';

type StreamStatus = 'loading' | 'waiting' | 'live';

/**
 * Read-only spectator stream for a whole run. A single connection follows the
 * run across games: the backend re-points it to each new game_id when a game
 * starts, so the streamer shares one link for the entire run.
 */
export default function RunStreamView() {
  const { runId } = useParams<{ runId: string }>();
  const [searchParams] = useSearchParams();
  const token = searchParams.get('token') ?? undefined;
  const { t } = useTranslation();

  const [status, setStatus] = useState<StreamStatus>('loading');
  const [players, setPlayers] = useState<Player[]>([]);
  const [deckSlots, setDeckSlots] = useState<(number | null)[]>([]);
  const [remainingCards, setRemainingCards] = useState<Record<string, number>>({});
  const [currentTurn, setCurrentTurn] = useState<number | undefined>(undefined);
  const [roundWinner, setRoundWinner] = useState<RoundWinner | null>(null);
  const [gameOver, setGameOver] = useState<GameOverData | null>(null);

  const socketRef = useRef<WebSocket | null>(null);
  const currentGameIdRef = useRef<string | null>(null);
  const playersRef = useRef<Player[]>([]);
  const roundWinnerClearTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    playersRef.current = players;
  }, [players]);

  const resetForGame = useCallback((gameId: string) => {
    currentGameIdRef.current = gameId;
    setRoundWinner(null);
    setGameOver(null);
  }, []);

  useEffect(() => {
    if (!runId || !token) return;

    const url = getWsUrl(`/ws/run/${runId}?token=${encodeURIComponent(token)}`);
    let reconnectTimeout: ReturnType<typeof setTimeout>;
    let shouldReconnect = true;

    const connect = () => {
      const ws = new WebSocket(url);
      socketRef.current = ws;

      ws.onopen = () => {
        console.log(`Run stream WS connected for run ${runId}`);
      };

      ws.onmessage = (evt) => {
        let data: Record<string, unknown>;
        try {
          data = JSON.parse(evt.data);
        } catch {
          return;
        }
        if (!data.type) return;

        switch (data.type) {
          case 'run_joined': {
            if (data.game_id) {
              resetForGame(data.game_id as string);
              setStatus('live');
            } else {
              setStatus('waiting');
            }
            break;
          }
          case 'game_state_snapshot': {
            const gameId = data.game_id as string;
            if (currentGameIdRef.current !== gameId) {
              resetForGame(gameId);
            }
            const rawPlayers = (data.players ?? []) as Array<{
              id: string;
              name: string;
              position: number;
              display_position: number;
              player_type: string;
              cards_count: number;
            }>;
            const snapshotPlayers: Player[] = rawPlayers.map((p) => ({
              id: p.id,
              type: p.player_type as 'human' | 'bot',
              name: p.name,
              position: p.position,
              display_position: p.display_position,
              cards: [],
              cards_count: p.cards_count,
              is_current_user: false,
            }));
            const remaining: Record<string, number> = {};
            snapshotPlayers.forEach((p) => {
              remaining[p.id] = p.cards_count ?? 0;
            });

            setPlayers(snapshotPlayers);
            setRemainingCards(remaining);

            if (data.status === 'pending' || data.status === 'ready') {
              setStatus('waiting');
            } else {
              setStatus('live');
            }

            setDeckSlots((data.played_cards as (number | null)[]) ?? []);
            setRoundWinner(null);

            let turn: number | undefined;
            if (data.rank !== null && data.rank !== undefined) {
              const current = snapshotPlayers.find((p) => p.position === data.rank);
              turn = current?.display_position;
            }
            setCurrentTurn(turn);
            break;
          }
          case 'card_played': {
            const playerId = data.player_id as string;
            const cardIndex = data.card_index as number;
            setRemainingCards((prev) => {
              const next = { ...prev };
              if (next[playerId] !== undefined) {
                next[playerId] = Math.max(0, next[playerId] - 1);
              }
              return next;
            });
            setDeckSlots((prev) => {
              const next = [...prev];
              const empty = next.findIndex((slot) => slot === null);
              if (empty !== -1) next[empty] = cardIndex;
              return next;
            });
            const nextTurn = data.next_turn as string | undefined;
            if (nextTurn) {
              const nextPlayer = playersRef.current.find((p) => p.id === nextTurn);
              if (nextPlayer) setCurrentTurn(nextPlayer.display_position);
            }
            break;
          }
          case 'round_completed': {
            const winnerId = data.winner_id as string;
            const winnerPlayer = playersRef.current.find((p) => p.id === winnerId);
            const winner: RoundWinner = {
              playerId: winnerId,
              position: winnerPlayer?.display_position ?? (data.winner_position as number),
              winType: (data.win_type as 'normal' | 'kora' | 'doubleKora') || 'normal',
            };
            setRoundWinner(winner);
            if (roundWinnerClearTimerRef.current) clearTimeout(roundWinnerClearTimerRef.current);
            roundWinnerClearTimerRef.current = setTimeout(() => {
              setRoundWinner(null);
              setDeckSlots((prev) => prev.map(() => null));
            }, 3000);
            break;
          }
          case 'game_finished': {
            const winnerPlayer = playersRef.current.find((p) => p.id === data.winner_id) ?? null;
            const result: GameResult = {
              status: data.status as 'finished' | 'kora' | 'doubleKora',
              finalScore: data.final_score as number | undefined,
              roundsPlayed: data.rounds_played as number,
            };
            setGameOver({ isGameOver: true, winner: winnerPlayer, result });
            break;
          }
          default:
            break;
        }
      };

      ws.onclose = () => {
        if (shouldReconnect) {
          reconnectTimeout = setTimeout(connect, 5000);
        }
      };

      ws.onerror = () => {
        ws.close();
      };
    };

    connect();

    const pingInterval = setInterval(() => {
      if (socketRef.current?.readyState === WebSocket.OPEN) {
        socketRef.current.send(JSON.stringify({ type: 'ping' }));
      }
    }, 30000);

    return () => {
      shouldReconnect = false;
      clearTimeout(reconnectTimeout);
      clearInterval(pingInterval);
      if (roundWinnerClearTimerRef.current) clearTimeout(roundWinnerClearTimerRef.current);
      socketRef.current?.close();
    };
  }, [runId, token, resetForGame]);

  if (status !== 'live') {
    return (
      <div className="min-h-screen flex items-center justify-center p-4">
        <div className="text-center">
          {status === 'loading' && (
            <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"></div>
          )}
          <h1 className="text-xl font-bold mb-2">{t('stream.spectatorWaitingTitle')}</h1>
          <p className="text-gray-600">{t('stream.spectatorWaitingSubtitle')}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen flex flex-col">
      <div className="flex-1">
        <GameTable
          players={players}
          currentTurn={currentTurn}
          deckSlots={deckSlots}
          remainingCards={remainingCards}
          roundWinner={roundWinner}
          gameOver={gameOver}
          spectatorMode
        />
      </div>
    </div>
  );
}
