import { useEffect, useRef, useState, useCallback } from 'react';
import { getWsUrl } from '../utils/runtimeConfig';

const log = (...args: unknown[]) => {
  if (import.meta.env.DEV) console.log(...args);
};

export interface GameStartedPlayer {
  id: string;
  name: string;
  position: number;
  display_position: number;
  cards_count: number;
  player_type: string;
}

export interface GameStatePlayer {
  id: string;
  name: string;
  position: number;
  display_position: number;
  player_type: string;
}

export interface GameStateCard {
  player_id: string;
  card_index: number;
}

export type GameEvent =
  | { type: 'card_played'; game_id: string; player_id: string; card_index: number; next_turn?: string }
  | { type: 'round_completed'; game_id: string; round_number: number; winner_id: string; winner_position: number; win_type?: string; deck_slots: (number | null)[] }
  | { type: 'game_finished'; game_id: string; winner_id?: string; winner_name?: string; winner_position?: number; status: string; final_score?: number; rounds_played: number }
  | { type: 'turn_changed'; game_id: string; current_turn: string }
  | { type: 'player_joined'; game_id: string; player_id: string; user_id: string; pseudo: string; position: number; player_count: number; max_players: number }
  | { type: 'game_cancelled'; game_id: string; reason: string }
  | { type: 'game_ready'; game_id: string }
  | { type: 'cards_dealt'; game_id: string; player_id: string; cards: number[] }
  | { type: 'game_started'; game_id: string; players: GameStartedPlayer[]; current_turn: string }
  | { type: 'game_state_snapshot'; game_id: string; roll: number; rank: number | null; status: string; current_winning_card: number | null; current_winning_player_position: number | null; players: GameStatePlayer[]; played_cards: GameStateCard[]; step_by_step?: boolean }
  | { type: 'player_disconnected'; game_id: string; player_id: string; player_position: number; disconnected_at?: string }
  | { type: 'player_reconnected'; game_id: string; player_id: string; player_position: number; reconnected_at?: string }
  | { type: 'staleness_warning'; game_id: string; player_id: string; player_name: string; kicked_after_seconds: number }
  | { type: 'player_kicked'; game_id: string; player_id: string; player_name: string }
  | { type: 'game_reshuffled'; game_id: string; remaining_players: number }
  | { type: 'player_forfeit_win'; game_id: string; winner_id: string; winner_name: string };

export type OutgoingMessage =
  | { type: 'ping' }
  | { type: 'join_game'; game_id: string; player_id?: string; player_position?: number }
  | { type: 'leave_game' };

interface UseWebSocketOptions {
  gameId: string;
  playerId?: string;
  playerPosition?: number;
  wsToken?: string;
  onMessage?: (event: GameEvent) => void;
  onError?: (error: Event) => void;
  onClose?: (event: CloseEvent) => void;
  autoReconnect?: boolean;
  reconnectInterval?: number;
}

// Global WebSocket manager with pub/sub pattern
class WebSocketManager {
  private static instances = new Map<string, WebSocketManager>();

  private ws: WebSocket | null = null;
  private subscribers: Set<(event: GameEvent) => void> = new Set();
  private errorSubscribers: Set<(error: Event) => void> = new Set();
  private closeSubscribers: Set<(event: CloseEvent) => void> = new Set();
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private cleanupTimer: ReturnType<typeof setTimeout> | null = null;
  private isConnecting = false;
  private usageCount = 0;
  private gameId: string;
  private playerId: string | null = null;
  private playerPosition: number | null = null;
  private wsToken: string | null = null;

  private constructor(gameId: string) {
    this.gameId = gameId;
  }

  setWsToken(token: string | null): void {
    this.wsToken = token;
  }

  setPlayerIdentity(playerId: string, playerPosition: number): void {
    const hadNoIdentity = !this.playerId;
    this.playerId = playerId;
    this.playerPosition = playerPosition;

    if (hadNoIdentity && this.ws?.readyState === WebSocket.OPEN) {
      const joinMsg: OutgoingMessage = {
        type: 'join_game',
        game_id: this.gameId,
        player_id: playerId,
        player_position: playerPosition,
      };
      this.send(joinMsg);
    }
  }

  static getInstance(gameId: string): WebSocketManager {
    if (!gameId) {
      throw new Error('gameId is required');
    }

    if (!this.instances.has(gameId)) {
      this.instances.set(gameId, new WebSocketManager(gameId));
    }
    return this.instances.get(gameId)!;
  }

  static cleanupInstance(gameId: string): void {
    const instance = this.instances.get(gameId);
    if (instance) {
      instance.close();
      this.instances.delete(gameId);
    }
  }

