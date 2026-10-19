<script setup lang="ts">
import { ref } from "vue";

const emit = defineEmits<{ pick: [emoji: string] }>();

const GROUPS: { name: string; icon: string; emojis: string[] }[] = [
  {
    name: "Caras",
    icon: "😀",
    emojis:
      "😀 😁 😂 🤣 😊 😍 😘 😎 🤩 🥳 😏 😅 😉 🙂 🙃 😋 😛 🤔 🤨 😐 😑 😴 😮 😱 😭 😢 😤 😡 🤯 🥶 🥵 🤢 😇 🤠 🤗 🫡".split(
        " ",
      ),
  },
  {
    name: "Gestos",
    icon: "👍",
    emojis: "👍 👎 👌 ✌️ 🤞 🤝 👏 🙌 🙏 💪 👊 ✊ 🤙 👋 🫶 🤘 ☝️ 👀 🧠 🫠".split(" "),
  },
  {
    name: "Juego",
    icon: "🎮",
    emojis: "🎮 🕹️ ⚔️ 🛡️ 🏹 🗡️ 💣 🔥 ⚡ 💥 ❄️ 🏆 🥇 🥈 🥉 💀 👑 💎 🎯 🎲 🧙 🧟 🐉 🐺 🦅".split(" "),
  },
  {
    name: "Símbolos",
    icon: "❤️",
    emojis: "❤️ 🧡 💛 💚 💙 💜 🖤 💔 ✨ 🎉 🎊 ✅ ❌ ❗ ❓ 💯 🔔 ⭐ 🌟 🚀".split(" "),
  },
];

const active = ref(0);
</script>

<template>
  <div class="emoji-picker" role="dialog" aria-label="Emojis">
    <nav class="emoji-picker__tabs">
      <button
        v-for="(group, i) in GROUPS"
        :key="group.name"
        type="button"
        class="emoji-picker__tab"
        :class="{ 'emoji-picker__tab--active': i === active }"
        :aria-label="group.name"
        :title="group.name"
        @click="active = i"
      >
        {{ group.icon }}
      </button>
    </nav>
    <div class="emoji-picker__grid">
      <button
        v-for="emoji in GROUPS[active].emojis"
        :key="emoji"
        type="button"
        class="emoji-picker__emoji"
        :aria-label="emoji"
        @click="emit('pick', emoji)"
      >
        {{ emoji }}
      </button>
    </div>
  </div>
</template>

<style scoped lang="scss">
.emoji-picker {
  width: 296px;
  padding: var(--s-2);
  background: var(--bg-3);
  border: 1px solid var(--line-2);
  border-radius: var(--r-md);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);

  &__tabs {
    display: flex;
    gap: var(--s-1);
    padding-bottom: var(--s-2);
    border-bottom: 1px solid var(--line-1, var(--line-2));
  }
  &__tab,
  &__emoji {
    all: unset;
    cursor: pointer;
    display: grid;
    place-items: center;
    border-radius: var(--r-sm);
    &:hover,
    &:focus-visible {
      background: rgba(255, 255, 255, 0.08);
    }
  }
  &__tab {
    width: 32px;
    height: 32px;
    font-size: 18px;
    opacity: 0.6;
    &--active {
      opacity: 1;
      background: rgba(255, 255, 255, 0.06);
    }
  }
  &__grid {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 2px;
    padding-top: var(--s-2);
    max-height: 180px;
    overflow-y: auto;
  }
  &__emoji {
    height: 32px;
    font-size: 20px;
  }
}
</style>
