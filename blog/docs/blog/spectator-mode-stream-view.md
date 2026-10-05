# Spectator Mode: A Native Read-Only Stream View for a Multiplayer Card Game

**Date:** 2026-09-17
**Tags:** `websocket`, `rust`, `actix-web`, `redis`, `react`, `spectator`, `streaming`, `privacy`, `scalability`

---

## The Problem

A streamer wants to broadcast their gameplay to Twitch or TikTok through OBS. The
obvious approach — capture the browser window — has a fatal flaw: **the streamer's
own hand is visible on screen**. Every viewer, and every opponent watching the
stream, can see the cards the streamer is holding. That is a competitive integrity
problem, not just a cosmetic one.

The requirement decomposes into two distinct surfaces:

| Surface | Concern | Status |
|---|---|---|
| A. Opponent privacy | A player must not see opponents' unplayed cards | Already enforced at the data layer |
| B. Streamer hand privacy | Viewers and opponents must not see the streamer's hand | Implemented by the spectator role |

Surface A was already solved by construction: opponent hands are **never sent** to
the frontend. The API builds game state with `cards = if is_me { my_cards } else { Vec::new() }`,
and the private `cards_dealt` event is routed only to its owner. The frontend
renders empty hands face-down.

Surface B is what this post is about. The solution is a **first-class, read-only
spectator role** — a native spectator view, not a headless browser relay.

---

## Why a Native Spectator Role

The alternative to a native role is a **headless browser / CDP relay**: run a
browser, load the game as a fake player, and re-broadcast the rendered output. That
approach is heavy, fragile, and — critically — it would still need a way to hide
the streamer's hand, which means it would need a spectator mode anyway.

Instead, we reuse the transport we already have. A spectator is **not a special
transport**. It is a normal WebSocket connection that is:

1. **Flagged** as a spectator (`spectator == true`, `player_id == None`).
2. **Fed a reduced, non-personalized view** — public state only, no hand data.

This is the industry-standard "native spectator view" approach. It costs one extra
boolean on the connection and one extra snapshot builder, and it inherits all the
existing WebSocket infrastructure: authentication, reconnection, heartbeats, and
the Redis pub/sub bridge.

The streamer keeps their normal play window (own hand face-up). The stream window
is a separate route, `/game/:gameId/stream`, loaded into OBS as a Browser Source.

---

## Architecture Overview

![Spectator mode token and stream flow](../images/spectator-mode-flow.png)

The flow has three phases:

1. **Mint** — a participant requests a short-lived, revocable spectator token.
2. **Join** — the stream view opens a WebSocket with that token and joins as a
   spectator.
3. **Feed** — the backend sends a public-only snapshot on join and on every state
   change, and the frontend renders it with all hands face-down.

---

## Backend Deep Dive

### 1. Minting the spectator token

The entry point is `mint_spectate_token()`,
exposed as `POST /api/games/{game_id}/spectate-token` in
`routes.rs`.

```rust
/// Mint a private, short-lived spectator token for the stream view.
/// Only a game participant may mint a token, and the token grants read-only
/// access to public game state (no hand data) via the WebSocket.
pub async fn mint_spectate_token(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<DashboardServiceType>>,
    auth_config: web::Data<AuthConfig>,
    redis: web::Data<Option<RedisClient>>,
) -> HttpResponse {
```

Three properties matter here:

- **Participant-only.** The handler calls `service.check_existing_players(game_id)`
  and returns `403 Forbidden` if the caller is not a participant. A random
  authenticated user cannot mint a token for someone else's game.
- **Short TTL.** `SPECTATE_TOKEN_TTL_SECS` is
  `21600` seconds (6 hours) — long enough for a streaming session, short enough to
  bound exposure.
- **Redis-backed and revocable.** The token is stored under
  `spectate_token:{game_id}:{jti}` with the same TTL. Deleting the key revokes the
  token immediately, even before the JWT expires.

The response is a `SpectateTokenResponse`
containing both the raw token and a ready-to-use URL:

