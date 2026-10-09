import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    // Dev-only: forwards /api/* to the Rust server so the browser sees
    // everything as same-origin, avoiding CORS entirely. In production
    // p1-web itself serves both the SPA and the API from one origin.
    proxy: {
      '/api': 'http://127.0.0.1:8080',
    },
  },
})
