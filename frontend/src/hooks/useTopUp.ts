import { useState } from 'react'
import axios from 'axios'
import { useTranslation } from 'react-i18next'
import { extractApiError } from '../utils/errors'

async function openPaymentPopup(redirectUrl: string, name: string): Promise<boolean> {
  const width = 500
  const height = 600
  const left = window.screenX + (window.outerWidth - width) / 2
  const top = window.screenY + (window.outerHeight - height) / 2
  const popup = window.open(
    redirectUrl,
    name,
    `width=${width},height=${height},left=${left},top=${top},popup=yes`,
  )

  if (!popup) return false

  await new Promise<void>((resolve) => {
    const interval = setInterval(() => {
      try {
        if (popup.closed) {
          clearInterval(interval)
          resolve()
        }
      } catch {
        clearInterval(interval)
        resolve()
      }
    }, 500)
    setTimeout(() => {
      try {
        if (!popup.closed) {
          popup.close()
        }
      } catch {
        // popup may already be closed or cross-origin
      }
      clearInterval(interval)
      resolve()
    }, 5 * 60 * 1000)
  })

  return true
}

export function useTopUp(fetchData: () => void, showToast: (msg: string) => void) {
  const { t } = useTranslation()
  const [toppingUp, setToppingUp] = useState(false)
  const [unfreezing, setUnfreezing] = useState(false)

  const handleTopUp = async (amountEurCents: number) => {
    setToppingUp(true)
    try {
      const orderRes = await axios.post('/api/me/topup', { amount_eur_cents: amountEurCents })
      const { redirect_url } = orderRes.data as { order_id: string; redirect_url: string }

      const opened = await openPaymentPopup(redirect_url, 'payment_topup')
      if (!opened) {
        showToast(t('dashboard.popupBlocked'))
        return
      }

      await fetchData()
    } catch (err: unknown) {
      showToast(extractApiError(err).message || 'Top up failed')
    } finally {
      setToppingUp(false)
    }
  }

  const handleUnfreeze = async () => {
    setUnfreezing(true)
    try {
      const orderRes = await axios.post('/api/me/unfreeze')
      const { redirect_url } = orderRes.data as { order_id: string; redirect_url: string }

      const opened = await openPaymentPopup(redirect_url, 'payment_unfreeze')
      if (!opened) {
        showToast(t('dashboard.popupBlocked'))
        return
      }

      await fetchData()
    } catch (err: unknown) {
      showToast(extractApiError(err).message || 'Unfreeze failed')
    } finally {
      setUnfreezing(false)
    }
  }

  return { toppingUp, handleTopUp, unfreezing, handleUnfreeze }
}
