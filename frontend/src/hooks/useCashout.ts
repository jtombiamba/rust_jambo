import { useEffect, useState } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import { extractApiError } from '../utils/errors'
import { getCachedConfig } from '../utils/configCache'
import type { CashoutConfig, CashoutHistoryData } from '../components/dashboard/types'

export function useCashout(fetchData: () => void, showToast: (msg: string) => void) {
  const { t } = useTranslation()
  const [cashoutConfig, setCashoutConfig] = useState<CashoutConfig | null>(null)
  const [cashoutOpen, setCashoutOpen] = useState(false)
  const [cashoutCredits, setCashoutCredits] = useState('')
  const [cashoutMethod, setCashoutMethod] = useState<'paypal_email' | 'stripe_connect'>('paypal_email')
  const [cashoutDestination, setCashoutDestination] = useState('')
  const [cashoutConnecting, setCashoutConnecting] = useState(false)
  const [cashoutSubmitting, setCashoutSubmitting] = useState(false)
  const [cashoutError, setCashoutError] = useState<string | null>(null)
  const [showCashoutHistory, setShowCashoutHistory] = useState(false)
  const [cashoutHistory, setCashoutHistory] = useState<CashoutHistoryData | null>(null)
  const [cashoutHistoryLoading, setCashoutHistoryLoading] = useState(false)

  useEffect(() => {
    const cached = getCachedConfig()
    if (cached) {
      setCashoutConfig({
        enabled: cached.cashout_enabled,
        minCredits: cached.cashout_min_credits,
        creditsPerEur: cached.cashout_credits_per_eur,
        maxEurCents: cached.cashout_max_eur_cents,
      })
      return
    }
    axios
      .get('/api/config')
      .then((res) => {
        setCashoutConfig({
          enabled: res.data.cashout_enabled,
          minCredits: res.data.cashout_min_credits,
          creditsPerEur: res.data.cashout_credits_per_eur,
          maxEurCents: res.data.cashout_max_eur_cents,
        })
      })
      .catch(() => {
        // leave cashoutConfig null -> cashout hidden
      })
  }, [])

  const openCashoutModal = () => {
    setCashoutCredits('')
    setCashoutMethod('paypal_email')
    setCashoutDestination('')
    setCashoutError(null)
    setCashoutOpen(true)
  }

  const closeCashoutModal = () => {
    setCashoutOpen(false)
    setCashoutError(null)
  }

  const handleConnectBank = async () => {
    setCashoutConnecting(true)
    setCashoutError(null)
    try {
      const res = await axios.post<{ url: string }>('/api/me/payout/onboarding')
      window.location.href = res.data.url
    } catch (err: unknown) {
      setCashoutError(extractApiError(err).message || t('dashboard.cashoutConnectFailed'))
      setCashoutConnecting(false)
    }
  }

  const handleCashoutSubmit = async () => {
    if (!cashoutConfig) return
    setCashoutError(null)

    const credits = parseInt(cashoutCredits, 10)
    if (!Number.isFinite(credits) || credits <= 0) {
      setCashoutError(
        t('dashboard.cashoutMin', {
          credits: cashoutConfig.minCredits,
          eur: (cashoutConfig.minCredits / cashoutConfig.creditsPerEur).toFixed(2),
        }),
      )
      return
    }
    if (credits < cashoutConfig.minCredits) {
      setCashoutError(
        t('dashboard.cashoutMin', {
          credits: cashoutConfig.minCredits,
          eur: (cashoutConfig.minCredits / cashoutConfig.creditsPerEur).toFixed(2),
        }),
      )
      return
    }
    if (credits % cashoutConfig.creditsPerEur !== 0) {
      setCashoutError(t('dashboard.cashoutMultiple', { credits: cashoutConfig.creditsPerEur }))
      return
    }
    if ((credits / cashoutConfig.creditsPerEur) * 100 > cashoutConfig.maxEurCents) {
      setCashoutError(t('dashboard.cashoutMax', { eur: (cashoutConfig.maxEurCents / 100).toFixed(2) }))
      return
    }
    if (cashoutMethod === 'paypal_email') {
      const email = cashoutDestination.trim()
      if (!email.includes('@') || !email.includes('.')) {
        setCashoutError(t('dashboard.cashoutInvalidEmail'))
        return
      }
    }

    setCashoutSubmitting(true)
    try {
      await axios.post('/api/me/cashout', {
        credits,
        payout_method: cashoutMethod,
        payout_destination: cashoutMethod === 'paypal_email' ? cashoutDestination.trim() : '',
      })
      setCashoutOpen(false)
      showToast(t('dashboard.cashoutSuccess'))
      await fetchData()
      if (showCashoutHistory) {
        fetchCashoutHistory()
      }
    } catch (err: unknown) {
      setCashoutError(extractApiError(err).message || t('dashboard.cashoutSubmit'))
    } finally {
      setCashoutSubmitting(false)
    }
  }

  const fetchCashoutHistory = async () => {
    setCashoutHistoryLoading(true)
    try {
      const res = await axios.get<CashoutHistoryData>('/api/me/cashout', {
        params: { page: 1, per_page: 100 },
      })
      setCashoutHistory(res.data)
    } catch (err: unknown) {
      console.error('Failed to load cashout history', err)
    } finally {
      setCashoutHistoryLoading(false)
    }
  }

  const toggleCashoutHistory = () => {
    const next = !showCashoutHistory
    setShowCashoutHistory(next)
    if (next && !cashoutHistory) {
      fetchCashoutHistory()
    }
  }

  return {
    cashoutConfig,
    cashoutOpen,
    cashoutCredits,
    cashoutMethod,
    cashoutDestination,
    cashoutConnecting,
    cashoutSubmitting,
    cashoutError,
    showCashoutHistory,
    cashoutHistory,
    cashoutHistoryLoading,
    openCashoutModal,
    closeCashoutModal,
    setCashoutCredits,
    setCashoutMethod,
    setCashoutDestination,
    handleConnectBank,
    handleCashoutSubmit,
    toggleCashoutHistory,
  }
}
