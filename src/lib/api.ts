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
}
export interface ServerFilter {
  map: string | null;
  first_person_only: boolean;
  no_password: boolean;
  max_mods: number | null;
  min_players: number | null;
  has_slots: boolean;
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

export const listServers = (refresh: boolean) =>
  invoke<Server[]>("list_servers", { refresh });
export const filterServers = (filter: ServerFilter, query: string) =>
  invoke<Server[]>("filter_servers", { filter, query });
export const play = (server: Server, password: string | null) =>
  invoke<void>("play", { server, password });
export const getProfile = () => invoke<Profile>("get_profile");
export const saveProfile = (profile: Profile) =>
  invoke<void>("save_profile", { profile });
