import { useCallback, useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import GameTable from './GameTable';
import { useWebSocket, GameEvent } from '../hooks/useWebSocket';
import { RoundWinner, GameOverData, GameResult, Player } from '../stores/useGameStore';

export default function StreamView() {
  const { gameId } = useParams<{ gameId: string }>();
  const [searchParams] = useSearchParams();
  const token = searchParams.get('token') ?? undefined;
  const { t } = useTranslation();

  const [players, setPlayers] = useState<Player[]>([]);
  const [deckSlots, setDeckSlots] = useState<(number | null)[]>([]);
  const [remainingCards, setRemainingCards] = useState<Record<string, number>>({});
  const [currentTurn, setCurrentTurn] = useState<number | undefined>(undefined);
  const [roundWinner, setRoundWinner] = useState<RoundWinner | null>(null);
  const [gameOver, setGameOver] = useState<GameOverData | null>(null);

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

        let turn: number | undefined;
        if (event.rank !== null && event.rank !== undefined) {
          const current = snapshotPlayers.find((p) => p.position === event.rank);
          turn = current?.display_position;
        }

        setPlayers(snapshotPlayers);
        setDeckSlots(event.played_cards);
        setRemainingCards(remaining);
        setCurrentTurn(turn);
        setRoundWinner(null);
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
  }, [players]);

  const { isConnected } = useWebSocket({
    gameId: gameId || '',
    spectator: true,
    wsToken: token,
    onMessage,
  });

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
        />
      </div>
    </div>
  );
}
