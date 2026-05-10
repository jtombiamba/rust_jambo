import { useState } from 'react'
import { useAuthStore } from '../stores/useAuthStore'

function RegisterForm() {
  const { register, authError, authLoading, setAuthView } = useAuthStore()
  const [pseudo, setPseudo] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [passwordConfirm, setPasswordConfirm] = useState('')

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    register({ pseudo, email, password, password_confirm: passwordConfirm })
  }

  return (
    <div>
      <h2 className="text-xl font-semibold mb-4">Create an account</h2>
      {authError && (
        <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">
          {authError.error}
        </div>
      )}
      <form onSubmit={handleSubmit} className="space-y-3">
        <div>
          <label className="block text-sm font-medium mb-1">Pseudo</label>
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
          <label className="block text-sm font-medium mb-1">Email</label>
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
          <label className="block text-sm font-medium mb-1">Password</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className={`w-full border rounded px-3 py-2 ${authError?.field === 'password' ? 'border-red-500' : ''}`}
            required
            minLength={8}
            autoComplete="new-password"
          />
        </div>
        <div>
          <label className="block text-sm font-medium mb-1">Confirm Password</label>
          <input
            type="password"
            value={passwordConfirm}
            onChange={(e) => setPasswordConfirm(e.target.value)}
            className={`w-full border rounded px-3 py-2 ${authError?.field === 'password_confirm' ? 'border-red-500' : ''}`}
            required
            minLength={8}
            autoComplete="new-password"
          />
        </div>
        <button
          type="submit"
          disabled={authLoading}
          className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700 disabled:opacity-50"
        >
          {authLoading ? 'Creating account...' : 'Create account'}
        </button>
      </form>
      <p className="mt-4 text-center text-sm text-gray-500">
        Already have an account?{' '}
        <button
          onClick={() => setAuthView('login')}
          className="text-blue-600 hover:underline"
        >
          Log in
        </button>
      </p>
    </div>
  )
}

function LoginForm() {
  const { login, authError, authLoading, setAuthView } = useAuthStore()
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    login({ email, password })
  }

  return (
    <div>
      <h2 className="text-xl font-semibold mb-4">Log in</h2>
      {authError && (
        <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">
          {authError.error}
        </div>
      )}
      <form onSubmit={handleSubmit} className="space-y-3">
        <div>
          <label className="block text-sm font-medium mb-1">Email</label>
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
          <label className="block text-sm font-medium mb-1">Password</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="w-full border rounded px-3 py-2"
            required
            autoComplete="current-password"
          />
        </div>
        <button
          type="submit"
          disabled={authLoading}
          className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700 disabled:opacity-50"
        >
          {authLoading ? 'Logging in...' : 'Log in'}
        </button>
      </form>
      <div className="mt-4 space-y-2 text-center text-sm">
        <p>
          <button
            onClick={() => setAuthView('forgot-password')}
            className="text-blue-600 hover:underline"
          >
            Password forgotten? Reset
          </button>
        </p>
        <p className="text-gray-500">
          Don't have an account?{' '}
          <button
            onClick={() => setAuthView('register')}
            className="text-blue-600 hover:underline"
          >
            Create one
          </button>
        </p>
      </div>
    </div>
  )
}

function ForgotPasswordForm() {
  const { forgotPassword, authError, authLoading, setAuthView } = useAuthStore()
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
        <h2 className="text-xl font-semibold mb-4">Check your email</h2>
        <p className="text-gray-600 mb-4">
          {authError?.error || `If ${email} exists, you will receive an email to reset your password.`}
        </p>
        <button
          onClick={() => setAuthView('login')}
          className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700"
        >
          Back to login
        </button>
      </div>
    )
  }

  return (
    <div>
      <h2 className="text-xl font-semibold mb-4">Reset your password</h2>
      <p className="text-gray-600 mb-4 text-sm">
        Enter your email address and we'll send you a link to reset your password.
      </p>
      {authError && (
        <div className="mb-4 p-3 bg-red-100 text-red-700 rounded text-sm">
          {authError.error}
        </div>
      )}
      <form onSubmit={handleSubmit} className="space-y-3">
        <div>
          <label className="block text-sm font-medium mb-1">Email</label>
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            className="w-full border rounded px-3 py-2"
            placeholder="Please write your email for password reinitialisation"
            required
            autoComplete="email"
          />
        </div>
        <button
          type="submit"
          disabled={authLoading}
          className="w-full py-2 bg-blue-600 text-white rounded font-semibold hover:bg-blue-700 disabled:opacity-50"
        >
          {authLoading ? 'Sending...' : 'Send reset link'}
        </button>
      </form>
      <p className="mt-4 text-center text-sm text-gray-500">
        <button
          onClick={() => setAuthView('login')}
          className="text-blue-600 hover:underline"
        >
          Back to login
        </button>
      </p>
    </div>
  )
}

export default function AuthModal() {
  const { authModalOpen, authView, closeAuthModal, setAuthView, pendingInviteMessage } = useAuthStore()

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
            <h2 className="text-xl font-semibold mb-6 text-center">Welcome</h2>
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
                Create an account
              </button>
              <button
                onClick={() => setAuthView('login')}
                className="w-full py-3 bg-gray-100 text-gray-800 rounded-lg font-semibold hover:bg-gray-200"
              >
                Log in
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
