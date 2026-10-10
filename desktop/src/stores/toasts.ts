import { defineStore } from "pinia";
import { ref } from "vue";

export interface Toast {
  id: number;
  tone: "info" | "success" | "error";
  title: string;
  text?: string;
}

let nextId = 1;

export const useToasts = defineStore("toasts", () => {
  const items = ref<Toast[]>([]);

  function push(tone: Toast["tone"], title: string, text?: string, ttl = 4500) {
    const id = nextId++;
    items.value.push({ id, tone, title, text });
    setTimeout(() => dismiss(id), ttl);
  }

  function dismiss(id: number) {
    items.value = items.value.filter((t) => t.id !== id);
  }

  return {
    items,
    dismiss,
    info: (title: string, text?: string) => push("info", title, text),
    success: (title: string, text?: string) => push("success", title, text),
    error: (title: string, text?: string) => push("error", title, text, 6500),
  };
});
