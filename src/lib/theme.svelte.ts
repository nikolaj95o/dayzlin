// Theme preference: Light / Dark / System, persisted in localStorage (no Rust/Profile change).
// The actual dark styling is driven by a `.dark` class on <html> (see app.css @custom-variant).

export type ThemePref = "light" | "dark" | "system";

const KEY = "theme";

export function getThemePref(): ThemePref {
  const v = localStorage.getItem(KEY);
  return v === "light" || v === "dark" || v === "system" ? v : "system";
}

const prefersDark = () =>
  window.matchMedia("(prefers-color-scheme: dark)").matches;

// Toggle the .dark class to match a preference (resolving "system" against the OS).
function resolve(pref: ThemePref): boolean {
  return pref === "dark" || (pref === "system" && prefersDark());
}

// Reactive store so Settings can bind a selector to the current preference.
export const theme = $state<{ pref: ThemePref }>({ pref: "system" });

// Apply a preference: persist it, update the store, and toggle the class. The matchMedia
// listener installed by initTheme() re-applies automatically while the pref is "system".
export function setTheme(pref: ThemePref) {
  theme.pref = pref;
  localStorage.setItem(KEY, pref);
  document.documentElement.classList.toggle("dark", resolve(pref));
}

// Call once at startup: sync the store from storage and follow OS changes in "system" mode.
export function initTheme() {
  theme.pref = getThemePref();
  document.documentElement.classList.toggle("dark", resolve(theme.pref));
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => {
      if (theme.pref === "system")
        document.documentElement.classList.toggle("dark", prefersDark());
    });
}
