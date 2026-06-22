import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: './', // Ensures relative paths for Tauri loading from file://
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    // Target modern engines supported by Tauri (Chromium/WebKit)
    target: 'es2024',
    minify: 'esbuild',
    sourcemap: false,
  },
});
