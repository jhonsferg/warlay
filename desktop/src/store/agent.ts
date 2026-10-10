import { defineStore } from "pinia";
import { ref } from "vue";
import {
  getAgentSettings as apiGetAgentSettings,
  getAgentStatus as apiGetAgentStatus,
  retryPeerMesh as apiRetryPeerMesh,
  saveAgentSettings as apiSaveAgentSettings,
} from "../api";
import type { AgentSettings, AgentStatusSnapshot } from "../types";

export const useAgentStore = defineStore("agent", () => {
  const settings = ref<AgentSettings>({ logLevel: "info", lanDiscoveryEnabled: true });
  const status = ref<AgentStatusSnapshot | null>(null);
  const loading = ref(false);

  async function loadSettings(): Promise<void> {
    settings.value = await apiGetAgentSettings();
  }

  /** Applies immediately on the daemon (log level reload, LAN discovery
   * toggle) and persists — same instant-feeling pattern as the client's own
   * `settingsStore.setLogLevel`. */
  async function saveSettings(next: AgentSettings): Promise<void> {
    await apiSaveAgentSettings(next);
    settings.value = next;
  }

  /** Best-effort: a stale/unreachable daemon shouldn't throw and blank the
   * panel out from under a user just looking at it — leaves the last known
   * `status.value` in place instead. */
  async function refreshStatus(): Promise<void> {
    try {
      status.value = await apiGetAgentStatus();
    } catch {
      // keep the last known snapshot
    }
  }

  async function retryPeerMesh(): Promise<void> {
    loading.value = true;
    try {
      await apiRetryPeerMesh();
      await refreshStatus();
    } finally {
      loading.value = false;
    }
  }

  return { settings, status, loading, loadSettings, saveSettings, refreshStatus, retryPeerMesh };
});
