import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  server: {
    host: '127.0.0.1', port: 1420, strictPort: true,
    // WSL's /mnt filesystem does not reliably report edits made by Windows tools.
    watch: { usePolling: Boolean(process.env.WSL_DISTRO_NAME) },
  },
  clearScreen: false,
  envPrefix: ['VITE_'],
  test: { environment: 'jsdom', include: ['src/**/*.test.ts'], clearMocks: true },
})
