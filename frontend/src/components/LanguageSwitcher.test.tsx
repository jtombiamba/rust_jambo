import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import LanguageSwitcher from './LanguageSwitcher';
import { useLanguageStore } from '../stores/useLanguageStore';

vi.mock('axios', () => ({
  default: {
    get: vi.fn(),
    post: vi.fn().mockResolvedValue({ data: { success: true } }),
  },
}));

describe('LanguageSwitcher', () => {
  beforeEach(() => {
    useLanguageStore.setState({
      language: 'en',
      availableLanguages: [
        { code: 'en', label: 'English' },
        { code: 'fr', label: 'Français' },
      ],
      loaded: true,
    });
  });

  it('renders a button for each available language', () => {
    render(<LanguageSwitcher />);
    expect(screen.getByText('EN')).toBeInTheDocument();
    expect(screen.getByText('FR')).toBeInTheDocument();
  });

  it('switches the language when a button is clicked', async () => {
    const user = userEvent.setup();
    render(<LanguageSwitcher />);

    await user.click(screen.getByText('FR'));

    expect(useLanguageStore.getState().language).toBe('fr');
  });

  it('renders nothing when fewer than two languages are available', () => {
    useLanguageStore.setState({
      language: 'en',
      availableLanguages: [{ code: 'en', label: 'English' }],
      loaded: true,
    });

    const { container } = render(<LanguageSwitcher />);
    expect(container).toBeEmptyDOMElement();
  });
});