```rust
let url = format!(
    "{}/game/{}/stream?token={}",
    auth_config.frontend_url, game_id, token
);
```

That URL is what the streamer pastes into OBS.

### 2. The token itself

The token is a JWT signed with the same secret as every other token, but
distinguished by its **purpose** claim. `generate_spectate_token()`
sets `purpose: "ws:spectate"`, and `validate_spectate_token()`
rejects any token whose purpose is not exactly that:

```rust
if token_data.claims.purpose != "ws:spectate" {
    return Err(jsonwebtoken::errors::Error::from(
        jsonwebtoken::errors::ErrorKind::InvalidSubject,
    ));
}
```

This purpose separation is what prevents a spectator token from being used as a
game token, and vice versa. A spectator token cannot be replayed as a player
identity because the purpose check fails.

### 3. WebSocket authentication fallback chain

The game WebSocket handler in `mod.rs` resolves
identity in a specific order:

1. **Auth cookie** → `validate_ws_token()` for
   logged-in players.
2. **`token` query parameter** → first tried as a one-time game token via
   `validate_game_token()`; if that fails, it is
   tried as a spectator token via
   `validate_spectate_token()`.

```rust
let game_id_auth =
    validate_game_token(gt, game_id, auth_config.clone(), redis_client.clone()).await;
if game_id_auth.is_some() {
    game_id_auth
} else {
    validate_spectate_token(gt, game_id, auth_config.clone(), redis_client.clone())
        .await
}
```

The spectator validator enforces two things: the JWT purpose must be `ws:spectate`
with a matching `game_id`, and the `spectate_token:{game_id}:{jti}` key must exist
in Redis. If Redis is unavailable, the check **fails open** (the JWT signature and
expiry have already been validated) and increments
`WS_TOKEN_VALIDATION_REDIS_ERRORS_TOTAL`.

### 4. Deterministic spectator identity

A spectator has no user account, but the connection still needs an identity for
logging and tracing. `derive_spectate_id()`
derives one deterministically from the game id:

```rust
const SPECTATE_NAMESPACE: u128 = 0x0000_5350_4543_5441_5445_5F41_4E4F_4E5F_4944_u128;

pub(crate) fn derive_spectate_id(game_id: Uuid) -> Uuid {
    Uuid::from_u128(game_id.as_u128() ^ SPECTATE_NAMESPACE)
}
```

The namespace constant is deliberately distinct from the anonymous-player
namespace, and there is a unit test asserting the two never collide
(`auth.rs`). This identity is **not** a player
identity — it is never used to look up a hand or authorize an action.

### 5. Flagging the connection as a spectator

After the WebSocket handshake, the client sends a `join_game` message. The
`JoinGame` variant carries a `spectator`
flag:

```rust
player_position: Option<i32>,
/// Read-only spectator join (no player identity, public state only).
#[serde(default)]
spectator: bool,
```

When `spectator` is true, the handler calls
`mark_spectator_for_connection()`,
passing the *specific* `connection_id` that carried the join, then immediately
sends a snapshot:

```rust
if spectator {
    // Read-only spectator join: public state only, no player identity.
    manager
        .mark_spectator_for_connection(game_id, connection_id)
        .await;
    if let Some(db) = &db {
        send_spectator_snapshot(manager.get_ref(), db, game_id).await;
    }
}
```

The `connection_id` is threaded explicitly through `handle_message()` from the
handshake in `ws_handler()`. This matters: the join is attributed to *this*
connection, never to the "most recently added" one. The earlier
`mark_spectator_for_latest_connection()` was a real bug source — a stale
reconnect or a concurrent spectator could be the latest entry and would steal the
flag, mis-routing the player's personalized events.

`mark_spectator_for_connection()` does two things: it sets the spectator flag and
clears any `player_id`/`player_position` on that connection, since a spectator
never has a hand and must not keep receiving per-player events.

The connection record itself carries the flag
(`connection.rs`):

```rust
pub(crate) last_pong: Instant,
pub(crate) spectator: bool,
```

### 5.1 Identity consolidation: player wins

