import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '../stores/useAuthStore'

function RegisterForm() {
  const { register, authError, authLoading, setAuthView } = useAuthStore()
  const { t } = useTranslation()
  const [pseudo, setPseudo] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [passwordConfirm, setPasswordConfirm] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [showPasswordConfirm, setShowPasswordConfirm] = useState(false)

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    register({ pseudo, email, password, password_confirm: passwordConfirm })
  }

  return (
    <div>
      <h2 className="text-xl font-semibold mb-4">{t('auth.createAccount')}</h2>
      {authError && (
        <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">
          {authError.error}
        </div>
      )}
      <form onSubmit={handleSubmit} className="space-y-3">
        <div>
          <label className="block text-sm font-medium mb-1">{t('auth.pseudo')}</label>
          <input
            type="text"
            value={pseudo}
            onChange={(e) => setPseudo(e.target.value)}
            className={`w-full border rounded px-3 py-2 ${authError?.field === 'pseudo' ? 'border-red-500' : ''}`}
            required
            autoComplete="username"
          />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">{t('auth.email')}</label>
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className={`w-full border rounded px-3 py-2 ${authError?.field === 'email' ? 'border-red-500' : ''}`}
            required
            autoComplete="email"
          />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">{t('auth.password')}</label>
          <div className="relative">
            <input
              type={showPassword ? 'text' : 'password'}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className={`w-full border rounded px-3 py-2 pr-10 ${authError?.field === 'password' ? 'border-red-500' : ''}`}
              required
              minLength={8}
              autoComplete="new-password"
            />
            <button
              type="button"
              onClick={() => setShowPassword(!showPassword)}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600"
              tabIndex={-1}
            >
              {showPassword ? (
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94" />
                  <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19" />
                  <line x1="1" y1="1" x2="23" y2="23" />
                </svg>
              ) : (
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                  <circle cx="12" cy="12" r="3" />
                </svg>
              )}
            </button>
          </div>
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">{t('auth.confirmPassword')}</label>
          <div className="relative">
            <input
              type={showPasswordConfirm ? 'text' : 'password'}
              value={passwordConfirm}
              onChange={(e) => setPasswordConfirm(e.target.value)}
              className={`w-full border rounded px-3 py-2 pr-10 ${authError?.field === 'password_confirm' ? 'border-red-500' : ''}`}
              required
              minLength={8}
              autoComplete="new-password"
            />
            <button
              type="button"
              onClick={() => setShowPasswordConfirm(!showPasswordConfirm)}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600"
              tabIndex={-1}
            >
              {showPasswordConfirm ? (
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94" />
                  <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19" />
                  <line x1="1" y1="1" x2="23" y2="23" />
                </svg>
              ) : (
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                  <circle cx="12" cy="12" r="3" />
                </svg>
              )}
            </button>
          </div>
        </div>
        <button
          type="submit"
          disabled={authLoading}
          className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700 disabled:opacity-50"
        >
          {authLoading ? t('auth.creatingAccount') : t('auth.createAccountBtn')}
        </button>
      </form>
      <p className="mt-4 text-center text-sm text-gray-500">
        {t('auth.alreadyHaveAccount')}{' '}
        <button
          onClick={() => setAuthView('login')}
          className="text-blue-600 hover:underline"
        >
          {t('auth.logIn')}
        </button>
      </p>
    </div>
  )
}

function LoginForm() {
  const { login, authError, authLoading, setAuthView } = useAuthStore()
  const { t } = useTranslation()
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [showPassword, setShowPassword] = useState(false)

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    login({ email, password })
  }

  return (
    <div>
      <h2 className="text-xl font-semibold mb-4">{t('auth.logIn')}</h2>
      {authError && (
        <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">
          {authError.error}
        </div>
      )}
      <form onSubmit={handleSubmit} className="space-y-3">
        <div>
          <label className="block text-sm font-medium mb-1">{t('auth.email')}</label>
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="w-full border rounded px-3 py-2"
            required
            autoComplete="email"
          />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">{t('auth.password')}</label>
          <div className="relative">
            <input
              type={showPassword ? 'text' : 'password'}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="w-full border rounded px-3 py-2 pr-10"
              required
              autoComplete="current-password"
            />
            <button
              type="button"
              onClick={() => setShowPassword(!showPassword)}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600"
              tabIndex={-1}
            >
              {showPassword ? (
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94" />
                  <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19" />
                  <line x1="1" y1="1" x2="23" y2="23" />
                </svg>
              ) : (
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                  <circle cx="12" cy="12" r="3" />
                </svg>
              )}
            </button>
          </div>
        </div>
        <button
          type="submit"
          disabled={authLoading}
          className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700 disabled:opacity-50"
        >
          {authLoading ? t('auth.loggingIn') : t('auth.logIn')}
        </button>
      </form>
      <div className="mt-4 space-y-2 text-center text-sm">
        <p>
          <button
            onClick={() => setAuthView('forgot-password')}
            className="text-blue-600 hover:underline"
          >
            {t('auth.passwordForgotten')}
          </button>
        </p>
        <p className="text-gray-500">
          {t('auth.noAccount')}{' '}
          <button
            onClick={() => setAuthView('register')}
            className="text-blue-600 hover:underline"
          >
            {t('auth.createOne')}
          </button>
        </p>
      </div>
    </div>
  )
}

