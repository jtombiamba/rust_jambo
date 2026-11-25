import { useCallback, useEffect, useRef, useState } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import type {
  ProfileData,
  GameHistoryData,
  QuickGameResponse,
  SortField,
  SortDir,
} from '../components/dashboard/types'

export function useDashboardData(
  onResumeGame: (data: QuickGameResponse) => void,
  showToast: (msg: string) => void,
) {
  const { t } = useTranslation()
  const [profile, setProfile] = useState<ProfileData | null>(null)
  const [history, setHistory] = useState<GameHistoryData | null>(null)
  const [page, setPage] = useState(1)
  const [loading, setLoading] = useState(true)
  const [freezeRemainingSec, setFreezeRemainingSec] = useState(0)
  const [statusFilter, setStatusFilter] = useState('')
  const [sortField, setSortField] = useState<SortField>(null)
  const [sortDir, setSortDir] = useState<SortDir>('desc')
  const [perPage, setPerPage] = useState(10)
  const freezeTimerRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined)

  const fetchData = useCallback(() => {
    setLoading(true)
    const params: Record<string, string | number> = { page, per_page: perPage }
    if (statusFilter) params.status = statusFilter
    if (sortField) params.order_by = `${sortField}_${sortDir}`

    Promise.all([
      axios.get<ProfileData>('/api/me/profile'),
      axios.get<GameHistoryData>('/api/me/games', { params }),
    ])
      .then(([profileRes, historyRes]) => {
        setProfile(profileRes.data)
        setHistory(historyRes.data)
        if (profileRes.data.frozen_until) {
          const frozenUntil = new Date(profileRes.data.frozen_until).getTime()
          const now = Date.now()
          const remaining = Math.max(0, Math.floor((frozenUntil - now) / 1000))
          setFreezeRemainingSec(remaining)
        } else {
          setFreezeRemainingSec(0)
        }
        setLoading(false)
      })
      .catch((err) => {
        console.error('Failed to load dashboard', err)
        setLoading(false)
      })
  }, [page, perPage, statusFilter, sortField, sortDir])

  useEffect(() => {
    fetchData()
  }, [fetchData])

  useEffect(() => {
    if (freezeRemainingSec <= 0) {
      if (freezeTimerRef.current) {
        clearInterval(freezeTimerRef.current)
        freezeTimerRef.current = undefined
      }
      return
    }
    freezeTimerRef.current = setInterval(() => {
      setFreezeRemainingSec((prev) => {
        if (prev <= 1) {
          if (freezeTimerRef.current) {
            clearInterval(freezeTimerRef.current)
            freezeTimerRef.current = undefined
          }
          return 0
        }
        return prev - 1
      })
    }, 1000)
    return () => {
      if (freezeTimerRef.current) {
        clearInterval(freezeTimerRef.current)
        freezeTimerRef.current = undefined
      }
    }
  }, [freezeRemainingSec])

  useEffect(() => {
    setPage(1)
  }, [statusFilter, sortField, sortDir, perPage])

  const handleSort = (field: SortField) => {
    if (sortField === field) {
      if (sortDir === 'desc') {
        setSortDir('asc')
      } else {
        setSortField(null)
        setSortDir('desc')
      }
    } else {
      setSortField(field)
      setSortDir('desc')
    }
  }

  const sortIndicator = (field: SortField) => {
    if (sortField !== field) return ''
    return sortDir === 'asc' ? ' \u25B2' : ' \u25BC'
  }

  const totalPages = history ? Math.ceil(history.total / history.per_page) : 0

  const handleGameClick = (gameId: string) => {
    axios
      .get<QuickGameResponse>(`/api/me/games/${gameId}`)
      .then((res) => {
        onResumeGame(res.data)
      })
      .catch((err) => {
        if (err.response?.status === 410) {
          showToast(t('dashboard.gameAlreadyFinished'))
        } else {
          showToast(t('dashboard.failedLoadGame'))
        }
      })
  }

  return {
    profile,
    history,
    page,
    perPage,
    loading,
    freezeRemainingSec,
    statusFilter,
    sortField,
    sortDir,
    totalPages,
    fetchData,
    handleSort,
    sortIndicator,
    setStatusFilter,
    setPage,
    setPerPage,
    handleGameClick,
  }
}