The converse of the spectator join is a player joining on a connection that was
previously marked spectator (or a stale render flipping the flag). The backend
resolves this with one invariant: **a player identity always wins over the
spectator flag.** `set_player_for_connection()` — which the handler now calls for
the exact `connection_id` rather than a "latest connection" helper — clears the
flag as it sets the identity:

```rust
if conn.id == connection_id {
    conn.player_id = Some(player_id);
    conn.player_position = Some(player_position);
    // A player identity always wins over a spectator flag. Without
    // this, a connection first marked spectator (or a stale render)
    // would keep receiving public-only snapshots and stop receiving
    // its personalized hand/deck.
    conn.spectator = false;
    break;
}
```

Symmetrically, `mark_spectator_for_connection()` clears the player identity when
it sets the flag. Together these two make the connection role idempotent: applying
either role wipes the other, so a connection can never be simultaneously "a
player" and "a spectator".
 <!-- There is a dedicated test for this
(`test_player_identity_clears_spectator_flag`) that marks a connection spectator,
then joins a real player on the same connection, and asserts it receives its
`cards_dealt` event. -->

### 6. The public-only snapshot

`send_spectator_snapshot()` is the heart
of the privacy guarantee. It builds a snapshot with a **fixed seat orientation** —
`display_position = p.position` — so every spectator sees the same table layout,
unlike players whose view is rotated so they always sit at the bottom.

```rust
let game_state_players: Vec<GameStatePlayer> = players
    .iter()
    .map(|p| {
        GameStatePlayer {
            id: p.id,
            name: p.name.clone(),
            position: p.position,
            display_position: p.position,   // fixed orientation, no rotation
            player_type: player_type_str.to_string(),
            cards_count: CARDS_PER_PLAYER as i32
                - *played_counts.get(&p.id).unwrap_or(&0) as i32,
        }
    })
    .collect();
```

The snapshot contains only public information:

- `players` with `cards_count` (a number, never card values)
- `played_cards` — the cards on the deck, built via `build_played_card_slots()`
- `roll`, `rank`, `status`, `current_winning_card`, `current_winning_player_position`
- `game_mode`, `step_by_step`, `claim_pending`

And it explicitly omits the private fields, while still exposing which position a
claim is being offered to:

```rust
claim_offered_to_me: false,
claim_offered_to_position,
special_cards: None,
```

`special_cards` is the recipient's own special-card flags — a spectator has none,
so it is `None`. `claim_offered_to_me` is always `false` because a spectator can
never be offered a claim, but `claim_offered_to_position` is public so the stream
can show *which* seat a claim is pending on without revealing any hand data.

### 7. Fan-out and Redis pub/sub

`send_to_spectators()` snapshots the
senders of every spectator connection under a read lock, then delivers outside the
lock. The payload is serialized once and shared via `Arc`, and each recipient gets
a non-blocking `try_send`:

```rust
pub async fn send_to_spectators(&self, game_id: Uuid, message: &str, kind: MessageKind) {
    let targets = {
        let inner = self.inner.read().await;
        inner.connections.get(&game_id)
            .map(|conns| conns.iter().filter(|c| c.spectator)
                .map(|c| (c.id, c.sender.clone())).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    if targets.is_empty() { return; }
    let msg = WsMessage { payload: Arc::from(message), kind };
    let slow = Self::deliver(&msg, &targets);
    self.disconnect_slow_consumers(game_id, slow).await;
}
```

Because `WebSocketManager` state is **per-instance**, a spectator connected to
instance B must still receive events from a game running on instance A. Redis
pub/sub is the bridge. The subscriber shards by game id, and only the owning shard
processes the event — the others drop it, preventing N× duplicate fan-out.

One subtlety: `game_started` is normally **rotated per player** so each player sees
themselves at the bottom. Spectators must receive the **non-rotated** version, so
`manager_redis.rs` sends the original
event to spectators after personalizing it for players:

```rust
// Spectators get the public (non-rotated) game_started.
self.send_to_spectators(game_id, &event.to_json()).await;
```

---

