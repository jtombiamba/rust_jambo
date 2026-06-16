const runtimeConfig = typeof window !== "undefined" ? window.__RUNTIME_CONFIG__ : undefined

export function getApiBaseUrl(): string {
  return (
    runtimeConfig?.VITE_API_BASE_URL ||
    import.meta.env.VITE_API_BASE_URL ||
    "http://backend:5000"
  )
}

export function getWsUrl(wsPath: string): string {
  const apiBaseUrl = getApiBaseUrl()

  if (apiBaseUrl) {
    let host: string
    try {
      const parsed = new URL(apiBaseUrl)
      host = parsed.host
    } catch {
      host = apiBaseUrl.replace(/^https?:\/\//, "")
    }
    const wsProtocol = apiBaseUrl.startsWith("https") ? "wss:" : "ws:"
    return `${wsProtocol}//${host}${wsPath}`
  }

  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:"
  return `${protocol}//${window.location.host}${wsPath}`
}
