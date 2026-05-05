import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import App from './App';

describe('App', () => {
  it('renders the dashboard heading', () => {
    render(<App />);
    const heading = screen.getByRole('heading', { name: /FapFap Card Game/i });
    expect(heading).toBeInTheDocument();
  });

  it('shows loading state initially', () => {
    render(<App />);
    const loading = screen.getByText(/Loading/i);
    expect(loading).toBeInTheDocument();
  });
});
