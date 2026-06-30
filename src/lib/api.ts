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
  steam_login: string;
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

// Diagnostics for the Settings tab; mirrors the Rust `EnvReport` struct.
export interface EnvReport {
  app_version: string;
  steamcmd_installed: boolean;
  terminal: string | null;
  steam_found: boolean;
  steam_kind: string | null;
  steam_root: string | null;
  dayz_installed: boolean;
  dayz_path: string | null;
  dayz_version: string | null;
  steam_login: string;
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

export const listServers = (refresh: boolean) =>
  invoke<Server[]>("list_servers", { refresh });
export const setupSteamLogin = () => invoke<void>("setup_steam_login");
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
