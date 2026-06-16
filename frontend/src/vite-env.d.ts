/// <reference types="vite/client" />

declare global {
  interface Window {
    __RUNTIME_CONFIG__?: {
      VITE_API_BASE_URL?: string;
    };
  }
}

export {}
