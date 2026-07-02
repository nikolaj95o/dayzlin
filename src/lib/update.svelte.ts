// Shared app-update state so the Settings nav dot and the Settings panel agree on whether a newer
// release is available. Populated by a best-effort check on startup (App.svelte) and refreshed
// when the user clicks "Check for updates" in Settings.

import { checkForUpdates, type UpdateStatus } from "./api";

export const updateState = $state<{ status: UpdateStatus | null }>({ status: null });

// Whether the last check found a newer version. Reading it in a template is reactive, so it drives
// the dot on the Settings tab.
export function updateAvailable(): boolean {
  return updateState.status?.update_available ?? false;
}

// Run a check and store the result. Swallows errors — used on startup where a failed/offline check
// should stay silent rather than pop a dialog.
export async function refreshUpdateStatus(): Promise<UpdateStatus | null> {
  try {
    updateState.status = await checkForUpdates();
  } catch {
    // best-effort; keep whatever we had
  }
  return updateState.status;
}
