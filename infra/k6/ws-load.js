// ws-load.js — k6 WebSocket load test for the Jambo real-time game endpoint.
//
// Models the spectator side of the Rust `ws-load-test` binary: setup registers
// four players, creates one game via the benchmark API, and mints a single
// spectate token (mirroring the "copy stream URL" flow). Every VU then connects
// to that game as a spectator and measures connect + first-event latency.
//
// Run (backend must be running with BENCHMARK_MODE=true):
//   docker run --rm --network host \
//     -v "$PWD/infra/k6:/scripts" grafana/k6 run /scripts/ws-load.js
//
// Tune with:
//   TARGET_URL         default http://localhost:5000
//   BENCHMARK_TOKEN    default aiusutzfzdv6529jhz
//   SPECTATORS         VU count (default 50) via env SPECTATORS

import http from 'k6/http';
import ws from 'k6/ws';
import { check } from 'k6';
import { Trend, Counter } from 'k6/metrics';

const TARGET = __ENV.TARGET_URL || 'http://localhost:5000';
const TOKEN = __ENV.BENCHMARK_TOKEN || 'aiusutzfzdv6529jhz';
const WS_TARGET = TARGET.replace(/^http/, 'ws');

const connectLatency = new Trend('ws_connect_latency', true);
const firstEventLatency = new Trend('ws_first_event_latency', true);
const connectFailures = new Counter('ws_connect_failures');

export const options = {
  vus: Number(__ENV.SPECTATORS || 50),
  duration: '30s',
  thresholds: {
    ws_connect_latency: ['p(95)<2000'],
    ws_first_event_latency: ['p(95)<100'],
    ws_connect_failures: ['count<5'],
  },
};

function registerUser(i, runId) {
  const res = http.post(
    `${TARGET}/api/auth/register`,
    JSON.stringify({
      pseudo: `k6ws_${runId}_${i}`,
      email: `k6ws_${runId}_${i}@benchmark.local`,
      password: 'benchmark123',
      password_confirm: 'benchmark123',
    }),
    { headers: { 'Content-Type': 'application/json' } }
  );
  if (res.status !== 201) throw new Error(`register failed: ${res.status}`);
  return { id: res.json('user.id'), cookie: res.headers['Set-Cookie'] || '' };
}

export function setup() {
  const runId = `${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
  const users = [0, 1, 2, 3].map((i) => registerUser(i, runId));

  const create = http.post(
    `${TARGET}/api/benchmark/create-multiplayer-game`,
    JSON.stringify({ user_ids: users.map((u) => u.id), bet: 10 }),
    { headers: { 'Content-Type': 'application/json', 'X-Benchmark-Token': TOKEN } }
  );
  if (create.status !== 201) throw new Error(`create failed: ${create.status}`);
  const gameId = create.json('game_id');

  const mint = http.post(
    `${TARGET}/api/games/${gameId}/spectate-token`,
    null,
    { headers: { Cookie: users[0].cookie } }
  );
  if (mint.status !== 200) throw new Error(`mint failed: ${mint.status}`);
  const token = mint.json('token');

  return { gameId, token };
}

export default function (data) {
  const url = `${WS_TARGET}/ws/${data.gameId}?token=${data.token}`;
  const startT0 = Date.now();

  const res = ws.connect(url, {}, (socket) => {
    let openedAt = 0;
    let gotFirst = false;

    socket.on('open', () => {
      openedAt = Date.now();
      connectLatency.add(openedAt - startT0);
      // Mark the connection as a spectator so the backend emits a snapshot.
      socket.send(JSON.stringify({ type: 'join_game', game_id: data.gameId, spectator: true }));
    });

    socket.on('message', () => {
      if (!gotFirst) {
        firstEventLatency.add(Date.now() - openedAt);
        gotFirst = true;
        socket.close();
      }
    });

    socket.on('error', (e) => {
      connectFailures.add(1);
      console.error('ws error', e);
    });

    socket.setTimeout(() => socket.close(), 5000);
  });

  if (!res || res.status !== 101) {
    connectFailures.add(1);
    check(res, { 'ws connected (101)': (r) => r && r.status === 101 });
  }
}
