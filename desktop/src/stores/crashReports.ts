import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { isDesktop, native, type CrashSummary } from "../services/native";

/** Crash reports left on this machine by an earlier run, waiting for the
 * player to send them, copy them or throw them away. */
export const useCrashReports = defineStore("crashReports", () => {
  const reports = ref<CrashSummary[]>([]);
  /** Hidden for this run with "Ahora no": they stay on disk for the next one. */
  const postponed = ref(false);
  const current = computed(() => (postponed.value ? null : (reports.value[0] ?? null)));
  const detail = ref<string | null>(null);

  async function load() {
    if (!isDesktop) return;
    reports.value = await native.listCrashReports();
  }

  /** The full text of the report being shown. */
  async function loadDetail() {
    detail.value = current.value ? ((await native.readCrashReport(current.value.id)) ?? "") : null;
  }

  async function discard() {
    const report = current.value;
    if (!report) return;
    await native.deleteCrashReport(report.id);
    reports.value = reports.value.filter((r) => r.id !== report.id);
    detail.value = null;
  }

  /** Opens the prefilled GitHub issue; the report is discarded once the player has it. */
  async function sendToGitHub() {
    const report = current.value;
    if (!report) return;
    await native.reportCrashOnGitHub(report.id);
  }

  function postpone() {
    postponed.value = true;
    detail.value = null;
  }

  return { reports, current, detail, load, loadDetail, discard, sendToGitHub, postpone };
});
