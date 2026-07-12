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
  stepByStep: boolean;
  setGame: (gameId: string, players: Player[], status: string, currentTurn: number, bet: number, deckSlots?: (number | null)[] | null) => void;
  resetGame: () => void;
  updatePlayerCards: (playerId: string, cards: number[]) => void;
  setCurrentTurn: (turn: number) => void;
  setDeckSlots: (slots: (number | null)[]) => void;
  setGameStatus: (status: string) => void;
  // Round completion
  setRoundWinner: (winner: RoundWinner | null) => void;
  clearRoundWinner: () => void;
  clearDeckSlots: () => void;
  // Game completion
  setGameOver: (gameOverData: GameOverData) => void;
  clearGameOver: () => void;
  // Helper to apply a CardPlayed event
  applyCardPlayed: (playerId: string, cardIndex: number, nextTurn?: string) => void;
  // Step-by-step mode
  setStepByStep: (active: boolean) => void;
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
  stepByStep: false,
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
      stepByStep: false,
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
