import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import axios from 'axios'
import { extractApiError } from '../utils/errors'

interface ContactFormProps {
  isOpen: boolean
  onClose: () => void
}

export default function ContactForm({ isOpen, onClose }: ContactFormProps) {
  const { t } = useTranslation()
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [subject, setSubject] = useState('')
  const [message, setMessage] = useState('')
  const [sending, setSending] = useState(false)
  const [sent, setSent] = useState(false)
  const [error, setError] = useState<string | null>(null)

  if (!isOpen) return null

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError(null)
    setSending(true)

    try {
      await axios.post('/api/contact', { name, email, subject, message })
      setSent(true)
    } catch (err: unknown) {
      setError(extractApiError(err).message || t('contact.sendFailed'))
    } finally {
      setSending(false)
    }
  }

  const handleClose = () => {
    setName('')
    setEmail('')
    setSubject('')
    setMessage('')
    setSent(false)
    setError(null)
    onClose()
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-md p-6 relative">
        <button
          onClick={handleClose}
          className="absolute top-3 right-3 text-gray-400 hover:text-gray-600 text-2xl leading-none"
        >
          &times;
        </button>

        <h2 className="text-xl font-semibold mb-4">{t('contact.contactUs')}</h2>

        {sent ? (
          <div className="text-center py-4">
            <p className="text-emerald-600 font-semibold mb-2">{t('contact.messageSent')}</p>
            <p className="text-gray-600 text-sm mb-4">
              {t('contact.thankYou')}
            </p>
            <button
              onClick={handleClose}
              className="px-4 py-2 bg-emerald-600 text-white rounded-lg font-semibold hover:bg-emerald-700"
            >
              {t('common.close')}
            </button>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-3">
            {error && (
              <div className="p-3 bg-red-100 text-red-700 rounded text-sm">{error}</div>
            )}
            <div>
              <label className="block text-sm font-medium mb-1">{t('contact.name')}</label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="w-full border rounded px-3 py-2"
                required
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">{t('contact.email')}</label>
              <input
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                className="w-full border rounded px-3 py-2"
                required
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">{t('contact.subject')}</label>
              <input
                type="text"
                value={subject}
                onChange={(e) => setSubject(e.target.value)}
                className="w-full border rounded px-3 py-2"
                required
              />
            </div>
            <div>
              <label className="block text-sm font-medium mb-1">{t('contact.message')}</label>
              <textarea
                value={message}
                onChange={(e) => setMessage(e.target.value)}
                className="w-full border rounded px-3 py-2 min-h-[100px]"
                required
              />
            </div>
            <button
              type="submit"
              disabled={sending}
              className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700 disabled:opacity-50"
            >
              {sending ? t('contact.sending') : t('contact.send')}
            </button>
          </form>
        )}
      </div>
    </div>
  )
}
