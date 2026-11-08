<script setup lang="ts">
import { computed } from "vue";
import { GAME_ARG_GROUPS, GAME_ARGS, parseGameArgs, setGameArg, withoutExtraArgs } from "../../services/gameArgs";
import WButton from "../atoms/WButton.vue";
import WSwitch from "../atoms/WSwitch.vue";

/** The launch arguments as the string passed to the game. */
const model = defineModel<string>({ default: "" });

const parsed = computed(() => parseGameArgs(model.value));
const groups = computed(() =>
  GAME_ARG_GROUPS.map((group) => ({ ...group, args: GAME_ARGS.filter((a) => a.group === group.id) })),
);
</script>

<template>
  <div class="game-args" data-testid="game-args">
    <p class="game-args__intro">Activa las opciones con las que quieres que se abra Warcraft III.</p>

    <section v-for="group in groups" :key="group.id" class="game-args__group">
      <h4>{{ group.label }}</h4>
      <div v-for="arg in group.args" :key="arg.id" class="game-args__row">
        <div class="game-args__text">
          <span class="game-args__label">{{ arg.label }}</span>
          <span class="game-args__description">{{ arg.description }}</span>
          <code class="game-args__flag">{{ arg.tokens.join(" ") }}</code>
        </div>
        <WSwitch
          :model-value="parsed.enabled.has(arg.id)"
          :label="arg.label"
          :data-testid="`game-arg-${arg.id}`"
          @update:model-value="model = setGameArg(model, arg.id, $event)"
        />
      </div>
    </section>

    <div v-if="parsed.extra.length" class="game-args__extra" data-testid="game-args-extra">
      <p>
        Hay argumentos guardados que no tienen interruptor y se siguen enviando al juego:
        <code>{{ parsed.extra.join(" ") }}</code>
      </p>
      <WButton size="sm" variant="secondary" @click="model = withoutExtraArgs(model)">Quitarlos</WButton>
    </div>
  </div>
</template>

<style scoped lang="scss">
.game-args {
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  &__intro {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
  &__group {
    display: flex;
    flex-direction: column;
    gap: var(--s-2);

    h4 {
      @include eyebrow;
      margin: 0;
      color: var(--text-2);
    }
  }
  &__row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-4);
    padding: var(--s-3) var(--s-4);
    background: var(--bg-2);
    border-radius: var(--r-md);
  }
  &__text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  &__label {
    font-weight: 600;
  }
  &__description {
    color: var(--text-3);
    font-size: var(--fs-sm);
  }
  &__flag {
    align-self: flex-start;
    margin-top: 2px;
    padding: 1px 6px;
    color: var(--text-2);
    font-size: var(--fs-xs, 12px);
    background: var(--bg-4);
    border-radius: var(--r-sm);
  }
  &__extra {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s-3);
    padding: var(--s-3) var(--s-4);
    color: var(--text-2);
    font-size: var(--fs-sm);
    background: var(--bg-2);
    border: 1px dashed var(--line-2);
    border-radius: var(--r-md);

    p {
      margin: 0;
    }
  }
}
</style>
