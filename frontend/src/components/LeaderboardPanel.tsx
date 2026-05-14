import { useEffect, useState } from 'react'
import axios from 'axios'

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

export default function LeaderboardPanel() {
  const [data, setData] = useState<LeaderboardResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    axios
      .get<LeaderboardResponse>('/api/me/leaderboard')
      .then((res) => {
        setData(res.data)
        setLoading(false)
      })
      .catch((err) => {
        setError(err.response?.data?.error || 'Failed to load leaderboard')
        setLoading(false)
      })
  }, [])

  if (loading) {
    return (
      <div className="bg-white p-4 sm:p-6 rounded-lg shadow mb-6 sm:mb-8">
        <p className="text-gray-500">Loading leaderboard...</p>
      </div>
    )
  }

  if (error) {
    return (
      <div className="bg-white p-4 sm:p-6 rounded-lg shadow mb-6 sm:mb-8">
        <p className="text-red-500">{error}</p>
      </div>
    )
  }

  if (!data) return null

  return (
    <div className="bg-white p-4 sm:p-6 rounded-lg shadow mb-6 sm:mb-8">
      <h2 className="text-lg sm:text-xl font-semibold mb-4">Leaderboard</h2>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 sm:gap-6">
        <div>
          <h3 className="text-sm font-semibold text-gray-600 mb-2 uppercase tracking-wide">
            Top by Wins
          </h3>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b text-gray-500">
                  <th className="py-2 pr-2">#</th>
                  <th className="py-2 pr-2">Player</th>
                  <th className="py-2 pr-2">Wins</th>
                </tr>
              </thead>
              <tbody>
                {data.top_by_wins.map((entry) => (
                  <tr
                    key={entry.user_id}
                    className={`border-b ${
                      entry.is_current_user ? 'bg-blue-50 font-semibold' : ''
                    }`}
                  >
                    <td className="py-2 pr-2">{entry.rank}</td>
                    <td className="py-2 pr-2">
                      {entry.pseudo}
                      {entry.is_current_user && (
                        <span className="text-blue-600 text-xs ml-1">(you)</span>
                      )}
                    </td>
                    <td className="py-2 pr-2">{entry.wins}</td>
                  </tr>
                ))}
                {data.current_user_wins_rank && (
                  <tr className="border-b text-gray-500 italic">
                    <td className="py-2 pr-2">{data.current_user_wins_rank}</td>
                    <td className="py-2 pr-2">You</td>
                    <td className="py-2 pr-2">-</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>

        <div>
          <h3 className="text-sm font-semibold text-gray-600 mb-2 uppercase tracking-wide">
            Top by Streak
          </h3>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b text-gray-500">
                  <th className="py-2 pr-2">#</th>
                  <th className="py-2 pr-2">Player</th>
                  <th className="py-2 pr-2">Streak</th>
                </tr>
              </thead>
              <tbody>
                {data.top_by_streak.map((entry) => (
                  <tr
                    key={entry.user_id}
                    className={`border-b ${
                      entry.is_current_user ? 'bg-blue-50 font-semibold' : ''
                    }`}
                  >
                    <td className="py-2 pr-2">{entry.rank}</td>
                    <td className="py-2 pr-2">
                      {entry.pseudo}
                      {entry.is_current_user && (
                        <span className="text-blue-600 text-xs ml-1">(you)</span>
                      )}
                    </td>
                    <td className="py-2 pr-2">{entry.winning_streak}</td>
                  </tr>
                ))}
                {data.current_user_streak_rank && (
                  <tr className="border-b text-gray-500 italic">
                    <td className="py-2 pr-2">{data.current_user_streak_rank}</td>
                    <td className="py-2 pr-2">You</td>
                    <td className="py-2 pr-2">-</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  )
}
