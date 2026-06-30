import { writable } from "svelte/store";
import type { LaunchProgress } from "./api";

// Backend phases plus the frontend-only "starting" phase shown briefly after the
// `play` command resolves (Steam has been handed off; DayZ is coming up).
export type LaunchState = LaunchProgress | { phase: "starting" };

// null = idle / dialog closed (UI unblocked).
export const launch = writable<LaunchState | null>(null);

export function startLaunch() {
  launch.set({ phase: "preparing" });
}

export function setLaunch(state: LaunchState) {
  launch.set(state);
}

export function closeLaunch() {
  launch.set(null);
}
