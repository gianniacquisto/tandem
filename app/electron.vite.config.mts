import { resolve } from 'node:path'

import react from '@vitejs/plugin-react'
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'
import type { Plugin } from 'vite'

import { contentSecurityPolicy } from './src/shared/security'

/**
 * Three builds, one config. Main and preload are bundled for Node with their
 * dependencies left external; the renderer is a plain Vite app rooted at
 * `renderer/`, so the generated wire types in `renderer/src/wire` stay where
 * PRT-11 put them.
 *
 * The policy is injected into the built page and set as a response header by
 * the main process; development is the one loosening (the refresh preamble and
 * the dev socket), and the packaged page carries none.
 */
const policy: Plugin = {
  name: 'tandem-csp',
  transformIndexHtml: {
    order: 'post',
    handler(html, context) {
      if (context.server !== undefined) return html
      const meta = `<meta http-equiv="Content-Security-Policy" content="${contentSecurityPolicy(false)}" />`
      return html.replace('</head>', `    ${meta}\n  </head>`)
    },
  },
}

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
  },
  renderer: {
    root: resolve('renderer'),
    plugins: [react(), policy],
    build: {
      rollupOptions: {
        input: resolve('renderer/index.html'),
      },
    },
  },
})
