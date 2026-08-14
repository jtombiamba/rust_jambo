import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import App from './App';

vi.mock('./stores/useAuthStore', () => {
  const state = {
    isAuthenticated: false,
    user: null,
    authModalOpen: false,
    authView: 'choice',
    authError: null,
    authLoading: false,
    openAuthModal: vi.fn(),
    closeAuthModal: vi.fn(),
    setAuthView: vi.fn(),
    checkAuth: vi.fn(),
    register: vi.fn(),
    login: vi.fn(),
    forgotPassword: vi.fn(),
    logout: vi.fn(),
  };
  const useAuthStore = Object.assign(vi.fn(() => state), {
    getState: () => state,
  });
  return {
    useAuthStore,
    __setAuthenticated: (value: boolean) => {
      state.isAuthenticated = value;
    },
  };
});

vi.mock('axios', () => ({
  default: {
    get: vi.fn().mockResolvedValue({
      data: {
        games_allowed: 10,
        games_played: 0,
        total_wins: 0,
        credits: 500,
      },
    }),
    post: vi.fn(),
  },
}));

describe('App', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('shows loading state initially', () => {
    render(<App />);
    const loading = screen.getByText(/Loading/i);
    expect(loading).toBeInTheDocument();
  });

  it('renders the dashboard heading after loading', async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText(/FapFap Card Game/i)).toBeInTheDocument();
    });
  });

  it('renders the Rules button', async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText('Rules')).toBeInTheDocument();
    });
  });

  it('opens Rules modal when clicking Rules button', async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText('Rules')).toBeInTheDocument();
    });
    screen.getByText('Rules').click();
    await waitFor(() => {
      expect(screen.getByText('How to Play Jambo')).toBeInTheDocument();
    });
  });
});
