import { invoke } from "@tauri-apps/api/core";

export interface ServerMod {
  name: string;
  workshop_id: number;
}
export interface Server {
  name: string;
  ip: string;
  game_port: number;
  players: number;
  max_players: number;
  map: string;
  time: string;
  first_person: boolean;
  password: boolean;
  mods: ServerMod[];
  version: string;
  // Whether the server's build matches the installed DayZ; null when unknown.
  version_match: boolean | null;
}
export interface ServerFilter {
  map: string | null;
  first_person_only: boolean;
  third_person_only: boolean;
  no_password: boolean;
  max_mods: number | null;
  min_players: number | null;
  has_slots: boolean;
  same_version_only: boolean;
}
export interface ServerRef {
  name: string;
  ip: string;
  port: number;
}
export interface Profile {
  player: string;
  steam_root: string | null;
  favorites: ServerRef[];
  history: ServerRef[];
}

export interface CommandError {
  kind: string;
  message: string;
  detail: string | null;
}

// An installed workshop mod with size and last-used info; mirrors the Rust `InstalledModInfo`.
export interface InstalledMod {
  name: string;
  workshop_id: number;
  size_bytes: number;
  // Unix-seconds timestamp of last use (last launch that used it, else the mod folder's mtime);
  // null only when neither is known.
  last_used: number | null;
}

// Diagnostics for the Settings tab; mirrors the Rust `EnvReport` struct.
export interface EnvReport {
  app_version: string;
  steam_running: boolean;
  steam_found: boolean;
  steam_kind: string | null;
  steam_root: string | null;
  dayz_installed: boolean;
  dayz_path: string | null;
  dayz_version: string | null;
}

// Result of a leftover-download cleanup pass; mirrors the Rust `CleanupReport` struct.
export interface CleanupReport {
  steam_running: boolean;
  removed: number;
  pending: number;
}

// Mirrors the Rust `LaunchProgress` enum emitted on the `launch-progress` event.
export type LaunchProgress =
  | { phase: "preparing" }
  | {
      phase: "downloading";
      current: number;
      total: number;
      id: number;
      name: string;
      bytes: number;
      total_bytes: number | null;
    }
  | { phase: "launching" };

// Returns whether the served data is stale — i.e. the caller should trigger a background refresh.
export const listServers = (refresh: boolean) =>
  invoke<boolean>("list_servers", { refresh });
export const filterServers = (filter: ServerFilter, query: string) =>
  invoke<Server[]>("filter_servers", { filter, query });
export const play = (server: Server, password: string | null) =>
  invoke<void>("play", { server, password });
export const cancelPlay = () => invoke<void>("cancel_play");
export const getProfile = () => invoke<Profile>("get_profile");
export const saveProfile = (profile: Profile) =>
  invoke<void>("save_profile", { profile });
export const toggleFavorite = (serverRef: ServerRef) =>
  invoke<Profile>("toggle_favorite", { serverRef });
export const checkEnvironment = () =>
  invoke<EnvReport>("check_environment");
export const cleanupDownloads = () =>
  invoke<CleanupReport>("cleanup_downloads");
export const resolveDayzPath = (path: string) =>
  invoke<string>("resolve_dayz_path", { path });
export const listInstalledMods = () =>
  invoke<InstalledMod[]>("list_installed_mods");
export const deleteInstalledMod = (id: number) =>
  invoke<void>("delete_installed_mod", { id });
export const openWorkshopPage = (id: number) =>
  invoke<void>("open_workshop_page", { id });
export const openModFolder = (id: number) =>
  invoke<void>("open_mod_folder", { id });
