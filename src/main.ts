import { mount } from 'svelte'
// Self-hosted fonts (bundled by Vite into dist/) — the app runs offline in a Flatpak sandbox, so
// fonts must never be fetched from a CDN. Chakra Petch = display/wordmark/nav; IBM Plex Sans = UI
// body; IBM Plex Mono = tabular data (player counts, versions, sizes). Only the used weights.
import '@fontsource/chakra-petch/500.css'
import '@fontsource/chakra-petch/600.css'
import '@fontsource/ibm-plex-sans/400.css'
import '@fontsource/ibm-plex-sans/500.css'
import '@fontsource/ibm-plex-mono/400.css'
import '@fontsource/ibm-plex-mono/500.css'
import './app.css'
import App from './App.svelte'
import { initTheme } from './lib/theme.svelte'

initTheme()

// Suppress the webview's default context menu (Reload / Inspect Element / …).
document.addEventListener('contextmenu', (e) => e.preventDefault())

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