## Frontend Deep Dive

### 1. The route

The stream view is a standalone route registered in
`main.tsx`:

```tsx
<Route path="/game/:gameId/stream" element={<StreamView />} />
```

It is deliberately **outside** the main `App` shell — no navigation, no auth
modal, no dashboard chrome. It is a clean canvas for OBS.

### 2. The stream view component

`StreamView.tsx` reads the token from
the query string and drives a small status machine:

```tsx
type StreamStatus = 'loading' | 'waiting' | 'live';
```

- `loading` — before the first message.
- `waiting` — the game exists but is `pending` or `ready`, or was cancelled.
- `live` — the game is in progress.

It subscribes with `spectator: true`:

```tsx
const { isConnected } = useWebSocket({
  gameId: gameId || '',
  spectator: true,
  wsToken: token,
  onMessage,
});
```

The `onMessage` handler mirrors the player-side event handling but with two
important differences:

- **Players are built with `cards: []` and `is_current_user: false`.** A spectator
  never has a hand, and no player is "the current user".
- **A round-completion animation guard.** While a collection animation is in
  flight, incoming snapshots do not reset the deck or the winner ring, so the
  animation is not torn down mid-flight. A safety-net timer clears the deck just
  past the animation duration.

When live, it renders `GameTable` with
`spectatorMode`:

```tsx
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
```

### 3. The spectator join message

`useWebSocket.ts` includes the flag in the
join payload only when set:

```ts
...(this.playerPosition !== null ? { player_position: this.playerPosition } : {}),
...(this.spectator ? { spectator: true } : {}),
```

The join message is now produced by a single `sendJoinIfOpen()` helper that the
manager calls both on open and whenever the role or identity changes:

```ts
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
```

`setSpectator()` is no longer a passive field write: it no-ops when the value is
unchanged, refuses to flip a connection that already has a player identity, and
re-announces the join so the backend applies the new role on the live socket.
Likewise `setPlayerIdentity()` re-sends `join_game` on every identity change, not
just the first time, and clears the spectator flag — mirroring the backend's
"player wins" invariant.

### 3.1 Connection lifecycle and force reconnection

The manager is a singleton per game (`WebSocketManager.getInstance(gameId)`), so a
player tab and a spectator tab for the same game share one socket. That sharing is
what made the lifecycle subtle. Three mechanisms keep the shared socket honest:

- **Token change forces a reconnect.** `setWsToken()` compares the new token to
  the current one and, on a real change, calls `forceReconnect()` — the token is
  part of the connection URL, so applying it requires a fresh socket:

  ```ts
  setWsToken(token: string | null): void {
    if (this.wsToken === token) {
      return;
    }
    this.wsToken = token;
    // The token is part of the connection URL, so a real change must be applied
    // by reconnecting. ...
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.forceReconnect();
    }
  }
  ```

- **`forceReconnect()` preserves state.** Closing a live socket lets the existing
  `onclose` handler schedule the reconnect (5s) and re-send `join_game` on open,
  so identity, token, spectator flag, and subscribers all survive:

  ```ts
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
  ```

  The hook's `reconnect` callback now calls this method directly; the old
  `cleanupInstance()`/re-`getInstance()` dance was removed because it discarded
  the manager's identity and subscribers.

- **Stable subscription via callback refs.** `onMessage`/`onError`/`onClose` are
  now held in refs and deliberately excluded from the subscription effect's
  dependency array. Before this, a new callback identity on every render tore down
  and rebuilt the subscription each render, re-joining the shared manager over and
  over — which is exactly how a player connection could be mis-attributed as a
  spectator and stop receiving its own hand while the spectator tab stayed correct.

These three changes together make the shared-socket connection idempotent: role,
identity, and token changes converge on a single correct `join_game` per socket,
and a reconnect re-establishes the same state instead of starting anonymous.

### 4. Rendering in spectator mode

`GameTable` accepts a `spectatorMode`
prop that does three things:

```tsx
/** Read-only spectator view: hide all hands and disable interactions. */
spectatorMode?: boolean;
```

