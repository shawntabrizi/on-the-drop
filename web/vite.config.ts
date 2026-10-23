import { defineConfig, type Plugin } from 'vitest/config';

// No external requests at run time: the built page allows only its own
// files (the wasm needs 'wasm-unsafe-eval'). Not in dev: Vite's dev client
// needs inline scripts and a websocket.
const CSP = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data: blob:",
  "media-src 'self' blob:",
  "worker-src 'self' blob:",
  "connect-src 'self'",
  "font-src 'self'",
  "object-src 'none'",
  "base-uri 'self'",
  "form-action 'none'",
].join('; ');

const csp: Plugin = {
  name: 'csp',
  apply: 'build',
  transformIndexHtml: (html) => html.replace('<meta charset="utf-8" />', `<meta charset="utf-8" />\n    <meta http-equiv="Content-Security-Policy" content="${CSP}" />`),
};

// BASE: the path the app is served under (GitHub Pages: /on-the-drop/).
export default defineConfig({
  base: process.env.BASE ?? '/',
  plugins: [csp],
  build: {
    target: 'es2022',
    chunkSizeWarningLimit: 1200,
  },
  worker: { format: 'es' },
  test: {
    environment: 'node',
    include: ['test/**/*.test.ts'],
    testTimeout: 60000,
  },
});
