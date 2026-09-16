import { describe, it, expect, beforeEach } from 'vitest';
import { useInvitationStore, InvitationItem } from './useInvitationStore';

const makeInvitation = (overrides: Partial<InvitationItem> = {}): InvitationItem => ({
  invite_id: 'inv-1',
  game_id: 'game-1',
  creator_pseudo: 'alice',
  bet: 10,
  player_count: 1,
  max_players: 4,
  created_at: '2026-01-01T00:00:00Z',
  expires_at: null,
  ...overrides,
});

describe('useInvitationStore', () => {
  beforeEach(() => {
    useInvitationStore.getState().clear();
  });

  it('starts empty', () => {
    expect(useInvitationStore.getState().invitations).toEqual([]);
  });

  it('setInvitations replaces the full list', () => {
    useInvitationStore.getState().setInvitations([makeInvitation(), makeInvitation({ invite_id: 'inv-2', game_id: 'game-2' })]);
    expect(useInvitationStore.getState().invitations).toHaveLength(2);
  });

  it('addInvitation appends a new invitation', () => {
    useInvitationStore.getState().addInvitation(makeInvitation());
    useInvitationStore.getState().addInvitation(makeInvitation({ invite_id: 'inv-2', game_id: 'game-2' }));
    expect(useInvitationStore.getState().invitations).toHaveLength(2);
  });

  it('addInvitation deduplicates by invite_id', () => {
    const inv = makeInvitation();
    useInvitationStore.getState().addInvitation(inv);
    useInvitationStore.getState().addInvitation(inv);
    expect(useInvitationStore.getState().invitations).toHaveLength(1);
  });

  it('removeInvitation removes by game_id', () => {
    useInvitationStore.getState().setInvitations([
      makeInvitation({ game_id: 'game-1' }),
      makeInvitation({ invite_id: 'inv-2', game_id: 'game-2' }),
    ]);
    useInvitationStore.getState().removeInvitation('game-1');
    const remaining = useInvitationStore.getState().invitations;
    expect(remaining).toHaveLength(1);
    expect(remaining[0].game_id).toBe('game-2');
  });

  it('clear empties the list', () => {
    useInvitationStore.getState().addInvitation(makeInvitation());
    useInvitationStore.getState().clear();
    expect(useInvitationStore.getState().invitations).toEqual([]);
  });
});
