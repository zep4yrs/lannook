<script setup lang="ts">
import { computed, onMounted, shallowRef } from "vue";
import { FolderOpen, FileText, FileImage, FileVideo, FileArchive, FolderSearch, Trash2 } from "lucide-vue-next";
import { useSettingsStore } from "@/stores/settings";
import { useTransfersStore } from "@/stores/transfers";
import { useDevicesStore } from "@/stores/devices";
import { useAppStore } from "@/stores/app";
import { isTauri, openReceiveFolder, revealInFolder } from "@/services/tauri";
import { formatBytes } from "@/utils/format";
import { useLocale } from "@/i18n";
import ConfirmDialog from "@/components/common/ConfirmDialog.vue";

const settingsStore = useSettingsStore();
const transfersStore = useTransfersStore();
const devicesStore = useDevicesStore();
const appStore = useAppStore();
const { t } = useLocale();

interface ReceivedFile {
  id: string;
  name: string;
  size: string;
  date: string;
  source: string;
  savePath?: string;
}

// audit-22: per-row destructive action needs confirmation too.
const deleteTarget = shallowRef<ReceivedFile | null>(null);
const busyId = shallowRef<string | null>(null);

function deviceName(id: string): string {
  if (id === "local") return t("device.thisDevice");
  return devicesStore.devices.find((d) => d.id === id)?.name ?? id;
}

