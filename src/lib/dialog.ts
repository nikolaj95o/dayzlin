import { writable } from "svelte/store";
import type { CommandError } from "./api";

export interface DialogMessage {
  title: string;
  message: string;
  detail?: string | null;
  kind?: string;
}

export const dialog = writable<DialogMessage | null>(null);

export function showMessage(m: DialogMessage) {
  dialog.set(m);
}

export function closeDialog() {
  dialog.set(null);
}

function titleFor(kind?: string): string {
  switch (kind) {
    case "steam_not_downloading":
    case "download_stalled":
      return "Mod download problem";
    case "steam_not_found":
      return "Steam not found";
    case "network":
      return "Network error";
    default:
      return "Error";
  }
}

// Normalize an unknown thrown value (CommandError | Error | string) into a dialog.
export function showError(e: unknown) {
  const ce = e as Partial<CommandError>;
  if (ce && typeof ce === "object" && "kind" in ce && "message" in ce) {
    dialog.set({
      title: titleFor(ce.kind),
      message: ce.message ?? "",
      detail: ce.detail ?? null,
      kind: ce.kind,
    });
  } else {
    dialog.set({ title: "Error", message: String(e), detail: null });
  }
}
