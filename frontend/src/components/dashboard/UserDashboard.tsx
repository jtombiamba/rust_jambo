import { useEffect, useRef, useState } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '../../stores/useAuthStore'
import { useInvitationStore, InvitationItem } from '../../stores/useInvitationStore'
import GameRules from '../GameRules'
import RoomList from '../RoomList'
import { useDashboardData } from '../../hooks/useDashboardData'
import { useTopUp } from '../../hooks/useTopUp'
import { useCashout } from '../../hooks/useCashout'
import { useMultiplayer } from '../../hooks/useMultiplayer'
import DashboardTopBar from './DashboardTopBar'
import DashboardSidebar from './DashboardSidebar'
import HeroPlayHub from './HeroPlayHub'
import StatsOverview from './StatsOverview'
import LeaderboardSnippet from './LeaderboardSnippet'
import GameHistory from './GameHistory'
import InvitationsPanel from './InvitationsPanel'
import CashoutHistory from './CashoutHistory'
import TopUpModal from './TopUpModal'
import MultiplayerModal from './MultiplayerModal'
import CashoutModal from './CashoutModal'
import AccountSettingsModal from './AccountSettingsModal'
import type { QuickGameResponse } from './types'

interface Props {
  onStartGame: (stepByStep?: boolean) => void
  onStartMultiplayerGame: (
    bet: number,
    maxPlayers: number,
  ) => Promise<{ gameId: string; error: string | null }>
  onResumeGame: (data: QuickGameResponse) => void
  onViewLobby: (gameId: string) => void
  onOpenRoom: (roomId: string) => void
  onCreateRoom: () => void
  onJoinRoom: () => void
  starting: boolean
  error: string | null
  stepByStep: boolean
  onStepByStepChange: (value: boolean) => void
}

