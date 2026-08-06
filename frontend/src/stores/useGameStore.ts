import { create } from 'zustand';

export interface Player {
  id: string;
  type: 'human' | 'bot';
  name: string;
  position: number;
  display_position: number;
  cards: number[];
  cards_count?: number;
}

export interface RoundWinner {
  playerId: string | null;
  position: number | null;
  winType: 'normal' | 'kora' | 'doubleKora';
}

export interface GameResult {
  status: 'finished' | 'kora' | 'doubleKora';
  finalScore?: number;
  roundsPlayed: number;
}

export interface GameOverData {
  isGameOver: boolean;
  winner: Player | null;
  result: GameResult;
}

export type StepByStepPhase = 'idle' | 'human_turn' | 'bot_turn' | 'evaluate_round';

export type QueuedBotEvent =
  | { kind: 'bot_play'; playerId: string; cardIndex: number; nextTurnPlayerId: string }
  | { kind: 'round_pause'; winner: RoundWinner | null };

export interface GameState {
  gameId: string | null;
  players: Player[];
  status: string;
  currentTurn: number;
  bet: number;
  deckSlots: (number | null)[];
  remainingCards: Record<string, number>;
  roundWinner: RoundWinner | null;
  gameOver: GameOverData | null;
  pendingGameOver: GameOverData | null;
  stepByStep: boolean;
  pendingBotMoves: QueuedBotEvent[];
  isReplayingBots: boolean;
  botReplayTimerId: ReturnType<typeof setTimeout> | null;
  roundWinnerClearTimerId: ReturnType<typeof setTimeout> | null;
  isBotChainActive: boolean;
  botThinkingDelayMs: number;
  roundPauseDelayMs: number;
  setGame: (gameId: string, players: Player[], status: string, currentTurn: number, bet: number, deckSlots?: (number | null)[] | null) => void;
  resetGame: () => void;
  updatePlayerCards: (playerId: string, cards: number[]) => void;
  setCurrentTurn: (turn: number) => void;
  setDeckSlots: (slots: (number | null)[]) => void;
  setGameStatus: (status: string) => void;
  setRoundWinner: (winner: RoundWinner | null) => void;
  clearRoundWinner: () => void;
  clearDeckSlots: () => void;
  setGameOver: (gameOverData: GameOverData) => void;
  setPendingGameOver: (gameOverData: GameOverData | null) => void;
  clearGameOver: () => void;
  applyCardPlayed: (playerId: string, cardIndex: number, nextTurn?: string) => void;
  setStepByStep: (active: boolean) => void;
  addPendingEvent: (event: QueuedBotEvent) => void;
  clearPendingEvents: () => void;
  startBotReplay: (botDelayMs: number, roundPauseMs: number) => void;
  cancelBotReplay: () => void;
  flushPendingEvents: () => void;
  setBotDelays: (botThinkingDelayMs: number, roundPauseDelayMs: number) => void;
}