  subscribe(
    onMessage?: (event: GameEvent) => void,
    onError?: (error: Event) => void,
    onClose?: (event: CloseEvent) => void
  ): () => void {
    // Cancel any pending cleanup timer since we have a new subscriber
    if (this.cleanupTimer) {
      clearTimeout(this.cleanupTimer);
      this.cleanupTimer = null;
    }

    if (onMessage) this.subscribers.add(onMessage);
    if (onError) this.errorSubscribers.add(onError);
    if (onClose) this.closeSubscribers.add(onClose);

    this.usageCount++;
    log(`New subscriber for game ${this.gameId}, usage count: ${this.usageCount}`);
    this.connect();

    // Return unsubscribe function
    return () => {
      if (onMessage) this.subscribers.delete(onMessage);
      if (onError) this.errorSubscribers.delete(onError);
      if (onClose) this.closeSubscribers.delete(onClose);

      this.usageCount--;
      log(`Subscriber removed for game ${this.gameId}, usage count: ${this.usageCount}`);

      if (this.usageCount <= 0) {
        // Schedule cleanup after a delay instead of immediate cleanup
        // This handles React StrictMode mount/unmount cycles
        log(`No more subscribers for game ${this.gameId}, scheduling cleanup in 10 seconds`);
        this.cleanupTimer = setTimeout(() => {
          log(`Cleaning up WebSocketManager for game ${this.gameId} after grace period`);
          this.cleanupTimer = null;
          this.close();
          WebSocketManager.instances.delete(this.gameId);
        }, 10000); // 10 second grace period
      }
    };
  }

  send(message: OutgoingMessage): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      const json = JSON.stringify(message);
      log('Sending WebSocket message:', json);
      this.ws.send(json);
    } else {
      console.warn('WebSocket not connected, cannot send message');
    }
  }

  private connect(): void {
    if (this.isConnecting) {
      log('Already connecting to game', this.gameId);
      return;
    }

    // Check if we have a usable WebSocket
    if (this.ws) {
      const state = this.ws.readyState;
      if (state === WebSocket.CONNECTING || state === WebSocket.OPEN) {
        log('WebSocket already connecting or open for game', this.gameId, 'state:', state);
        return;
      }
      // If WebSocket is CLOSING or CLOSED, we need a new one
      log('WebSocket exists but in state', state, 'for game', this.gameId, 'creating new connection');
      this.ws = null;
    }

    this.isConnecting = true;
    log('Creating WebSocket connection to game', this.gameId);

    const basePath = `/ws/${this.gameId}`;
    const queryString = this.wsToken ? `?token=${encodeURIComponent(this.wsToken)}` : '';
    const url = getWsUrl(`${basePath}${queryString}`);

    const ws = new WebSocket(url);
    this.ws = ws;

    ws.onopen = () => {
      this.isConnecting = false;
      log(`WebSocket connected to game ${this.gameId}`);
      // Send join message with player identity if available
      const joinMsg: OutgoingMessage = {
        type: 'join_game',
        game_id: this.gameId,
        ...(this.playerId ? { player_id: this.playerId } : {}),
        ...(this.playerPosition !== null ? { player_position: this.playerPosition } : {}),
      };
      this.send(joinMsg);
    };

      ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.type && ['card_played', 'round_completed', 'game_finished', 'turn_changed', 'player_joined', 'game_cancelled', 'game_ready', 'cards_dealt', 'game_started', 'game_state_snapshot', 'player_disconnected', 'player_reconnected', 'staleness_warning', 'player_kicked', 'game_reshuffled', 'player_forfeit_win'].includes(data.type)) {
          log('Received GameEvent:', data);
          this.subscribers.forEach(callback => callback(data as GameEvent));
        } else {
          log('Received non-GameEvent message:', data);
        }
      } catch (err) {
        console.error('Failed to parse WebSocket message:', err);
      }
    };

    ws.onerror = (error) => {
      this.isConnecting = false;
      console.error('WebSocket error for game', this.gameId, error);
      this.errorSubscribers.forEach(callback => callback(error));
    };

    ws.onclose = (event) => {
      this.isConnecting = false;
      log(`WebSocket closed for game ${this.gameId}`, event.code, event.reason);
      this.closeSubscribers.forEach(callback => callback(event));

      // Clear the WebSocket reference
      this.ws = null;

      // Schedule reconnect if there are still subscribers
      if (this.subscribers.size > 0) {
        const reconnectInterval = 5000; // Minimum 5 seconds
        log(`Scheduling reconnect in ${reconnectInterval}ms`);
        this.reconnectTimer = setTimeout(() => {
          this.reconnectTimer = null;
          this.connect();
        }, reconnectInterval);
      }
    };
  }

  private close(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    if (this.cleanupTimer) {
      clearTimeout(this.cleanupTimer);
      this.cleanupTimer = null;
    }

    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }

    this.isConnecting = false;
  }

  getConnectionStatus(): 'connecting' | 'connected' | 'disconnected' {
    if (this.isConnecting) return 'connecting';
    if (this.ws && this.ws.readyState === WebSocket.OPEN) return 'connected';
    return 'disconnected';
  }
}

