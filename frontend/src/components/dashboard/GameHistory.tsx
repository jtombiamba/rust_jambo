import { useTranslation } from 'react-i18next'
import type { GameHistoryData, SortField } from './types'

const STATUS_OPTIONS = [
  { label: 'All', value: '' },
  { label: 'Pending', value: 'pending' },
  { label: 'Active', value: 'active' },
  { label: 'Ready', value: 'ready' },
  { label: 'Finished', value: 'finished' },
  { label: 'Kora', value: 'kora,double_kora' },
  { label: 'Cancelled', value: 'cancelled' },
]

interface GameHistoryProps {
  history: GameHistoryData | null
  page: number
  perPage: number
  statusFilter: string
  totalPages: number
  onStatusFilter: (value: string) => void
  onSort: (field: SortField) => void
  onPage: (page: number) => void
  onPerPage: (perPage: number) => void
  onGameClick: (gameId: string) => void
  sortIndicator: (field: SortField) => string
}

function statusBadge(status: string) {
  if (status === 'active') return 'bg-blue-100 text-blue-700'
  if (status === 'finished') return 'bg-green-100 text-green-700'
  if (status === 'pending') return 'bg-yellow-100 text-yellow-700'
  if (status === 'kora' || status === 'double_kora') return 'bg-yellow-100 text-yellow-700'
  if (status === 'cancelled') return 'bg-red-100 text-red-700'
  return 'bg-gray-100 text-gray-600'
}

export default function GameHistory({
  history,
  page,
  perPage,
  statusFilter,
  totalPages,
  onStatusFilter,
  onSort,
  onPage,
  onPerPage,
  onGameClick,
  sortIndicator,
}: GameHistoryProps) {
  const { t } = useTranslation()

  return (
    <div className="bg-white rounded-lg border border-gray-200 shadow-sm hover:shadow-md transition-shadow p-4 sm:p-6">
      <div className="flex flex-wrap items-center justify-between mb-4">
        <h2 className="text-lg font-semibold text-gray-900">
          {t('dashboard.gameHistory')}
          {history && <span className="text-gray-500 text-sm ml-2">({history.total} total)</span>}
        </h2>
        <div className="flex items-center gap-2 mt-2 sm:mt-0">
          <span className="text-xs text-gray-500">{t('dashboard.perPage')}</span>
          <select
            value={perPage}
            onChange={(e) => onPerPage(Number(e.target.value))}
            className="text-xs border border-gray-300 rounded px-2 py-1"
          >
            <option value={10}>10</option>
            <option value={20}>20</option>
            <option value={50}>50</option>
          </select>
        </div>
      </div>

      <div className="flex flex-wrap gap-1 sm:gap-2 mb-4">
        {STATUS_OPTIONS.map((opt) => (
          <button
            key={opt.value}
            onClick={() => onStatusFilter(opt.value)}
            className={`px-2 sm:px-3 py-1 rounded text-xs sm:text-sm font-medium transition-colors ${
              statusFilter === opt.value
                ? 'bg-blue-600 text-white'
                : 'bg-white text-gray-600 hover:bg-gray-200 border border-gray-300'
            }`}
          >
            {opt.label}
          </button>
        ))}
      </div>

      {history && history.games.length === 0 ? (
        <p className="text-gray-500">{t('dashboard.noGamesFound')}</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <thead>
              <tr className="border-b">
                <th className="py-2 pr-4">{t('dashboard.status')}</th>
                <th className="py-2 pr-4">{t('dashboard.result')}</th>
                <th
                  className="py-2 pr-4 cursor-pointer select-none hover:text-blue-600"
                  onClick={() => onSort('bet')}
                >
                  {t('dashboard.bet')}
                  {sortIndicator('bet')}
                </th>
                <th className="py-2 pr-4">{t('dashboard.players')}</th>
                <th
                  className="py-2 cursor-pointer select-none hover:text-blue-600"
                  onClick={() => onSort('date')}
                >
                  {t('dashboard.date')}
                  {sortIndicator('date')}
                </th>
              </tr>
            </thead>
            <tbody>
              {history?.games.map((game) => (
                <tr
                  key={game.game_id}
                  className="border-b cursor-pointer hover:bg-gray-50 transition-colors"
                  onClick={() => onGameClick(game.game_id)}
                >
                  <td className="py-2 pr-4">
                    <span
                      className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${statusBadge(game.status)}`}
                    >
                      {game.status}
                    </span>
                  </td>
                  <td className="py-2 pr-4">
                    <span
                      className={
                        game.result === 'win'
                          ? 'text-green-600 font-semibold'
                          : game.result === 'loss'
                            ? 'text-red-600'
                            : 'text-gray-500'
                      }
                    >
                      {game.result}
                    </span>
                  </td>
                  <td className="py-2 pr-4">{game.bet}</td>
                  <td className="py-2 pr-4 text-gray-500">{game.player_count}</td>
                  <td className="py-2 text-gray-500 text-xs">
                    {new Date(game.played_at).toLocaleDateString()}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {history && history.total > history.per_page && (
        <div className="flex justify-center items-center gap-1 mt-4">
          <button
            onClick={() => onPage(Math.max(1, page - 1))}
            disabled={page <= 1}
            className="px-3 py-1 bg-gray-200 rounded disabled:opacity-50 hover:bg-gray-300 text-sm"
          >
            {t('common.prev')}
          </button>
          {Array.from({ length: Math.min(totalPages, 7) }, (_, i) => {
            let pageNum: number
            if (totalPages <= 7) {
              pageNum = i + 1
            } else if (page <= 4) {
              pageNum = i + 1
            } else if (page >= totalPages - 3) {
              pageNum = totalPages - 6 + i
            } else {
              pageNum = page - 3 + i
            }
            return (
              <button
                key={pageNum}
                onClick={() => onPage(pageNum)}
                className={`px-3 py-1 rounded text-sm ${
                  pageNum === page ? 'bg-blue-600 text-white' : 'bg-gray-200 hover:bg-gray-300'
                }`}
              >
                {pageNum}
              </button>
            )
          })}
          <button
            onClick={() => onPage(page + 1)}
            disabled={page >= totalPages}
            className="px-3 py-1 bg-gray-200 rounded disabled:opacity-50 hover:bg-gray-300 text-sm"
          >
            {t('common.next')}
          </button>
        </div>
      )}
    </div>
  )
}
