<script setup lang="ts">
import { Radio, ShieldCheck, Users } from "@lucide/vue";
import WLogo from "../atoms/WLogo.vue";
</script>

<template>
  <div class="auth-layout">
    <section class="auth-layout__hero" aria-hidden="true">
      <div class="auth-layout__glow auth-layout__glow--a" />
      <div class="auth-layout__glow auth-layout__glow--b" />
      <div class="auth-layout__grid" />
      <div class="auth-layout__copy">
        <WLogo :size="56" wordmark />
        <h1>Tu LAN, sin fronteras.</h1>
        <p>
          Juega Warcraft III en red local con tus amigos estén donde estén. Conexión directa entre jugadores, sin VPN ni
          puertos.
        </p>
        <ul>
          <li><Radio /> Conexión P2P directa: menos latencia para todos</li>
          <li><Users /> Amigos, comunidades y salas en tiempo real</li>
          <li><ShieldCheck /> Cifrado de extremo a extremo entre jugadores</li>
        </ul>
      </div>
    </section>
    <section class="auth-layout__panel">
      <div class="auth-layout__card">
        <slot />
      </div>
    </section>
  </div>
</template>

<style scoped lang="scss">
.auth-layout {
  display: grid;
  grid-template-columns: 1.1fr 1fr;
  height: 100vh;
  background: var(--bg-0);

  &__hero {
    position: relative;
    overflow: hidden;
    display: flex;
    align-items: center;
    padding: var(--s-12);
    background: linear-gradient(160deg, #0d0b1f 0%, #090f1c 60%, #07090e 100%);
    border-right: 1px solid var(--line);
  }
  &__glow {
    position: absolute;
    border-radius: 50%;
    filter: blur(80px);
    opacity: 0.55;
    &--a {
      width: 520px;
      height: 520px;
      left: -160px;
      top: -120px;
      background: #5b3fff;
    }
    &--b {
      width: 420px;
      height: 420px;
      right: -140px;
      bottom: -140px;
      background: #0891b2;
    }
  }
  &__grid {
    position: absolute;
    inset: 0;
    background-image: linear-gradient(rgba(255, 255, 255, 0.04) 1px, transparent 1px),
      linear-gradient(90deg, rgba(255, 255, 255, 0.04) 1px, transparent 1px);
    background-size: 44px 44px;
    mask-image: radial-gradient(ellipse at center, #000 30%, transparent 75%);
  }
  &__copy {
    position: relative;
    max-width: 480px;
    h1 {
      @include display-font(52px);
      margin: var(--s-8) 0 var(--s-4);
      background: linear-gradient(90deg, #fff, #c9c0ff 60%, #8be9ff);
      -webkit-background-clip: text;
      background-clip: text;
      color: transparent;
    }
    p {
      color: var(--text-2);
      font-size: var(--fs-lg);
      margin: 0 0 var(--s-8);
    }
    ul {
      list-style: none;
      padding: 0;
      margin: 0;
      display: flex;
      flex-direction: column;
      gap: var(--s-3);
    }
    li {
      display: flex;
      align-items: center;
      gap: var(--s-3);
      color: var(--text-1);
      svg {
        width: 20px;
        height: 20px;
        color: var(--accent-2);
      }
    }
  }

  &__panel {
    display: grid;
    place-items: center;
    padding: var(--s-8);
    overflow-y: auto;
  }
  &__card {
    width: 100%;
    max-width: 400px;
  }

  @media (max-width: 920px) {
    grid-template-columns: 1fr;
    &__hero {
      display: none;
    }
  }
}
</style>

<!-- Shared by every page rendered inside this layout (login, register). -->
<style lang="scss">
.auth-form {
  display: flex;
  flex-direction: column;
  gap: var(--s-5);

  header h2 {
    @include display-font(var(--fs-3xl));
    margin: 4px 0 var(--s-2);
  }
  &__eyebrow {
    @include eyebrow;
    color: var(--accent-2);
    margin: 0;
  }
  &__error {
    margin: 0;
    padding: var(--s-3) var(--s-4);
    border-radius: var(--r-sm);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: var(--fs-sm);
  }
  &__switch {
    text-align: center;
    color: var(--text-3);
    margin: 0;
    a {
      font-weight: 600;
    }
  }
}
</style>