/**
 * A React hook that manages a WebSocket connection to the game server.
 * Uses a global WebSocket manager to ensure only one connection per game.
 *
 * @param options Configuration options
 * @returns Object containing connection status and a send function.
 */
export function useWebSocket({
  gameId,
  playerId,
  playerPosition,
  wsToken,
  onMessage,
  onError,
  onClose,
  // Note: autoReconnect and reconnectInterval are handled by WebSocketManager internally
  // with fixed values (always reconnects after 5 seconds if there are subscribers)
  autoReconnect = true,
  reconnectInterval = 3000,
}: UseWebSocketOptions) {
  // These parameters are intentionally not used in this implementation
  // as WebSocketManager handles reconnection internally
  void autoReconnect;
  void reconnectInterval;
  const [isConnected, setIsConnected] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);
  const unsubscribeRef = useRef<(() => void) | null>(null);
  const lastAppliedTokenRef = useRef<string | null | undefined>(undefined);

  // Convert connection status to boolean
  const updateConnectionStatus = useCallback(() => {
    if (!gameId) {
      setIsConnected(false);
      return;
    }

    try {
      const manager = WebSocketManager.getInstance(gameId);
      const status = manager.getConnectionStatus();
      setIsConnected(status === 'connected');
    } catch (err) {
      setIsConnected(false);
    }
  }, [gameId]);

  const send = useCallback((message: OutgoingMessage) => {
    if (!gameId) {
      console.warn('Cannot send message without gameId');
      return;
    }

    try {
      const manager = WebSocketManager.getInstance(gameId);
      manager.send(message);
    } catch (err) {
      console.error('Failed to send WebSocket message:', err);
    }
  }, [gameId]);

  useEffect(() => {
    if (!gameId) {
      return;
    }

    // Create wrapped callbacks that also update state
    const wrappedOnMessage = onMessage ? (event: GameEvent) => {
      onMessage(event);
    } : undefined;

    const wrappedOnError = onError ? (error: Event) => {
      setLastError('WebSocket connection error');
      onError(error);
    } : undefined;

    const wrappedOnClose = onClose ? (event: CloseEvent) => {
      setIsConnected(false);
      onClose(event);
    } : undefined;

    // Subscribe to the WebSocket manager
    try {
      const manager = WebSocketManager.getInstance(gameId);

      // Set the one-time game token on the manager (for unauthenticated users).
      // Only apply when the token changes to avoid reconnecting needlessly.
      if (wsToken && wsToken !== lastAppliedTokenRef.current) {
        lastAppliedTokenRef.current = wsToken;
        manager.setWsToken(wsToken);
      } else if (!wsToken) {
        lastAppliedTokenRef.current = undefined;
      }

      // Set player identity on the manager so it's included in join_game message
      if (playerId && playerPosition !== undefined) {
        manager.setPlayerIdentity(playerId, playerPosition);
      }

      unsubscribeRef.current = manager.subscribe(
        wrappedOnMessage,
        wrappedOnError,
        wrappedOnClose
      );

      // Initial status update
      updateConnectionStatus();

      // Set up interval to check connection status
      const statusInterval = setInterval(updateConnectionStatus, 1000);

      return () => {
        clearInterval(statusInterval);
        if (unsubscribeRef.current) {
          unsubscribeRef.current();
          unsubscribeRef.current = null;
        }
      };
    } catch (err) {
      console.error('Failed to subscribe to WebSocket manager:', err);
      setLastError('Invalid gameId');
    }
  }, [gameId, playerId, playerPosition, wsToken, onMessage, onError, onClose, updateConnectionStatus]);

  // Expose a manual reconnect function
  const reconnect = useCallback(() => {
    if (!gameId) return;

    try {
      WebSocketManager.cleanupInstance(gameId);
      // Get new instance - this will trigger reconnection
      WebSocketManager.getInstance(gameId);
      // The next status update will reflect the reconnection
    } catch (err) {
      console.error('Failed to reconnect:', err);
    }
  }, [gameId]);

  return { isConnected, lastError, send, reconnect };
}

export default useWebSocket;
