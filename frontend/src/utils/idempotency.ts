/**
 * Generates a stable-per-action idempotency key for state-changing requests.
 *
 * The key is generated once per logical action and must be reused across
 * retries of that same action so the backend (`X-Idempotency-Key`) can
 * deduplicate in-flight requests. A fresh key is produced for each new action.
 */
export function generateIdempotencyKey(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
