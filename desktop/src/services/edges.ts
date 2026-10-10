// Which helper edge this client keeps its realtime socket and chat on. The
// main server hands out the list (least loaded first); it's cached so the
// client can still move between edges while the main server is down.
import { api, mainServerUrl } from "./http";

export interface Edge {
  id: string;
  name: string;
  region: string;
  url: string;
  clients: number;
}

const CACHE_KEY = "warlay.edges";

let edges: Edge[] = load();
let index = 0;

function load(): Edge[] {
  try {
    return JSON.parse(localStorage.getItem(CACHE_KEY) ?? "[]") as Edge[];
  } catch {
    return [];
  }
}

/** Refreshes the list from the main server; keeps the cached one if it's down. */
export async function refreshEdges(): Promise<void> {
  try {
    const fresh = await api<Edge[]>("GET", "/edges");
    edges = fresh;
    index = 0;
    localStorage.setItem(CACHE_KEY, JSON.stringify(fresh));
  } catch {
    /* main server unreachable: keep what we had */
  }
}

/** Base URL for realtime and chat: the current edge, or the main server. */
export function edgeBase(): string {
  return edges.length ? edges[index % edges.length].url.replace(/\/+$/, "") : mainServerUrl();
}

export function currentEdge(): Edge | null {
  return edges.length ? edges[index % edges.length] : null;
}

/** Moves to the next edge after the current one failed. Returns false when
 * every edge was tried (the caller then falls back to the main server). */
export function nextEdge(): boolean {
  if (edges.length === 0) return false;
  index += 1;
  return index % edges.length !== 0;
}