export default function UserDashboard({
  onStartGame,
  onStartMultiplayerGame,
  onResumeGame,
  onViewLobby,
  onOpenRoom,
  onCreateRoom,
  onJoinRoom,
  starting,
  error,
  stepByStep,
  onStepByStepChange,
}: Props) {
  const { user, logout } = useAuthStore()
  const { t } = useTranslation()

  const [toast, setToast] = useState<string | null>(null)
  const [rulesOpen, setRulesOpen] = useState(false)
  const [sidebarOpen, setSidebarOpen] = useState(false)
  const [topUpOpen, setTopUpOpen] = useState(false)
  const [accountSettingsOpen, setAccountSettingsOpen] = useState(false)
  const toastTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const showToast = (msg: string) => {
    if (toastTimer.current) clearTimeout(toastTimer.current)
    setToast(msg)
    toastTimer.current = setTimeout(() => setToast(null), 3000)
  }

  useEffect(() => {
    return () => {
      if (toastTimer.current) clearTimeout(toastTimer.current)
    }
  }, [])

  const {
    profile,
    history,
    page,
    perPage,
    loading,
    freezeRemainingSec,
    statusFilter,
    totalPages,
    fetchData,
    handleSort,
    sortIndicator,
    setStatusFilter,
    setPage,
    setPerPage,
    handleGameClick,
  } = useDashboardData(onResumeGame, showToast)

  const { toppingUp, handleTopUp, unfreezing, handleUnfreeze } = useTopUp(fetchData, showToast)
  const cashout = useCashout(fetchData, showToast)
  const multiplayer = useMultiplayer(profile, onStartMultiplayerGame, onViewLobby, showToast)

  // Seed invitations once from the server, then let the user WebSocket
  // (useUserWebSocket) own subsequent add/remove.
  useEffect(() => {
    axios
      .get<{ invitations: InvitationItem[] }>('/api/me/invitations')
      .then((res) => {
        useInvitationStore.getState().seedInvitations(res.data.invitations)
      })
      .catch((err) => {
        console.error('Failed to load invitations', err)
      })
  }, [])

  const cashoutAvailable = Boolean(cashout.cashoutConfig?.enabled && !profile?.cashout_locked)

  if (loading && !history) {
    return (
      <div className="container mx-auto p-4 sm:p-8">
        <p className="text-gray-600">{t('dashboard.loadingDashboard')}</p>
      </div>
    )
  }

  const pseudo = user?.pseudo ?? ''
  const email = user?.email ?? ''

  return (
    <div className="min-h-screen bg-gray-50">
      <GameRules isOpen={rulesOpen} onClose={() => setRulesOpen(false)} />

      <DashboardTopBar
        pseudo={pseudo}
        email={email}
        credit={profile?.credit ?? 0}
        toppingUp={toppingUp}
        cashoutAvailable={cashoutAvailable}
        showCashoutHistory={cashout.showCashoutHistory}
        onOpenTopUp={() => setTopUpOpen(true)}
        onLogout={logout}
        onOpenRules={() => setRulesOpen(true)}
        onOpenCashout={cashout.openCashoutModal}
        onOpenAccountSettings={() => setAccountSettingsOpen(true)}
        onToggleCashoutHistory={cashout.toggleCashoutHistory}
        onOpenSidebar={() => setSidebarOpen(true)}
      />

      <div className="flex">
        <DashboardSidebar open={sidebarOpen} onClose={() => setSidebarOpen(false)} />

        <main className="flex-1 p-4 sm:p-6">
          <section id="dashboard" className="mb-6 scroll-mt-20">
            <h1 className="text-2xl sm:text-3xl font-bold mb-1 text-gray-900">
              {t('dashboard.welcome', { pseudo })}
            </h1>
            <p className="text-gray-500 mb-4">{email}</p>

            {profile?.cashout_locked && (
              <div className="mb-4 bg-yellow-50 border border-yellow-300 rounded-lg p-4">
                <p className="text-yellow-800 font-semibold">{t('dashboard.cashoutLockedNotice')}</p>
              </div>
            )}

            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className="md:col-span-2">
                <StatsOverview profile={profile} toppingUp={toppingUp} onOpenTopUp={() => setTopUpOpen(true)} />
              </div>
              <div id="leaderboard" className="scroll-mt-20">
                <LeaderboardSnippet />
              </div>
            </div>
          </section>

          <section id="play" className="mb-6 scroll-mt-20">
            <HeroPlayHub
              frozen={freezeRemainingSec > 0}
              freezeRemainingSec={freezeRemainingSec}
              unfreezing={unfreezing}
              starting={starting}
              stepByStep={stepByStep}
              onStepByStepChange={onStepByStepChange}
              onUnfreeze={handleUnfreeze}
              onStartSolo={() => onStartGame(stepByStep)}
              onOpenMultiplayer={multiplayer.openMultiplayerModal}
            />
          </section>

          <section id="rooms" className="mb-6 scroll-mt-20">
            <RoomList onSelectRoom={onOpenRoom} onCreateRoom={onCreateRoom} onJoinRoom={onJoinRoom} />
          </section>

          <InvitationsPanel
            freezeRemainingSec={freezeRemainingSec}
            onViewLobby={onViewLobby}
            showToast={showToast}
          />

          {error && (
            <div className="mb-6 p-3 bg-red-100 text-red-700 rounded text-sm">
              {t('dashboard.failedStartGame')}: {error}
            </div>
          )}

          <section id="history" className="mb-6 scroll-mt-20">
            <GameHistory
              history={history}
              page={page}
              perPage={perPage}
              statusFilter={statusFilter}
              totalPages={totalPages}
              onStatusFilter={setStatusFilter}
              onSort={handleSort}
              onPage={setPage}
              onPerPage={setPerPage}
              onGameClick={handleGameClick}
              sortIndicator={sortIndicator}
            />
          </section>

          {cashout.showCashoutHistory && (
            <CashoutHistory
              cashoutHistory={cashout.cashoutHistory}
              loading={cashout.cashoutHistoryLoading}
            />
          )}
        </main>
      </div>

      {toast && (
        <div className="fixed bottom-8 left-1/2 -translate-x-1/2 z-50 px-6 py-3 bg-gray-800 text-white text-sm rounded-lg shadow-lg animate-fade-in-out">
          {toast}
        </div>
      )}

      <TopUpModal
        open={topUpOpen}
        toppingUp={toppingUp}
        onClose={() => setTopUpOpen(false)}
        onConfirm={(amount) => {
          setTopUpOpen(false)
          handleTopUp(amount)
        }}
      />

      <AccountSettingsModal
        open={accountSettingsOpen}
        onClose={() => setAccountSettingsOpen(false)}
        showToast={showToast}
      />

      <MultiplayerModal
        open={multiplayer.multiplayerOpen}
        step={multiplayer.multiplayerStep}
        bet={multiplayer.multiplayerBet}
        maxPlayers={multiplayer.multiplayerMaxPlayers}
        creating={multiplayer.multiplayerCreating}
        error={multiplayer.multiplayerError}
        pseudos={multiplayer.multiplayerPseudos}
        credit={profile?.credit ?? 0}
        onClose={multiplayer.closeMultiplayerModal}
        onStepChange={multiplayer.setMultiplayerStep}
        onBetChange={multiplayer.setMultiplayerBet}
        onMaxPlayersChange={multiplayer.setMultiplayerMaxPlayers}
        onPseudoChange={(slot, value) =>
          multiplayer.setMultiplayerPseudos((prev) => ({ ...prev, [slot]: value }))
        }
        onSubmit={multiplayer.handleStep1Submit}
      />

      {cashout.cashoutConfig && (
        <CashoutModal
          open={cashout.cashoutOpen}
          config={cashout.cashoutConfig}
          credits={cashout.cashoutCredits}
          method={cashout.cashoutMethod}
          destination={cashout.cashoutDestination}
          connecting={cashout.cashoutConnecting}
          submitting={cashout.cashoutSubmitting}
          error={cashout.cashoutError}
          onClose={cashout.closeCashoutModal}
          onCreditsChange={cashout.setCashoutCredits}
          onMethodChange={cashout.setCashoutMethod}
          onDestinationChange={cashout.setCashoutDestination}
          onConnectBank={cashout.handleConnectBank}
          onSubmit={cashout.handleCashoutSubmit}
        />
      )}
    </div>
  )
}