function ForgotPasswordForm() {
  const { forgotPassword, authError, authLoading, setAuthView } = useAuthStore()
  const { t } = useTranslation()
  const [email, setEmail] = useState('')
  const [sent, setSent] = useState(false)

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    forgotPassword(email)
    setSent(true)
  }

  if (sent) {
    return (
      <div>
        <h2 className="text-xl font-semibold mb-4">{t('auth.checkYourEmail')}</h2>
        <p className="text-gray-600 mb-4">
          {authError?.error || t('auth.emailSentMessage', { email })}
        </p>
        <button
          onClick={() => setAuthView('login')}
          className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700"
        >
          {t('auth.backToLogin')}
        </button>
      </div>
    )
  }

  return (
    <div>
      <h2 className="text-xl font-semibold mb-4">{t('auth.resetYourPassword')}</h2>
      <p className="text-gray-600 mb-4 text-sm">
        {t('auth.resetInstructions')}
      </p>
      {authError && (
        <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">
          {authError.error}
        </div>
      )}
      <form onSubmit={handleSubmit} className="space-y-3">
        <div>
          <label className="block text-sm font-medium mb-1">{t('auth.email')}</label>
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="w-full border rounded px-3 py-2"
            placeholder={t('auth.resetEmailPlaceholder')}
            required
            autoComplete="email"
          />
        </div>
        <button
          type="submit"
          disabled={authLoading}
          className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700 disabled:opacity-50"
        >
          {authLoading ? t('auth.sending') : t('auth.sendResetLink')}
        </button>
      </form>
      <p className="mt-4 text-center text-sm text-gray-500">
        <button
          onClick={() => setAuthView('login')}
          className="text-blue-600 hover:underline"
        >
          {t('auth.backToLogin')}
        </button>
      </p>
    </div>
  )
}

export default function AuthModal() {
  const { authModalOpen, authView, closeAuthModal, setAuthView, pendingInviteMessage } = useAuthStore()
  const { t } = useTranslation()

  if (!authModalOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-md p-6 mx-4 relative">
        <button
          onClick={closeAuthModal}
          className="absolute top-3 right-3 text-gray-400 hover:text-gray-600 text-2xl leading-none"
        >
          &times;
        </button>

        {pendingInviteMessage && (
          <div className="mb-4 p-3 bg-emerald-50 border border-emerald-200 text-emerald-800 rounded text-sm">
            {pendingInviteMessage}
          </div>
        )}

        {authView === 'register' && <RegisterForm />}
        {authView === 'login' && <LoginForm />}
        {authView === 'forgot-password' && <ForgotPasswordForm />}

        {authView === 'choice' && (
          <div>
            <h2 className="text-xl font-semibold mb-6 text-center">{t('auth.welcome')}</h2>
            {pendingInviteMessage && (
              <p className="text-sm text-emerald-700 mb-4 text-center">
                {pendingInviteMessage}
              </p>
            )}
            <div className="space-y-3">
              <button
                onClick={() => setAuthView('register')}
                className="w-full py-3 bg-blue-600 text-white rounded-lg font-semibold hover:bg-blue-700"
              >
                {t('auth.createAccount')}
              </button>
              <button
                onClick={() => setAuthView('login')}
                className="w-full py-3 bg-gray-100 text-gray-800 rounded-lg font-semibold hover:bg-gray-200"
              >
                {t('auth.logIn')}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
