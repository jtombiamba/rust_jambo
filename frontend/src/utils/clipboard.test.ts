import { describe, it, expect, vi, afterEach } from 'vitest';
import { copyTextToClipboard } from './clipboard';

describe('copyTextToClipboard', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('uses navigator.clipboard.writeText when available', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });

    const ok = await copyTextToClipboard('hello');

    expect(writeText).toHaveBeenCalledWith('hello');
    expect(ok).toBe(true);
  });

  it('falls back to execCommand copy when clipboard.writeText rejects', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('denied'));
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });

    const execCommand = vi.fn().mockReturnValue(true);
    document.execCommand = execCommand;

    const ok = await copyTextToClipboard('fallback');

    expect(execCommand).toHaveBeenCalledWith('copy');
    expect(ok).toBe(true);
    // The hidden textarea must be cleaned up.
    expect(document.querySelector('textarea')).toBeNull();
  });

  it('falls back to execCommand copy when navigator.clipboard is undefined', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: undefined,
    });

    const execCommand = vi.fn().mockReturnValue(true);
    document.execCommand = execCommand;

    const ok = await copyTextToClipboard('legacy');

    expect(execCommand).toHaveBeenCalledWith('copy');
    expect(ok).toBe(true);
  });

  it('returns false when every strategy fails', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: undefined,
    });
    document.execCommand = vi.fn().mockReturnValue(false);

    const ok = await copyTextToClipboard('nope');

    expect(ok).toBe(false);
  });
});
