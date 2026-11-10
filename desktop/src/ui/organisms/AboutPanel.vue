<script setup lang="ts">
import { onMounted, ref } from "vue";
import { copyrightLine, formatDate, type AppInfo } from "../../services/about";
import { isDesktop, native } from "../../services/native";
import WLogo from "../atoms/WLogo.vue";
import { t } from "../../services/i18n";

const info = ref<AppInfo | null>(null);

onMounted(async () => {
  if (!isDesktop) return;
  try {
    info.value = (await native.appInfo()) ?? null;
  } catch {
    info.value = null;
  }
});
</script>

<template>
  <section class="about" :aria-label="t('Acerca de Warlay')" data-testid="about">
    <WLogo :size="56" />
    <div class="about__body">
      <h3 data-testid="about-name">{{ info?.name ?? "Warlay" }}</h3>
      <dl class="about__facts">
        <div>
          <dt>{{ t("Versión") }}</dt>
          <dd data-testid="about-version">{{ info?.version ?? "—" }}</dd>
        </div>
        <div>
          <dt>{{ t("Build") }}</dt>
          <dd data-testid="about-build">{{ info?.build ?? "—" }}</dd>
        </div>
        <div>
          <dt>{{ t("Fecha de lanzamiento") }}</dt>
          <dd data-testid="about-released">{{ formatDate(info?.releasedAt) }}</dd>
        </div>
        <div>
          <dt>{{ t("Última actualización") }}</dt>
          <dd data-testid="about-installed">{{ formatDate(info?.installedAt) }}</dd>
        </div>
      </dl>
      <p class="about__rights" data-testid="about-rights">{{ copyrightLine(new Date().getFullYear()) }}</p>
    </div>
  </section>
</template>

<style scoped lang="scss">
.about {
  @include card(var(--s-6));
  display: flex;
  align-items: center;
  gap: var(--s-6);

  &__body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--s-3);

    h3 {
      margin: 0;
      @include display-font(var(--fs-2xl));
    }
  }
  &__facts {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-3);

    div {
      flex: 1 1 170px;
      display: flex;
      flex-direction: column;
      gap: 2px;
      padding: var(--s-3);
      border-radius: var(--r-md);
      background: var(--bg-2);
    }
    dt {
      @include eyebrow;
    }
    dd {
      margin: 0;
      font-weight: 600;
    }
  }
  &__rights {
    margin: 0;
    color: var(--text-3);
    font-size: var(--fs-sm);
  }

  @container (max-width: 520px) {
    flex-direction: column;
    align-items: flex-start;
  }
}
</style>