- **All hands face-down.** `shouldShowCardsFaceUp()` returns `false` unconditionally
  in spectator mode, so no hand is ever revealed — not even the streamer's.
- **No interactions.** The rules button, mobile top bar, and action controls are
  hidden.
- **No mobile chrome.** The mobile back button and top bar are suppressed, since
  the stream view is not navigable.

<!-- There are dedicated tests for this in
`GameTable.test.tsx`.

There is also a Playwright end-to-end test (`spectator-resync.spec.ts`) that
guards deck-slot stability under the periodic resync: it plays cards in a
non-seat order, then sends a `game_state_snapshot` with the same seat-ordered
`played_cards`, and asserts the deck does not reorder or "jump" — the resync must
be a no-op for an already-live table. -->

### 5. Getting the URL to the streamer

Two places expose the "Copy stream URL" action, both calling the same endpoint and
copying the returned URL to the clipboard:

- `App.tsx` — the in-game view.
- `GameLobby.tsx` — the lobby.

```tsx
const handleCopyStreamUrl = async () => {
  if (!gameId) return
  const res = await axios.post(`/api/games/${gameId}/spectate-token`)
  const url = res.data.url
  const copied = await copyTextToClipboard(url)
  if (copied) {
    showToast(t('game.streamUrlCopied'), 'success')
  } else {
    showToast(url, 'info')   // fall back to showing the raw URL
  }
}
```

The clipboard write goes through `copyTextToClipboard()`
(`utils/clipboard.ts`) instead of a bare
`navigator.clipboard.writeText()`. That matters: the URL is fetched
asynchronously before the copy, so the transient user activation can be gone by
the time `writeText()` runs — and the async Clipboard API is only available in
secure contexts. The helper tries the async API first, then falls back to a hidden
`<textarea>` + `document.execCommand('copy')`, returning `false` when neither
works so the caller can show the URL directly.

---

## Security and Privacy Invariants

The spectator role is safe by construction, not by filtering. The invariants:

- **Spectators never receive `cards_dealt`.** That event is owner-scoped in
  `route_event` and sent only via `send_to_player`. A spectator has no player id,
  so it can never be a recipient.
- **Spectators never receive hand values.** The snapshot carries `cards_count`
  only. There is no code path that puts card values into a spectator payload.
- **Spectators cannot act.** Play, evaluate, and advance are authenticated HTTP
  POSTs keyed to a `player_id`. A spectator has no player identity, so it cannot
  invoke them.
- **Tokens are participant-minted.** Only a game participant can mint a token, and
  only for their own game.
- **Tokens are short-lived and revocable.** 6-hour TTL, Redis-backed, revocable by
  deleting the key.
- **Tokens are purpose-scoped.** The `ws:spectate` purpose prevents a spectator
  token from being used as a game token.

---

## Current Limitations

This is the honest part. The spectator feature works, but it has known limits.
A couple of limits from when this post was first written have since been fixed:
the per-connection send queue is now **bounded** (256 messages) with non-blocking
`try_send` and message classification, and fan-out now serializes once and shares
the payload via `Arc`. The earlier unbounded-queue OOM risk and the per-spectator
`to_string()` allocation are gone. What remains:

### 1. Redis pub/sub is fire-and-forget (mitigated by resync)

There is no per-subscriber acknowledgment and no replay. If a backend instance is
briefly disconnected, events published during the gap are **lost**. A periodic
resync task now re-sends the full public snapshot (every 10 seconds by default)
to every game with active spectators, so a missed intermediate event is
self-healed by the next snapshot. The gap still exists — it is just bounded in
time by the resync interval.

### 2. Fan-out cost is still linear in viewers

Fan-out now shares a single `Arc` allocation, but delivery cost is still linear in
viewer count: every spectator connection gets a `try_send` per event. The bounded
queue means a slow viewer can no longer take the instance down, but CPU and
latency still grow linearly. For large viewer counts the ceiling is reached by
fan-out cost, not game logic.

### 3. Public-only snapshot still omits per-player special-card flags

