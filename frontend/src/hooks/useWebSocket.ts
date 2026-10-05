import { useEffect, useRef, useState, useCallback } from 'react';
import { getWsUrl } from '../utils/runtimeConfig';
import type { SpecialCards } from '../stores/useGameStore';

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
  cards_count: number;
}

export interface GameStateCard {
  player_id: number;
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
  | { type: 'cards_dealt'; game_id: string; player_id: string; cards: number[]; special_cards?: SpecialCards }
  | { type: 'game_started'; game_id: string; players: GameStartedPlayer[]; current_turn: string; game_mode: string }
  | { type: 'game_state_snapshot'; game_id: string; roll: number; rank: number | null; status: string; current_winning_card: number | null; current_winning_player_position: number | null; players: GameStatePlayer[]; played_cards: (number | null)[]; step_by_step?: boolean; game_mode?: string; claim_pending?: boolean; claim_offered_to_me?: boolean; claim_offered_to_position?: number | null; special_cards?: SpecialCards }
  | { type: 'player_disconnected'; game_id: string; player_id: string; player_position: number; disconnected_at?: string }
  | { type: 'player_reconnected'; game_id: string; player_id: string; player_position: number; reconnected_at?: string }
  | { type: 'staleness_warning'; game_id: string; player_id: string; player_name: string; kicked_after_seconds: number }
  | { type: 'player_kicked'; game_id: string; player_id: string; player_name: string }
  | { type: 'game_reshuffled'; game_id: string; remaining_players: number }
  | { type: 'player_forfeit_win'; game_id: string; winner_id: string; winner_name: string }
  | { type: 'claim_pending'; game_id: string }
  | { type: 'claim_offered'; game_id: string; player_id: string; special_cards: SpecialCards }
  | { type: 'claim_resolved'; game_id: string }
  | { type: 'special_claim'; game_id: string; player_id: string; cards: number[]; winner_position: number };

export type OutgoingMessage =
  | { type: 'ping' }
  | { type: 'join_game'; game_id: string; player_id?: string; player_position?: number; spectator?: boolean }
  | { type: 'leave_game' };

interface UseWebSocketOptions {
  gameId: string;
  playerId?: string;
  playerPosition?: number;
  wsToken?: string;
  spectator?: boolean;
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
  private spectator = false;

  private constructor(gameId: string) {
    this.gameId = gameId;
  }

  setWsToken(token: string | null): void {
    if (this.wsToken === token) {
      return;
    }
    this.wsToken = token;
    // The token is part of the connection URL, so a real change must be applied
    // by reconnecting. On first mount this is a no-op because the socket has not
    // been opened yet (subscribe/connect runs after this call).
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.forceReconnect();
    }
  }

  setSpectator(value: boolean): void {
    // A connection that has a known player identity is never a spectator. This
    // guards against a spectator hook instance (or a stale render) flipping the
    // flag on a shared per-game manager and causing the backend to stop sending
    // personalized events to a real player.
    if (value && this.playerId) {
      return;
    }
    if (this.spectator === value) {
      return;
    }
    this.spectator = value;
    // Re-announce the connection so the backend applies the new role. Without
    // this, a manager that was first created as a spectator would keep the
    // spectator flag on the server even after the role changes.
    this.sendJoinIfOpen();
  }

  setPlayerIdentity(playerId: string, playerPosition: number): void {
    const identityChanged =
      this.playerId !== playerId || this.playerPosition !== playerPosition;

    this.playerId = playerId;
    this.playerPosition = playerPosition;
    // A player identity always wins over a spectator flag.
    this.spectator = false;

    // Re-send join_game whenever the identity changes (not only the first time).
    // The effect that calls this can re-run many times; if the socket was
    // reconnected in between, the backend needs to be told who we are again,
    // otherwise the connection stays anonymous and stops receiving personalized
    // events (the player's hand/deck would freeze while spectators stay correct).
    if (identityChanged) {
      this.sendJoinIfOpen();
    }
  }

  /// Send the join_game handshake if the socket is currently open.
  private sendJoinIfOpen(): void {
    if (this.ws?.readyState !== WebSocket.OPEN) {
      return;
    }
    const joinMsg: OutgoingMessage = {
      type: 'join_game',
      game_id: this.gameId,
      ...(this.playerId ? { player_id: this.playerId } : {}),
      ...(this.playerPosition !== null ? { player_position: this.playerPosition } : {}),
      ...(this.spectator ? { spectator: true } : {}),
    };
    this.send(joinMsg);
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
        ...(this.spectator ? { spectator: true } : {}),
      };
      this.send(joinMsg);
    };

      ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (data.type && ['card_played', 'round_completed', 'game_finished', 'turn_changed', 'player_joined', 'game_cancelled', 'game_ready', 'cards_dealt', 'game_started', 'game_state_snapshot', 'player_disconnected', 'player_reconnected', 'staleness_warning', 'player_kicked', 'game_reshuffled', 'player_forfeit_win', 'claim_pending', 'claim_offered', 'claim_resolved', 'special_claim'].includes(data.type)) {
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

  /// Force a fresh connection while preserving identity, token, spectator flag
  /// and subscribers. Closing the live socket lets the existing `onclose` handler
  /// schedule the reconnect and re-send `join_game` on open.
  forceReconnect(): void {
    const ws = this.ws;
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
      ws.close();
      return;
    }
    // No live socket (e.g. already closed and waiting on the reconnect timer):
    // connect immediately.
    this.connect();
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
  spectator,
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

  // Keep the latest callbacks in refs so the subscription effect does not need
  // to depend on them. Callers (e.g. useGameWebSocket) recreate `onMessage` on
  // every render; depending on it directly would tear down and re-create the
  // subscription constantly, which is what allowed the identity/spectator race
  // to corrupt the shared per-game connection.
  const onMessageRef = useRef(onMessage);
  const onErrorRef = useRef(onError);
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onMessageRef.current = onMessage;
    onErrorRef.current = onError;
    onCloseRef.current = onClose;
  }, [onMessage, onError, onClose]);

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
    } catch {
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

    // Create wrapped callbacks that also update state. They read the latest
    // handler from the refs so the subscription stays stable across renders.
    const wrappedOnMessage = (event: GameEvent) => {
      onMessageRef.current?.(event);
    };

    const wrappedOnError = (error: Event) => {
      setLastError('WebSocket connection error');
      onErrorRef.current?.(error);
    };

    const wrappedOnClose = (event: CloseEvent) => {
      setIsConnected(false);
      onCloseRef.current?.(event);
    };

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

      // Mark this connection as a read-only spectator when requested.
      manager.setSpectator(spectator === true);

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
    // NOTE: onMessage/onError/onClose are intentionally NOT in the dependency
    // array. They are read through refs (see onMessageRef/onErrorRef/onCloseRef)
    // so that a new callback identity on every render does not tear down and
    // rebuild the WebSocket subscription. Re-subscribing on every render was the
    // root cause of the identity/spectator race: the manager would be re-joined
    // repeatedly and could be mis-attributed as a spectator, causing a player's
    // own hand to stop updating while the spectator tab stayed correct.
  }, [gameId, playerId, playerPosition, wsToken, spectator, updateConnectionStatus]);

  // Expose a manual reconnect function
  const reconnect = useCallback(() => {
    if (!gameId) return;

    try {
      WebSocketManager.getInstance(gameId).forceReconnect();
    } catch (err) {
      console.error('Failed to reconnect:', err);
    }
  }, [gameId]);

  return { isConnected, lastError, send, reconnect };
}

export default useWebSocket;
