import { defineStore } from "pinia";
import { ref } from "vue";
import { discoverRelays, launchGame, listGames, startLocalRelay, stopLocalRelay, watchGames } from "../api";
import type { DetectedSubnet, GameInfo, RelayInfo } from "../types";
import { relayIp } from "../types";
import { buildLaunchArgs } from "../utils/launch";
import { useSettingsStore } from "./settings";

/** LAN-mesh legacy mode: the original broadcast-reflection peer mesh
 * between physically isolated subnets. Fully self-contained — touches
 * nothing else in the app besides reading `settings` for the configured
 * game executable. All Map/Set fields are `ref`s (not `reactive`) uniformly,
 * even the ones only ever mutated in place, so wholesale reassignment from
 * outside (e.g. `lanMeshStore.selectedSubnetCidrs = new Set(...)` in
 * `bootstrap.ts`) always correctly propagates into this store's own
 * closures too — a plain `reactive()` local wouldn't. */
export const useLanMeshStore = defineStore("lanMesh", () => {
  const settingsStore = useSettingsStore();

  const subnets = ref<DetectedSubnet[]>([]);
  const selectedSubnetCidrs = ref(new Set<string>());
  const extraCidrs = ref<string[]>([]);
  const controlPort = ref(6116);
  const discoveryTimeoutMs = ref(1500);
  const relays = ref(new Map<string, RelayInfo>());
  const tunnelPeerAddrs = ref(new Set<string>());
  const selectedRelayAddr = ref<string | null>(null);
  const games = ref<GameInfo[]>([]);
  const bindPort = ref(6112);
  const tunnelRunning = ref(false);

  function selectedCidrs(): string[] {
    return [...selectedSubnetCidrs.value, ...extraCidrs.value];
  }

  function toggleSubnetCidr(cidr: string, checked: boolean): void {
    if (checked) selectedSubnetCidrs.value.add(cidr);
    else selectedSubnetCidrs.value.delete(cidr);
  }

  function addExtraCidr(cidr: string): void {
    const value = cidr.trim();
    if (!value || extraCidrs.value.includes(value)) return;
    extraCidrs.value.push(value);
  }

  function removeExtraCidr(cidr: string): void {
    extraCidrs.value = extraCidrs.value.filter((entry) => entry !== cidr);
  }

  function toggleTunnelPeer(addr: string, checked: boolean): void {
    if (checked) tunnelPeerAddrs.value.add(addr);
    else tunnelPeerAddrs.value.delete(addr);
  }

  async function runScan(): Promise<number> {
    const cidrs = selectedCidrs();
    if (cidrs.length === 0) {
      throw new Error("Selecciona al menos una subred o añade una manualmente.");
    }
    relays.value.clear();
    const found = await discoverRelays(cidrs, controlPort.value || 6116, discoveryTimeoutMs.value || 1500);
    return found.length;
  }

  async function selectRelay(addr: string): Promise<void> {
    selectedRelayAddr.value = addr;
    games.value = [];

    const relay = relays.value.get(addr);
    if (!relay) return;
    const ip = relayIp(relay);
    const port = controlPort.value || 6116;

    games.value = await listGames(ip, port);
    await watchGames(ip, port, 4000);
  }

  async function startTunnel(): Promise<void> {
    const bindAddr = `0.0.0.0:${bindPort.value || 6112}`;
    const peers = [...tunnelPeerAddrs.value].map((addr) => {
      const relay = relays.value.get(addr)!;
      return `${relayIp(relay)}:${relay.relayPort}`;
    });
    if (peers.length === 0) {
      throw new Error("Marca al menos un relay para usar como peer del túnel.");
    }
    await startLocalRelay(bindAddr, peers);
  }

  async function stopTunnel(): Promise<void> {
    await stopLocalRelay();
  }

  async function launchConfiguredGame(): Promise<{ pid: number }> {
    const path = settingsStore.settings.gameExecutablePath.trim();
    if (!path) {
      throw new Error("Configura el ejecutable en la sección de Ajustes primero.");
    }
    const args = buildLaunchArgs(settingsStore.settings);
    return launchGame(path, args, null);
  }

  return {
    subnets,
    selectedSubnetCidrs,
    extraCidrs,
    controlPort,
    discoveryTimeoutMs,
    relays,
    tunnelPeerAddrs,
    selectedRelayAddr,
    games,
    bindPort,
    tunnelRunning,
    toggleSubnetCidr,
    addExtraCidr,
    removeExtraCidr,
    toggleTunnelPeer,
    runScan,
    selectRelay,
    startTunnel,
    stopTunnel,
    launchConfiguredGame,
  };
});
