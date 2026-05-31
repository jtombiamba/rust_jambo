import axios from 'axios'

export interface ApiError {
  success: false
  error: string
  field?: string
  source: string
  request_id?: string
}

export function extractApiError(err: unknown): { message: string; source?: string; requestId?: string } {
  if (axios.isAxiosError(err) && err.response?.data) {
    const data = err.response.data
    return {
      message: data.error || '',
      source: data.source,
      requestId: data.request_id,
    }
  }
  return { message: '' }
}
