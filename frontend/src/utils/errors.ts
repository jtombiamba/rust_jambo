import axios from 'axios'

export interface ApiError {
  success: false
  error: string
  code?: string
  field?: string
  source: string
  request_id?: string
}

export function extractApiError(err: unknown): { message: string; code?: string; source?: string; requestId?: string } {
  if (axios.isAxiosError(err) && err.response?.data) {
    const data = err.response.data
    return {
      message: data.error || '',
      code: data.code,
      source: data.source,
      requestId: data.request_id,
    }
  }
  else if (axios.isAxiosError(err)) {
    const data = err.code;
    return {
      message: data || '',
    }
  }
  return { message: '' }
}
