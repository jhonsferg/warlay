import { defineStore } from "pinia";
import { ref } from "vue";

export interface ToastAction {
  label: string;
  /** "primary" is the accept-like action; "ghost" the decline-like one. */
  variant?: "primary" | "ghost";
  /** Runs the action; the toast closes once it settles (or fails). */
  run: () => unknown | Promise<unknown>;
}

export interface Toast {
  id: number;
  tone: "info" | "success" | "error";
  title: string;
  text?: string;
  /** Present on actionable toasts: they stay until answered or closed with the X. */
  actions?: ToastAction[];
}

let nextId = 1;

export const useToasts = defineStore("toasts", () => {
  const items = ref<Toast[]>([]);

  function push(tone: Toast["tone"], title: string, text?: string, ttl = 4500, actions?: ToastAction[]) {
    const id = nextId++;
    items.value.push({ id, tone, title, text, actions });
    setTimeout(() => dismiss(id), ttl);
    return id;
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
    /** An actionable toast (accept / decline). Closing it never answers the question. */
    action: (title: string, text: string | undefined, actions: ToastAction[], ttl = 20000) =>
      push("info", title, text, ttl, actions),
  };
});