function formatDateTime(iso?: string): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours()
  )}:${pad(d.getMinutes())}`;
}

// Files this host received: completed uploads from devices to the host.
const receivedFiles = computed<ReceivedFile[]>(() => {
  const received = transfersStore.transfers.filter(
    (t) => t.direction === "upload_to_host" && t.status === "completed"
  );
  if (received.length > 0) {
    return received
      .slice()
      .sort(
        (a, b) =>
          new Date(b.completedAt ?? b.createdAt).getTime() -
          new Date(a.completedAt ?? a.createdAt).getTime()
      )
      .map((transfer) => ({
        id: transfer.id,
        name: transfer.files[0]?.name ?? t("received.unnamed"),
        size: formatBytes(transfer.totalBytes),
        date: formatDateTime(transfer.completedAt ?? transfer.createdAt),
        source: deviceName(transfer.sourceDeviceId),
        savePath: transfer.savePath,
      }));
  }
  return [];
});

async function handleOpenReceiveFolder() {
  if (!isTauri()) {
    appStore.pushToast("info", t("received.desktopOnly"), t("received.desktopOnlyDescription"));
    return;
  }
  try {
    await openReceiveFolder();
  } catch (err) {
    console.error("[received] Failed to open receive folder:", err);
    appStore.pushToast("error", t("received.openFailed"), t("received.openFailedDescription"));
  }
}

function getFileIcon(name: string) {
  if (/\.(png|jpg|jpeg|gif|svg|webp)$/i.test(name)) return FileImage;
  if (/\.(mp4|mov|avi|mkv)$/i.test(name)) return FileVideo;
  if (/\.(zip|rar|7z|tar|gz)$/i.test(name)) return FileArchive;
  return FileText;
}

/** Reveal the received file in the platform file manager (audit-22). */
async function handleReveal(file: ReceivedFile) {
  if (!isTauri()) {
    appStore.pushToast("info", t("received.desktopOnly"), t("received.desktopOnlyDescription"));
    return;
  }
  busyId.value = file.id;
  try {
    const result = await revealInFolder(file.savePath ?? settingsStore.receiveFolder);
    if (!result.success) throw new Error(result.error ?? "reveal failed");
  } catch (err) {
    console.error("[received] Failed to reveal file:", err);
    appStore.pushToast("error", t("received.openFailed"), t("received.openFailedDescription"));
  } finally {
    busyId.value = null;
  }
}

async function confirmDelete() {
  const file = deleteTarget.value;
  deleteTarget.value = null;
  if (!file) return;
  await transfersStore.removeTransfers([file.id]);
}

onMounted(() => {
  void transfersStore.fetchTransfers();
});
</script>

<template>
  <div class="received-page">
    <header class="page-header">
      <div class="header-left">
        <h1 class="page-title">{{ t("received.title") }}</h1>
        <p class="page-subtitle">{{ settingsStore.receiveFolder }}</p>
      </div>
      <button class="outline-btn" @click="handleOpenReceiveFolder">
        <FolderOpen :size="15" />
        {{ t("received.openFolder") }}
      </button>
    </header>

    <div class="received-card">
      <div v-if="receivedFiles.length === 0" class="received-empty">
        {{ t("received.empty") }}
      </div>
        <div
          v-for="file in receivedFiles"
          :key="file.id"
          class="received-item"
        >
        <component :is="getFileIcon(file.name)" :size="18" class="received-icon" />
        <div class="received-info">
          <span class="received-name">{{ file.name }}</span>
          <span class="received-meta">{{ t("received.from", { name: file.source }) }}</span>
        </div>
        <div class="received-right">
          <span class="received-size">{{ file.size }}</span>
          <span class="received-date">{{ file.date }}</span>
        </div>
        <!-- audit-22: act on a single received file without hunting Explorer -->
        <div class="received-actions">
          <button
            class="received-action"
            type="button"
            :title="t('received.reveal')"
            :aria-label="t('received.reveal')"
            :disabled="busyId === file.id"
            @click="handleReveal(file)"
          >
            <FolderSearch :size="15" />
          </button>
          <button
            class="received-action received-action--danger"
            type="button"
            :title="t('received.deleteRecord')"
            :aria-label="t('received.deleteRecord')"
            @click="deleteTarget = file"
          >
            <Trash2 :size="15" />
          </button>
        </div>
      </div>
    </div>

    <ConfirmDialog
      :visible="deleteTarget != null"
      :title="t('received.deleteConfirmTitle')"
      :description="t('received.deleteConfirmDescription', { name: deleteTarget?.name ?? '' })"
      :confirm-label="t('received.deleteRecord')"
      tone="danger"
      @confirm="confirmDelete"
      @cancel="deleteTarget = null"
    />
  </div>
</template>

<style scoped>
.received-page {
  padding: 32px;
  max-width: var(--content-max-width);
  margin: 0 auto;
}

.page-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 24px;
}

.page-title {
  font-size: var(--text-2xl);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  margin: 0 0 4px;
  line-height: var(--leading-tight);
}

.page-subtitle {
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
  margin: 0;
  font-family: var(--font-mono);
}

.outline-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  border: 1px solid var(--color-brand-primary);
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--color-text-brand);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: all var(--transition-fast);
  white-space: nowrap;
}

.outline-btn:hover {
  background: var(--color-selected);
}

.received-card {
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
  overflow: hidden;
}

.received-empty {
  padding: 32px 20px;
  text-align: center;
  font-size: var(--text-sm);
  color: var(--color-text-tertiary);
}

.received-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 20px;
  border-bottom: 1px solid var(--color-border);
  transition: background var(--transition-fast);
}

.received-item:last-child {
  border-bottom: none;
}

.received-item:hover {
  background: var(--color-hover);
}

.received-icon {
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.received-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.received-name {
  font-size: var(--text-base);
  font-weight: var(--weight-medium);
  color: var(--color-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.received-meta {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.received-right {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 2px;
  flex-shrink: 0;
}

.received-size {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  font-family: var(--font-mono);
}

.received-date {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  font-family: var(--font-mono);
}

.received-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  opacity: 0;
  transition: opacity var(--transition-fast);
}

.received-item:hover .received-actions,
.received-item:focus-within .received-actions {
  opacity: 1;
}

.received-action {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.received-action:hover:not(:disabled) {
  background: var(--color-hover);
  color: var(--color-text-primary);
}

.received-action--danger:hover:not(:disabled) {
  background: var(--color-state-error-soft);
  color: var(--color-state-error);
}

.received-action:disabled { cursor: wait; opacity: 0.5; }
</style>
