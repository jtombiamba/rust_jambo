import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import AccountSettingsModal from './AccountSettingsModal';

const mockPost = vi.fn();

vi.mock('axios', () => ({
  default: {
    get: vi.fn(),
    post: (...args: unknown[]) => mockPost(...args),
    defaults: {},
    interceptors: {
      request: { use: vi.fn() },
      response: { use: vi.fn() },
    },
  },
}));

vi.mock('../../stores/useAuthStore', () => ({
  useAuthStore: vi.fn((selector) => selector({ setUserEmail: vi.fn() })),
}));

const baseProps = {
  open: true,
  onClose: vi.fn(),
  showToast: vi.fn(),
};

describe('AccountSettingsModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockPost.mockResolvedValue({ data: { success: true, user: { email: 'new@example.com' } } });
  });

  it('renders the language, email and password sections', () => {
    render(<AccountSettingsModal {...baseProps} />);

    expect(screen.getByText('Account Settings')).toBeInTheDocument();
    expect(screen.getByText('Language')).toBeInTheDocument();
    expect(screen.getByText('Change Email')).toBeInTheDocument();
    expect(screen.getByText('Change Password')).toBeInTheDocument();
  });

  it('submits a change email request with the right payload', async () => {
    const user = userEvent.setup();
    render(<AccountSettingsModal {...baseProps} />);

    await user.type(screen.getAllByLabelText('New Email')[0], 'new@example.com');
    await user.type(screen.getAllByLabelText('Current Password')[0], 'password1');
    await user.click(screen.getAllByRole('button', { name: 'Save' })[0]);

    expect(mockPost).toHaveBeenCalledWith('/api/auth/change-email', {
      new_email: 'new@example.com',
      current_password: 'password1',
    });
  });

  it('submits a change password request with the right payload', async () => {
    const user = userEvent.setup();
    render(<AccountSettingsModal {...baseProps} />);

    await user.type(screen.getAllByLabelText('Current Password')[1], 'password1');
    await user.type(screen.getByLabelText('New Password'), 'newpassword2');
    await user.type(screen.getByLabelText('Confirm New Password'), 'newpassword2');
    await user.click(screen.getAllByRole('button', { name: 'Save' })[1]);

    expect(mockPost).toHaveBeenCalledWith('/api/auth/change-password', {
      current_password: 'password1',
      new_password: 'newpassword2',
      new_password_confirm: 'newpassword2',
    });
  });
});
