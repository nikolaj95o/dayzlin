import { mount } from 'svelte'
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
