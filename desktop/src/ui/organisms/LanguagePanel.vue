<script setup lang="ts">
import { Languages } from "@lucide/vue";
import { LOCALES, locale, setLocale, t } from "../../services/i18n";
import WButton from "../atoms/WButton.vue";

/** Picks the language of the app; it applies at once and is remembered. */
</script>

<template>
  <section class="language" data-testid="language-card">
    <h3><Languages /> {{ t("Idioma") }}</h3>
    <p class="language__muted">{{ t("El idioma de los menús y avisos de la aplicación.") }}</p>
    <div class="language__choices" role="radiogroup" :aria-label="t('Idioma')">
      <WButton
        v-for="l in LOCALES"
        :key="l.id"
        :variant="locale === l.id ? 'primary' : 'secondary'"
        role="radio"
        :aria-checked="locale === l.id"
        :data-testid="`language-${l.id}`"
        @click="setLocale(l.id)"
      >
        {{ l.label }}
      </WButton>
    </div>
  </section>
</template>

<style scoped lang="scss">
.language {
  @include card(var(--s-6));
  display: flex;
  flex-direction: column;
  gap: var(--s-4);

  h3 {
    display: flex;
    align-items: center;
    gap: var(--s-2);
    margin: 0;
    @include display-font(var(--fs-xl));
    svg {
      width: 20px;
      height: 20px;
      color: var(--accent);
    }
  }

  &__muted {
    margin: 0;
    color: var(--text-3);
  }

  &__choices {
    display: flex;
    gap: var(--s-2);
  }
}
</style>
