import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('axios', () => ({
  default: {
    get: vi.fn(),
    post: vi.fn(),
  },
}));

import axios from 'axios';
import i18n from '../i18n/config';
import { useLanguageStore } from './useLanguageStore';

const mockedAxios = axios as unknown as {
  get: ReturnType<typeof vi.fn>;
  post: ReturnType<typeof vi.fn>;
};

describe('useLanguageStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLanguageStore.setState({ language: 'en', availableLanguages: [], loaded: false });
    i18n.changeLanguage('en');
  });

  it('setLanguage switches language and posts to the backend', async () => {
    mockedAxios.post.mockResolvedValue({ data: { success: true } });

    await useLanguageStore.getState().setLanguage('fr');

    expect(useLanguageStore.getState().language).toBe('fr');
    expect(i18n.language).toBe('fr');
    expect(mockedAxios.post).toHaveBeenCalledWith('/api/lang', { lang: 'fr' });
  });

  it('setLanguage reverts to the previous language when the backend rejects', async () => {
    mockedAxios.post.mockRejectedValue(new Error('network error'));

    await useLanguageStore.getState().setLanguage('fr');

    expect(useLanguageStore.getState().language).toBe('en');
    expect(i18n.language).toBe('en');
  });

  it('init loads available languages and applies the backend language', async () => {
    mockedAxios.get.mockResolvedValue({
      data: {
        current: 'fr',
        languages: [
          { code: 'en', label: 'English' },
          { code: 'fr', label: 'Français' },
        ],
      },
    });

    await useLanguageStore.getState().init();

    expect(useLanguageStore.getState().language).toBe('fr');
    expect(useLanguageStore.getState().availableLanguages).toHaveLength(2);
    expect(useLanguageStore.getState().loaded).toBe(true);
  });

  it('init falls back to defaults when the backend is unreachable', async () => {
    mockedAxios.get.mockRejectedValue(new Error('network error'));

    await useLanguageStore.getState().init();

    expect(useLanguageStore.getState().loaded).toBe(true);
    expect(useLanguageStore.getState().availableLanguages).toHaveLength(2);
  });

  it('syncFromUser switches language when it differs', () => {
    useLanguageStore.getState().syncFromUser('fr');

    expect(useLanguageStore.getState().language).toBe('fr');
    expect(i18n.language).toBe('fr');
  });

  it('syncFromUser keeps the current language when it matches', () => {
    useLanguageStore.getState().syncFromUser('en');

    expect(useLanguageStore.getState().language).toBe('en');
  });
});