export const useGameStore = create<GameState>((set, get) => ({
  gameId: null,
  players: [],
  status: 'pending',
  currentTurn: 0,
  bet: 10,
  deckSlots: [null, null, null, null],
  remainingCards: {},
  roundWinner: null,
  gameOver: null,
  pendingGameOver: null,
  stepByStep: false,
  pendingBotMoves: [],
  isReplayingBots: false,
  botReplayTimerId: null,
  roundWinnerClearTimerId: null,
  isBotChainActive: false,
  botThinkingDelayMs: 800,
  roundPauseDelayMs: 2500,
  setGame: (gameId, players, status, currentTurn, bet, deckSlots?) => {
    const remainingCards: Record<string, number> = {};
    const playersWithDisplay = players.map((p) => ({
      ...p,
      display_position: p.display_position ?? p.position,
    }));
    playersWithDisplay.forEach((p) => {
      remainingCards[p.id] = p.cards_count ?? p.cards.length;
    });
    const resolvedDeckSlots: (number | null)[] = deckSlots && deckSlots.length === players.length
      ? deckSlots
      : new Array(players.length).fill(null);
    set({ gameId, players: playersWithDisplay, status, currentTurn, bet, remainingCards, deckSlots: resolvedDeckSlots });
  },
  resetGame: () =>
    set({
      gameId: null,
      players: [],
      status: 'pending',
      currentTurn: 0,
      bet: 10,
      deckSlots: [],
      remainingCards: {},
      roundWinner: null,
      gameOver: null,
      pendingGameOver: null,
      stepByStep: false,
      pendingBotMoves: [],
      isReplayingBots: false,
      isBotChainActive: false,
      roundWinnerClearTimerId: null,
    }),
  updatePlayerCards: (playerId, cards) =>
    set((state) => ({
      players: state.players.map((p) =>
        p.id === playerId ? { ...p, cards } : p
      ),
    })),
  setCurrentTurn: (currentTurn) =>
    set({ currentTurn }),
  setDeckSlots: (deckSlots) =>
    set({ deckSlots }),
  setGameStatus: (status) =>
    set({ status }),
  setRoundWinner: (winner) =>
    set({ roundWinner: winner }),
  clearRoundWinner: () =>
    set({ roundWinner: null }),
  clearDeckSlots: () => {
    const players = get().players;
    set({ deckSlots: new Array(players.length).fill(null) });
  },
  setGameOver: (gameOverData) =>
    set({ gameOver: gameOverData }),
  setPendingGameOver: (gameOverData) =>
    set({ pendingGameOver: gameOverData }),
  clearGameOver: () =>
    set({ gameOver: null }),
  applyCardPlayed: (playerId, cardIndex, nextTurn) => {
    const state = get();
    // Remove the card from the player's hand
    const updatedPlayers = state.players.map((player) => {
      if (player.id === playerId) {
        const newCards = player.cards.filter((c) => c !== cardIndex);
        return { ...player, cards: newCards };
      }
      return player;
    });
    // Decrement remaining count for the player who played
    const updatedRemaining = { ...state.remainingCards };
    if (updatedRemaining[playerId] !== undefined) {
      updatedRemaining[playerId] = Math.max(0, updatedRemaining[playerId] - 1);
    }
    // Determine next turn position if nextTurn player ID is provided
    let nextTurnPosition = state.currentTurn;
    if (nextTurn) {
      const nextPlayer = updatedPlayers.find((p) => p.id === nextTurn);
      if (nextPlayer) {
        nextTurnPosition = nextPlayer.display_position;
      }
    } else {
      const maxDisplayPos = Math.max(...updatedPlayers.map((p) => p.display_position), 0);
      nextTurnPosition = (state.currentTurn + 1) % (maxDisplayPos + 1);
    }
    // Update deck slots? For simplicity, we can add the played card to the first empty slot
    const newDeckSlots = [...state.deckSlots];
    const emptyIndex = newDeckSlots.findIndex((slot) => slot === null);
    if (emptyIndex !== -1) {
      newDeckSlots[emptyIndex] = cardIndex;
    }
    set({
      players: updatedPlayers,
      currentTurn: nextTurnPosition,
      deckSlots: newDeckSlots,
      remainingCards: updatedRemaining,
    });
  },
  setStepByStep: (active) =>
    set({ stepByStep: active }),
  addPendingEvent: (event) =>
    set((state) => ({
      pendingBotMoves: [...state.pendingBotMoves, event],
    })),
  clearPendingEvents: () =>
    set({ pendingBotMoves: [] }),
  startBotReplay: (botDelayMs, roundPauseMs) => {
    const state = get();
    if (state.isReplayingBots) return;
    if (state.pendingBotMoves.length === 0) return;

    set({ isReplayingBots: true });

    const replayNext = () => {
      const current = get();
      if (current.pendingBotMoves.length === 0) {
        set({ isReplayingBots: false, isBotChainActive: false, botReplayTimerId: null });
        // If a game-over was deferred while the bot chain was replaying, apply
        // it now that the last bot card (and round_pause) have been shown.
        if (current.pendingGameOver) {
          set({ gameOver: current.pendingGameOver, pendingGameOver: null });
        }
        return;
      }

      const [nextEvent, ...remaining] = current.pendingBotMoves;
      set({ pendingBotMoves: remaining });

      let nextDelay: number;

      if (nextEvent.kind === 'round_pause') {
        // Round boundary: show the winner and clear the deck now that the
        // last card of the round has been applied by the replay.
        current.setRoundWinner(nextEvent.winner);
        current.clearDeckSlots();
        if (current.roundWinnerClearTimerId) {
          clearTimeout(current.roundWinnerClearTimerId);
        }
        const clearTimer = setTimeout(() => {
          set({ roundWinner: null, roundWinnerClearTimerId: null });
        }, roundPauseMs);
        set({ roundWinnerClearTimerId: clearTimer });
        nextDelay = roundPauseMs;
      } else {
        current.applyCardPlayed(nextEvent.playerId, nextEvent.cardIndex, nextEvent.nextTurnPlayerId);
        if (remaining.length > 0 && remaining[0].kind === 'round_pause') {
          nextDelay = roundPauseMs;
        } else {
          nextDelay = botDelayMs;
        }
      }

      const timerId = setTimeout(replayNext, nextDelay);
      set({ botReplayTimerId: timerId });
    };

    const timerId = setTimeout(replayNext, botDelayMs);
    set({ botReplayTimerId: timerId });
  },
  cancelBotReplay: () => {
    const state = get();
    if (state.botReplayTimerId !== null) {
      clearTimeout(state.botReplayTimerId);
    }
    if (state.roundWinnerClearTimerId !== null) {
      clearTimeout(state.roundWinnerClearTimerId);
    }
    set({
      pendingBotMoves: [],
      isReplayingBots: false,
      isBotChainActive: false,
      botReplayTimerId: null,
      roundWinnerClearTimerId: null,
      // If a game-over was deferred and the replay is being cancelled for any
      // reason, apply it now so it is never lost.
      ...(state.pendingGameOver
        ? { gameOver: state.pendingGameOver, pendingGameOver: null }
        : {}),
    });
  },
  flushPendingEvents: () => {
    const state = get();
    for (const event of state.pendingBotMoves) {
      if (event.kind === 'bot_play') {
        state.applyCardPlayed(event.playerId, event.cardIndex, event.nextTurnPlayerId);
      }
    }
    state.cancelBotReplay();
  },
  setBotDelays: (botThinkingDelayMs, roundPauseDelayMs) =>
    set({ botThinkingDelayMs, roundPauseDelayMs }),
}));

export const useStepByStepPhase = (): StepByStepPhase => {
  return useGameStore((state) => {
    if (!state.stepByStep) return 'idle';
    if (state.gameOver?.isGameOver) return 'idle';

    const allSlotsFilled = state.deckSlots.length > 0
      && state.deckSlots.every(slot => slot !== null);
    if (allSlotsFilled) return 'evaluate_round';

    const currentPlayer = state.players.find(
      p => p.display_position === state.currentTurn
    );
    if (!currentPlayer) return 'idle';

    return currentPlayer.type === 'bot' ? 'bot_turn' : 'human_turn';
  });
};
