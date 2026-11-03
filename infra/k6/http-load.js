// http-load.js — k6 HTTP load test for the Jambo benchmark API.
//
// Models the same slice of traffic as the Rust `http-load-test` binary:
// register users, create a 4-player multiplayer game via the benchmark
// endpoint, then play a card. The difference (on purpose): this script plays
// ONE card per game, while the Rust harness plays a full game using the real
// `pick_playable_cards_in_round` strategy. See load-testing-rust-vs-k6.md.
//
// Run (backend must be running with BENCHMARK_MODE=true):
//   docker run --rm --network host \
//     -v "$PWD/infra/k6:/scripts" grafana/k6 run /scripts/http-load.js
//
// Tune with:
//   TARGET_URL         default http://localhost:5000
//   BENCHMARK_TOKEN    default aiusutzfzdv6529jhz
//   BATCH_USERS        users pre-registered in setup (default 400 = 100 games x 4)

import http from 'k6/http';
import { check, group } from 'k6';
import { Trend, Counter } from 'k6/metrics';

const TARGET = __ENV.TARGET_URL || 'http://localhost:5000';
const TOKEN = __ENV.BENCHMARK_TOKEN || 'aiusutzfzdv6529jhz';
const BATCH = Number(__ENV.BATCH_USERS || 400);

const regLatency = new Trend('register_latency', true);
const createLatency = new Trend('game_create_latency', true);
const playLatency = new Trend('card_play_latency', true);
const playFailures = new Counter('card_play_failures');

export const options = {
  scenarios: {
    gameplay: {
      executor: 'ramping-vus',
      startVUs: 0,
      stages: [
        { duration: '10s', target: 20 }, // ramp-up
        { duration: '40s', target: 20 }, // steady state
        { duration: '10s', target: 0 }, // ramp-down
      ],
    },
  },
  thresholds: {
    game_create_latency: ['p(95)<500'],
    card_play_latency: ['p(95)<500'],
    http_req_failed: ['rate<0.01'],
  },
};

function registerUser(i, runId) {
  const payload = JSON.stringify({
    pseudo: `k6_${runId}_${i}`,
    email: `k6_${runId}_${i}@benchmark.local`,
    password: 'benchmark123',
    password_confirm: 'benchmark123',
  });
  const t0 = Date.now();
  const res = http.post(`${TARGET}/api/auth/register`, payload, {
    headers: { 'Content-Type': 'application/json' },
  });
  regLatency.add(Date.now() - t0);
  const ok = check(res, { 'register 201': (r) => r.status === 201 });
  if (!ok) return null;
  return {
    id: res.json('user.id'),
    cookie: res.headers['Set-Cookie'] || '',
  };
}

export function setup() {
  const runId = `${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
  const users = [];
  for (let i = 0; i < BATCH; i++) {
    const u = registerUser(i, runId);
    if (u) users.push(u);
  }
  return { users };
}

export default function (data) {
  const users = data.users;
  if (users.length < 4) return;

  // Each VU grabs a distinct 4-user slice; slice index advances per iteration
  // so concurrent games use different users (approximates the harness).
  const numGames = Math.floor(users.length / 4);
  const game = (__VU - 1 + __ITER) % numGames;
  const idx = game * 4;
  const four = [users[idx], users[idx + 1], users[idx + 2], users[idx + 3]];
  const cookieByUserId = {};
  for (const u of four) cookieByUserId[u.id] = u.cookie;

  let gameId = null;
  let players = [];
  let currentTurn = 0;

  group('create_game', () => {
    const t0 = Date.now();
    const res = http.post(
      `${TARGET}/api/benchmark/create-multiplayer-game`,
      JSON.stringify({ user_ids: four.map((u) => u.id), bet: 10 }),
      { headers: { 'Content-Type': 'application/json', 'X-Benchmark-Token': TOKEN } }
    );
    createLatency.add(Date.now() - t0);
    const ok = check(res, { 'create game 201': (r) => r.status === 201 });
    if (!ok) return;
    gameId = res.json('game_id');
    players = res.json('players') || [];
    currentTurn = res.json('current_turn_position') || 0;
  });

  if (!gameId) return;

  group('play_card', () => {
    const player = players.find((p) => p.position === currentTurn) || players[0];
    if (!player || !player.cards || player.cards.length === 0) return;
    const cardIndex = player.cards[0];
    const cookie = cookieByUserId[player.user_id] || '';
    const t0 = Date.now();
    const res = http.post(
      `${TARGET}/api/games/${gameId}/play`,
      JSON.stringify({ player_id: player.player_id, card_index: cardIndex }),
      { headers: { 'Content-Type': 'application/json', Cookie: cookie } }
    );
    playLatency.add(Date.now() - t0);
    const ok = check(res, { 'play 200': (r) => r.status === 200 });
    if (!ok) playFailures.add(1);
  });
}
