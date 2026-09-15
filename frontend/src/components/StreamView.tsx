import { useCallback, useEffect, useRef, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import GameTable from './GameTable';
import { useWebSocket, GameEvent } from '../hooks/useWebSocket';
import { RoundWinner, GameOverData, GameResult, Player } from '../stores/useGameStore';

type StreamStatus = 'loading' | 'waiting' | 'live';

export default function StreamView() {
  const { gameId } = useParams<{ gameId: string }>();
  const [searchParams] = useSearchParams();
  const token = searchParams.get('token') ?? undefined;
  const { t } = useTranslation();

  const [status, setStatus] = useState<StreamStatus>('loading');
  const [cancelled, setCancelled] = useState(false);
  const [players, setPlayers] = useState<Player[]>([]);
  const [deckSlots, setDeckSlots] = useState<(number | null)[]>([]);
  const [remainingCards, setRemainingCards] = useState<Record<string, number>>({});
  const [currentTurn, setCurrentTurn] = useState<number | undefined>(undefined);
  const [roundWinner, setRoundWinner] = useState<RoundWinner | null>(null);
  const [gameOver, setGameOver] = useState<GameOverData | null>(null);

  const deckClearTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const roundWinnerClearTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const animationInFlightRef = useRef(false);

  const clearDeckSlots = useCallback(() => {
    setDeckSlots((prev) => prev.map(() => null));
    animationInFlightRef.current = false;
  }, []);

  useEffect(() => {
    return () => {
      if (deckClearTimerRef.current) clearTimeout(deckClearTimerRef.current);
      if (roundWinnerClearTimerRef.current) clearTimeout(roundWinnerClearTimerRef.current);
    };
  }, []);

  const onMessage = useCallback((event: GameEvent) => {
    switch (event.type) {
      case 'game_state_snapshot': {
        const snapshotPlayers: Player[] = event.players.map((p) => ({
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

        if (event.status === 'pending' || event.status === 'ready') {
          setStatus('waiting');
        } else {
          setStatus('live');
        }

        // While a round-completion animation is in flight, do not reset the
        // winner ring or repopulate the deck from the snapshot, otherwise the
        // collection animation would be torn down before it can clear the deck.
        if (!animationInFlightRef.current) {
          setDeckSlots(event.played_cards);
          setRoundWinner(null);
        }

        let turn: number | undefined;
        if (event.rank !== null && event.rank !== undefined) {
          const current = snapshotPlayers.find((p) => p.position === event.rank);
          turn = current?.display_position;
        }
        setCurrentTurn(turn);
        break;
      }
      case 'game_started': {
        const startedPlayers: Player[] = event.players.map((p) => ({
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
        startedPlayers.forEach((p) => {
          remaining[p.id] = p.cards_count ?? 0;
        });

        setPlayers(startedPlayers);
        setRemainingCards(remaining);
        setDeckSlots(new Array(startedPlayers.length).fill(null));
        const turnPlayer = startedPlayers.find((p) => p.id === event.current_turn);
        setCurrentTurn(turnPlayer?.display_position ?? 0);
        setRoundWinner(null);
        setCancelled(false);
        setStatus('live');
        break;
      }
      case 'game_cancelled': {
        setCancelled(true);
        setStatus('waiting');
        break;
      }
      case 'card_played': {
        setPlayers((prev) =>
          prev.map((p) =>
            p.id === event.player_id ? { ...p, cards: p.cards.filter((c) => c !== event.card_index) } : p
          )
        );
        setRemainingCards((prev) => {
          const next = { ...prev };
          if (next[event.player_id] !== undefined) {
            next[event.player_id] = Math.max(0, next[event.player_id] - 1);
          }
          return next;
        });
        setDeckSlots((prev) => {
          const next = [...prev];
          const empty = next.findIndex((slot) => slot === null);
          if (empty !== -1) next[empty] = event.card_index;
          return next;
        });
        setCurrentTurn((prev) => {
          if (!event.next_turn) return prev;
          const nextPlayer = players.find((p) => p.id === event.next_turn);
          return nextPlayer?.display_position ?? prev;
        });
        break;
      }
      case 'turn_changed': {
        const player = players.find((p) => p.id === event.current_turn);
        if (player) setCurrentTurn(player.display_position ?? player.position);
        break;
      }
      case 'round_completed': {
        const winnerPlayer = players.find((p) => p.id === event.winner_id);
        const winner: RoundWinner = {
          playerId: event.winner_id,
          position: winnerPlayer?.display_position ?? event.winner_position,
          winType: (event.win_type as 'normal' | 'kora' | 'doubleKora') || 'normal',
        };
        setRoundWinner(winner);
        animationInFlightRef.current = true;

        if (deckClearTimerRef.current) clearTimeout(deckClearTimerRef.current);
        if (roundWinnerClearTimerRef.current) clearTimeout(roundWinnerClearTimerRef.current);

        // Safety net: clear the deck just past the collection animation so the
        // deck resets even if the animation component is torn down early.
        deckClearTimerRef.current = setTimeout(() => {
          clearDeckSlots();
          deckClearTimerRef.current = null;
        }, 900);

        roundWinnerClearTimerRef.current = setTimeout(() => {
          setRoundWinner(null);
          roundWinnerClearTimerRef.current = null;
        }, 3000);
        break;
      }
      case 'game_finished': {
        const winnerPlayer = players.find((p) => p.id === event.winner_id) ?? null;
        const result: GameResult = {
          status: event.status as 'finished' | 'kora' | 'doubleKora',
          finalScore: event.final_score,
          roundsPlayed: event.rounds_played,
        };
        setGameOver({
          isGameOver: true,
          winner: winnerPlayer,
          result,
        });
        break;
      }
      default:
        break;
    }
  }, [players, clearDeckSlots]);

  const { isConnected } = useWebSocket({
    gameId: gameId || '',
    spectator: true,
    wsToken: token,
    onMessage,
  });

  if (status !== 'live') {
    return (
      <div className="min-h-screen flex items-center justify-center p-4">
        <div className="text-center">
          {status === 'loading' && (
            <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto mb-4"></div>
          )}
          <h1 className="text-xl font-bold mb-2">{t('stream.spectatorWaitingTitle')}</h1>
          <p className="text-gray-600">
            {cancelled ? t('stream.gameCancelled') : t('stream.spectatorWaitingSubtitle')}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen flex flex-col">
      <div className="flex-1">
        {!isConnected && (
          <div className="sticky top-0 z-30 bg-yellow-500 text-white text-center py-2 px-4 text-sm font-medium">
            {t('common.reconnecting')}
          </div>
        )}
        <GameTable
          players={players}
          currentTurn={currentTurn}
          deckSlots={deckSlots}
          remainingCards={remainingCards}
          roundWinner={roundWinner}
          gameOver={gameOver}
          spectatorMode
          onDeckAnimationComplete={clearDeckSlots}
        />
      </div>
    </div>
  );
}
