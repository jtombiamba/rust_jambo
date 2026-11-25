import { useTranslation } from 'react-i18next'

interface SidebarProps {
  open: boolean
  onClose: () => void
}

const NAV_ITEMS = [
  { id: 'dashboard', key: 'nav.dashboard' },
  { id: 'play', key: 'nav.play' },
  { id: 'rooms', key: 'nav.rooms' },
  { id: 'leaderboard', key: 'nav.leaderboard' },
  { id: 'history', key: 'nav.history' },
] as const

export default function DashboardSidebar({ open, onClose }: SidebarProps) {
  const { t } = useTranslation()

  const scrollTo = (id: string) => {
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    onClose()
  }

  return (
    <>
      {open && (
        <div
          className="fixed inset-0 z-40 bg-black/40 lg:hidden"
          onClick={onClose}
          aria-hidden="true"
        />
      )}
      <aside
        className={`fixed lg:static inset-y-0 left-0 z-40 top-16 lg:top-auto w-56 shrink-0 bg-white border-r border-gray-200 p-3 transition-transform lg:translate-x-0 ${
          open ? 'translate-x-0' : '-translate-x-full'
        }`}
      >
        <nav className="space-y-1">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.id}
              onClick={() => scrollTo(item.id)}
              className="w-full text-left px-3 py-2 rounded-lg text-sm font-medium text-gray-700 hover:bg-blue-50 hover:text-blue-700 transition-colors"
            >
              {t(item.key)}
            </button>
          ))}
        </nav>
      </aside>
    </>
  )
}