The spectator snapshot sets `special_cards: None` and `claim_offered_to_me: false`.
The claim *position* is now included (`claim_offered_to_position`), so the stream
view can show who a claim is being offered to. But the per-player special-card
flags themselves remain private, so any overlay that wants to visualize the
streamer's own special-card state still needs a new public-only representation.

<!-- ### 4. Refactor TODOs in the snapshot builder

`send_spectator_snapshot()` duplicates
logic that also exists in the player snapshot path. Two `TODO` comments flag this:

```rust
// TODO: refactor  game_state_players by having a function that returns the GameStatePlayer list
// TODO: refactor by creating outside function that return slots based on played_cards
```

This duplication is a maintenance risk: a change to the player snapshot could
silently diverge from the spectator snapshot. -->

---

## Debugging the Stream View

If you open the stream view and do not see WebSocket frames in the browser's
Network tab, that is usually expected behavior, not a bug:

- **Open the frame inspector before the connection.** DevTools only records frames
  for a WebSocket while the "Messages" panel is open. If you open it after the
  initial snapshot has been delivered, you will see an empty list.
- **The payload is intentionally minimal.** A spectator receives
  `game_state_snapshot` and public events only. There is no `cards_dealt`, no hand
  data, and no per-player rotation. A quiet stream between turns is normal.
- **The initial snapshot is sent on join.** The `join_game` message with
  `spectator: true` triggers `send_spectator_snapshot()` immediately, so the first
  frame arrives right after the join, not on the next game event.
- **Check the token.** If the token is missing, expired, or not in Redis, the
  handshake fails with `401 Unauthorized` and no WebSocket is established at all.
  The backend logs `Spectator token not found in Redis` in that case.

To observe the raw frames, open DevTools → Network → WS → select the game socket →
Messages, **then** reload the stream view.

---

## Observability and Operations

The spectator feature is instrumented with a per-game spectator gauge and two
hot-spot alerts:

- **`ws_spectators_per_game` gauge** — a per-game spectator count, set on join and
  disconnect, with the series removed at zero to bound cardinality.
- **Warning alert at 700 spectators** — a single game is becoming a hot spot.
- **Critical alert at 1,500 spectators** — a single game can threaten the instance.
- **Companion metrics** — `ws_messages_dropped_total`,
  `ws_slow_consumer_disconnects_total`, and `ws_send_queue_depth` are now active:
  a full queue drops a `Snapshot` or disconnects a stalled client on a `Control`
  message.

The intended narrative: **700 spectators is the warning that a game is a hot spot;
a non-zero dropped-message rate is the proof that the hot spot is degrading.**

The runbook when the 700 alert fires:

1. Confirm the concentration with `topk(10, ws_spectators_per_game)`.
2. Check for degradation with the dropped-message and slow-consumer rates.
3. If queues are healthy, monitor — no action required.
4. If queues are filling, check the drop and slow-consumer metrics — a bounded
   queue should be shedding `Snapshot` traffic or disconnecting the slow viewer,
   not growing without bound.
5. If the game is a persistent hot spot, consider edge aggregation or a dedicated
   broadcast channel.

---

## Roadmap

The single most impactful change for streaming scale is to **stop treating each
viewer as a first-class WebSocket connection**. The current model is now safe
under a stalled viewer (bounded queues + backpressure), but it is still O(N) in
fan-out; an edge-aggregation or dedicated broadcast channel — or snapshot
coalescing — is what would make it scalable to thousands of viewers.

---

## Conclusion

Spectator mode is a small feature with a large design surface. By reusing the
existing WebSocket transport and adding a read-only role — rather than a headless
browser relay — we got streamer hand privacy for the cost of one boolean, one
snapshot builder, and one token type.

The privacy guarantees are structural: spectators never receive `cards_dealt`, never
receive hand values, and cannot act. The remaining limitations are equally
structural: fan-out is still O(N) in viewers. That is the next problem to solve.

---

## Related Reading

- `observability-grafana-lgtm-vs-sentry-signoz.md`
  — the observability stack this feature plugs into.
