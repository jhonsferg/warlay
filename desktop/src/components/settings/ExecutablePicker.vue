<script setup lang="ts">
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "@lucide/vue";
import { ref, watch } from "vue";
import { pathExists } from "../../api";
import { debounce } from "../../utils/format";
import BaseButton from "../base/BaseButton.vue";
import BaseInput from "../base/BaseInput.vue";
import HintText from "../base/HintText.vue";

const path = defineModel<string>({ default: "" });

const status = ref("");
const statusError = ref(false);

const checkPath = debounce(async (candidate: string) => {
  if (!candidate.trim()) {
    status.value = "";
    return;
  }
  const exists = await pathExists(candidate);
  status.value = exists ? "Ejecutable encontrado." : "No se encontró un archivo en esa ruta.";
  statusError.value = !exists;
}, 300);

watch(path, (value) => checkPath(value));

async function browse(): Promise<void> {
  const selected = await openFileDialog({
    title: "Selecciona Frozen Throne.exe",
    multiple: false,
    directory: false,
    filters: [
      { name: "Warcraft III: Frozen Throne", extensions: ["exe"] },
      { name: "Todos los archivos", extensions: ["*"] },
    ],
  });
  if (typeof selected !== "string") return;
  path.value = selected;
}
</script>

<template>
  <label class="field"
    >Ejecutable del juego (Frozen Throne.exe)
    <div class="input-group">
      <BaseInput
        v-model="path"
        placeholder="C:\Juegos\Warcraft III\Frozen Throne.exe"
        test-id="settings-exe-input"
        aria-label="Ruta del ejecutable de Frozen Throne"
      />
      <BaseButton
        variant="secondary"
        :icon="FolderOpen"
        :icon-size="14"
        test-id="settings-browse-exe"
        aria-label="Buscar ejecutable del juego"
        @click="browse"
        >Buscar…</BaseButton
      >
    </div>
  </label>
  <HintText :text="status" :error="statusError" test-id="settings-exe-status" />
</template>
