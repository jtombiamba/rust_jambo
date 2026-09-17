import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import MobileTopBar from './MobileTopBar';

describe('MobileTopBar', () => {
  it('renders a back arrow and a rules button', () => {
    render(<MobileTopBar onBack={() => {}} onOpenRules={() => {}} />);
    expect(screen.getByLabelText('Back to Dashboard')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Rules' })).toBeInTheDocument();
  });

  it('calls onBack when the back arrow is clicked', () => {
    const onBack = vi.fn();
    render(<MobileTopBar onBack={onBack} onOpenRules={() => {}} />);
    fireEvent.click(screen.getByLabelText('Back to Dashboard'));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it('calls onOpenRules when the rules button is clicked', () => {
    const onOpenRules = vi.fn();
    render(<MobileTopBar onBack={() => {}} onOpenRules={onOpenRules} />);
    fireEvent.click(screen.getByRole('button', { name: 'Rules' }));
    expect(onOpenRules).toHaveBeenCalledTimes(1);
  });
});
