/// <reference types="vite/client" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from 'tailwindcss'
import autoprefixer from 'autoprefixer'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  css: {
    postcss: {
      plugins: [tailwindcss, autoprefixer],
    },
  },
  server: {
    proxy: {
      '/api': {
        target: process.env.VITE_PROXY_API_TARGET || 'http://backend:5000',
        changeOrigin: true,
      },
      '/ws': {
        target: process.env.VITE_PROXY_WS_TARGET || 'ws://backend:5000',
        ws: true,
      },
    },
  },
})
