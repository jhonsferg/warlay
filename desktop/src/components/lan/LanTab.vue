<script setup lang="ts">
import ExtraCidrInput from "./ExtraCidrInput.vue";
import GameList from "./GameList.vue";
import LaunchGameCard from "./LaunchGameCard.vue";
import RelayList from "./RelayList.vue";
import ScanControls from "./ScanControls.vue";
import SubnetList from "./SubnetList.vue";
import TunnelPanel from "./TunnelPanel.vue";
</script>

<template>
  <section class="lan-view" data-testid="lan-view">
    <div class="grid">
      <div class="card">
        <h2>Subredes</h2>
        <SubnetList />
        <ExtraCidrInput />
        <ScanControls />
      </div>

      <div class="card">
        <h2>Servidores de relay</h2>
        <RelayList />
      </div>

      <div class="card">
        <h2>Partidas observadas</h2>
        <GameList />
      </div>

      <div class="card">
        <h2>Túnel local</h2>
        <TunnelPanel />
      </div>

      <div class="card">
        <h2>Lanzar juego</h2>
        <LaunchGameCard />
      </div>
    </div>
  </section>
</template>

<style scoped lang="scss">
.lan-view {
  // CSS multi-column instead of CSS grid, same reasoning and same fix as
  // SettingsView.vue: this view's five cards are wildly uneven heights
  // (Subredes -- with its own list, CIDR input and scan controls -- vs.
  // the one-line Lanzar juego card), and a grid row is only ever as tall
  // as its shortest-fitting content, so short cards sitting in the same
  // row as a tall one just leave dead space beneath them. Multi-column
  // balances cards into columns by actual height instead. And, same bug
  // as before: no height/overflow of its own here -- a block-size-
  // constrained multi-column box spawns extra columns past its own width
  // instead of scrolling, which is what caused this view to overflow
  // horizontally. Left auto-height, it only grows vertically and scrolls
  // via .shell__main-content (App.vue).
  .grid {
    column-width: 360px;
    column-gap: $spacing-lg;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: $spacing-md;
    break-inside: avoid;
    margin: 0 0 $spacing-lg;
  }
}
</style>
