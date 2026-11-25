import { useEffect, useState } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import { extractApiError } from '../../utils/errors'

interface LeaderboardEntry {
  rank: number
  user_id: string
  pseudo: string
  wins: number
  winning_streak: number
  is_current_user: boolean
}

interface LeaderboardResponse {
  top_by_wins: LeaderboardEntry[]
  top_by_streak: LeaderboardEntry[]
  current_user_wins_rank: number | null
  current_user_streak_rank: number | null
}

export default function LeaderboardSnippet() {
  const { t } = useTranslation()
  const [data, setData] = useState<LeaderboardResponse | null>(null)

  useEffect(() => {
    axios
      .get<LeaderboardResponse>('/api/me/leaderboard')
      .then((res) => setData(res.data))
      .catch((err) => {
        console.error('Failed to load leaderboard snippet', extractApiError(err).message)
      })
  }, [])

  const top = data?.top_by_wins.slice(0, 5) ?? []
  const yourRank = data?.current_user_wins_rank

  return (
    <div className="bg-white rounded-lg border border-gray-200 shadow-sm hover:shadow-md transition-shadow p-4">
      <h3 className="text-sm font-semibold text-gray-500 uppercase tracking-wide mb-3">
        {t('leaderboard.title')}
      </h3>

      {top.length === 0 ? (
        <p className="text-gray-500 text-sm">{t('leaderboard.loading')}</p>
      ) : (
        <ul className="space-y-1">
          {top.map((entry) => (
            <li
              key={entry.user_id}
              className={`flex items-center justify-between px-2 py-1.5 rounded ${
                entry.is_current_user ? 'bg-blue-50 font-semibold' : ''
              }`}
            >
              <span className="flex items-center gap-2 text-sm">
                <span className="w-6 text-center font-bold text-gray-400">{entry.rank}</span>
                <span className="text-gray-800">
                  {entry.pseudo}
                  {entry.is_current_user && (
                    <span className="text-blue-600 text-xs ml-1">{t('leaderboard.you')}</span>
                  )}
                </span>
              </span>
              <span className="text-sm text-gray-600">{entry.wins}</span>
            </li>
          ))}
        </ul>
      )}

      {yourRank && (
        <p className="mt-3 pt-3 border-t border-gray-100 text-sm text-gray-500">
          {t('leaderboard.yourRank')}: <span className="font-semibold text-gray-800">#{yourRank}</span>
        </p>
      )}
    </div>
  )
}
