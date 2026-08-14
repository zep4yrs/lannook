<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, shallowRef } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  FolderOpen,
  FileText,
  FileImage,
  FileVideo,
  FileArchive,
  Pause,
  X,
  ChevronDown,
  ChevronRight,
  ArrowDownToLine,
  ArrowUpFromLine,
  RefreshCw,
  Search,
  Trash2,
  RotateCcw,
} from "lucide-vue-next";
import { useTransfersStore } from "@/stores/transfers";
import { useDevicesStore } from "@/stores/devices";
import { useSettingsStore } from "@/stores/settings";
import { useAppStore } from "@/stores/app";
import { isTauri, openReceiveFolder } from "@/services/tauri";
import type { TransferTask } from "@/types";
import TransferCenterFilterBar, { type TransferCenterFilter } from "@/components/transfers/TransferCenterFilterBar.vue";
import { useLocale } from "@/i18n";

const transfersStore = useTransfersStore();
const devicesStore = useDevicesStore();
const settingsStore = useSettingsStore();
const appStore = useAppStore();
const route = useRoute();
const router = useRouter();
const { t } = useLocale();

const expandedId = shallowRef<string | null>(null);
const now = shallowRef(Date.now());
const searchQuery = ref("");
const showSelection = ref(false);
const selectedIds = ref<string[]>([]);
let elapsedTimer: ReturnType<typeof window.setInterval> | null = null;

const attentionStatuses = new Set(["paused", "failed", "awaiting_acceptance"]);

function parseFilter(value: unknown): TransferCenterFilter {
  if (value === "active" || value === "completed" || value === "attention") {
    return value;
  }
  return "all";
}

const selectedFilter = computed(() => parseFilter(route.query.filter));

function matchesSearch(task: TransferTask): boolean {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return true;
  return (
    task.files.some((file) => file.name.toLowerCase().includes(query)) ||
    getDeviceName(task.sourceDeviceId).toLowerCase().includes(query) ||
    getDeviceName(task.targetDeviceId).toLowerCase().includes(query)
  );
}

const filteredTransfers = computed(() => {
  const base = (() => {
    switch (selectedFilter.value) {
      case "active":
        return transfersStore.activeTransfers;
      case "completed":
        return transfersStore.completedTransfers;
      case "attention":
        return transfersStore.transfers.filter((task) => attentionStatuses.has(task.status));
      default:
        return transfersStore.transfers;
    }
  })();
  return base.filter(matchesSearch);
});

const filterCounts = computed<Record<TransferCenterFilter, number>>(() => ({
  all: transfersStore.transfers.length,
  active: transfersStore.activeTransfers.length,
  completed: transfersStore.completedTransfers.length,
  attention: transfersStore.transfers.filter((task) => attentionStatuses.has(task.status)).length,
}));

const emptyStateText = computed(() => {
  const labels: Record<TransferCenterFilter, string> = {
    all: t("transfers.empty.all"),
    active: t("transfers.empty.active"),
    completed: t("transfers.empty.completed"),
    attention: t("transfers.empty.attention"),
  };
  return labels[selectedFilter.value];
});

const activeCount = computed(
  () => transfersStore.activeTransfers.length
);

const totalSpeed = computed(() => {
  const total = transfersStore.transfers.reduce(
    (sum, t) => sum + t.speedBytesPerSecond,
    0
  );
  return formatSpeed(total);
});

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return "—";
  if (bytesPerSec < 1024 * 1024)
    return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
}

function formatRemaining(seconds?: number): string {
  if (seconds == null || seconds <= 0) return "—";
  if (seconds < 60) return `${seconds}s`;
  const min = Math.floor(seconds / 60);
  const sec = seconds % 60;
  return `${min}m ${sec}s`;
}

function parseTimestamp(value: string | undefined): number | null {
  if (!value) return null;
  const parsed = Date.parse(value);
  if (!Number.isNaN(parsed)) return parsed;
  const seconds = Number(value);
  return Number.isFinite(seconds) ? seconds * 1_000 : null;
}

function formatElapsed(task: TransferTask): string {
  const start = parseTimestamp(task.createdAt);
  const end = task.completedAt ? parseTimestamp(task.completedAt) : now.value;
  if (!start || !end || end < start) return "—";
  const elapsed = Math.floor((end - start) / 1_000);
  const minutes = Math.floor(elapsed / 60);
  const seconds = elapsed % 60;
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`;
}

function getDeviceName(id: string): string {
  if (id === "local") return t("device.thisDevice");
  const device = devicesStore.devices.find((d) => d.id === id);
  return device?.name ?? id;
}

function getFileIcon(name: string) {
  if (/\.(png|jpg|jpeg|gif|svg|webp)$/i.test(name)) return FileImage;
  if (/\.(mp4|mov|avi|mkv)$/i.test(name)) return FileVideo;
  if (/\.(zip|rar|7z|tar|gz)$/i.test(name)) return FileArchive;
  return FileText;
}

function getDirectionLabel(direction: string): string {
  switch (direction) {
    case "upload_to_host":
      return t("transfers.upload");
    case "download_from_host":
      return t("transfers.download");
    case "relay":
      return t("transfers.relay");
    default:
      return direction;
  }
}

function getRelayPath(task: TransferTask): string | null {
  if (task.direction !== "relay") return null;
  const source = getDeviceName(task.sourceDeviceId);
  const target = getDeviceName(task.targetDeviceId);
  return `${source} → ${t("transfers.host")} → ${target}`;
}

function getStatusLabel(status: string): string {
  const map: Record<string, string> = {
    transferring: t("transfer.status.transferring"),
    verifying: t("transfer.status.verifying"),
    paused: t("transfer.status.paused"),
    completed: t("transfer.status.completed"),
    waiting: t("transfer.status.waiting"),
    requesting: t("transfer.status.requesting"),
    awaiting_acceptance: t("transfer.status.awaitingAcceptance"),
    accepted: t("transfer.status.accepted"),
    rejected: t("transfer.status.rejected"),
    expired: t("transfer.status.expired"),
    cancelled: t("transfer.status.cancelled"),
    failed: t("transfer.status.failed"),
  };
  return map[status] ?? status;
}

function getStatusClass(status: string): string {
  if (status === "transferring") return "badge--success";
  if (status === "verifying") return "badge--warning";
  if (status === "paused") return "badge--neutral";
  if (status === "completed") return "badge--success";
  if (status === "awaiting_acceptance") return "badge--warning";
  if (status === "accepted") return "badge--success";
  if (status === "rejected" || status === "cancelled" || status === "failed") return "badge--error";
  if (status === "expired") return "badge--neutral";
  return "badge--neutral";
}

function getChecksum(task: TransferTask): string | null {
  return task.files[0]?.checksum ?? null;
}

function getChecksumLabel(task: TransferTask): string {
  if (getChecksum(task)) return t("transfers.viewChecksum");
  return task.status === "completed" ? t("transfers.notGenerated") : t("transfers.calculating");
}

function getShortChecksum(checksum: string): string {
  const normalized = checksum.replace(/\s/g, "").toUpperCase();
  if (normalized.length <= 16) return normalized;
  return `${normalized.slice(0, 8)} · ${normalized.slice(-8)}`;
}

function toggleExpand(id: string) {
  expandedId.value = expandedId.value === id ? null : id;
}

function selectFilter(filter: TransferCenterFilter) {
  if (filter === selectedFilter.value) return;
  expandedId.value = null;
  void router.replace({
    name: "transfers",
    query: filter === "all" ? {} : { filter },
  });
}

function handlePause(task: TransferTask) {
  if (task.status === "paused") {
    transfersStore.resumeTransfer(task.id);
  } else {
    transfersStore.pauseTransfer(task.id);
  }
}

function handleCancel(id: string) {
  transfersStore.cancelTransfer(id);
}

async function handleOpenReceiveFolder() {
  if (!isTauri()) {
    appStore.pushToast("info", t("transfers.desktopOnly"), t("transfers.desktopOnlyDescription"));
    return;
  }
  try {
    await openReceiveFolder();
  } catch (err) {
    console.error("[transfers] Failed to open receive folder:", err);
    appStore.pushToast("error", t("transfers.openFailed"), t("transfers.openFailedDescription"));
  }
}

function handleRetry(id: string) {
  transfersStore.retryTransfer(id);
}

function toggleSelect(id: string) {
  if (selectedIds.value.includes(id)) {
    selectedIds.value = selectedIds.value.filter((selected) => selected !== id);
  } else {
    selectedIds.value.push(id);
  }
}

const allSelected = computed(() => {
  const visible = filteredTransfers.value;
  return visible.length > 0 && visible.every((task) => selectedIds.value.includes(task.id));
});

function toggleSelectAll() {
  if (allSelected.value) {
    const visibleIds = new Set(filteredTransfers.value.map((task) => task.id));
    selectedIds.value = selectedIds.value.filter((id) => !visibleIds.has(id));
  } else {
    for (const task of filteredTransfers.value) {
      if (!selectedIds.value.includes(task.id)) selectedIds.value.push(task.id);
    }
  }
}

function exitSelection() {
  showSelection.value = false;
  selectedIds.value = [];
}

function batchRetry() {
  for (const id of selectedIds.value) {
    const task = transfersStore.transfers.find((t) => t.id === id);
    if (task?.status === "failed") transfersStore.retryTransfer(id);
  }
  exitSelection();
}

async function batchDelete() {
  const ids = [...selectedIds.value];
  exitSelection();
  await transfersStore.removeTransfers(ids);
}

onMounted(() => {
  elapsedTimer = window.setInterval(() => {
    now.value = Date.now();
  }, 1_000);
  void transfersStore.fetchTransfers();
  // Only register listeners once
  if (!transfersStore.listenersRegistered) {
    transfersStore.setupWebSocketListeners();
  }
});

onUnmounted(() => {
  if (elapsedTimer !== null) window.clearInterval(elapsedTimer);
});
</script>

<template>
  <div class="transfers-page">
    <!-- Title Section -->
    <header class="page-header">
      <div class="header-left">
        <h1 class="page-title">{{ t("transfers.title") }}</h1>
        <p class="page-subtitle">
          {{ t("transfers.summary", { count: activeCount, speed: totalSpeed }) }}
        </p>
      </div>
      <button class="outline-btn" @click="handleOpenReceiveFolder">
        <FolderOpen :size="15" />
        {{ t("received.openFolder") }}
      </button>
    </header>

    <TransferCenterFilterBar
      :model-value="selectedFilter"
      :counts="filterCounts"
      @update:model-value="selectFilter"
    />

    <!-- Search & Batch Actions -->
    <div class="toolbar">
      <div class="search-box">
        <Search :size="14" />
        <input
          v-model="searchQuery"
          type="text"
          :placeholder="t('transfers.searchPlaceholder')"
        />
      </div>
      <div class="toolbar-actions">
        <button
          v-if="!showSelection"
          class="outline-btn outline-btn--small"
          @click="showSelection = true"
        >
          {{ t("transfers.select") }}
        </button>
        <template v-else>
          <button
            class="outline-btn outline-btn--small"
            :disabled="selectedIds.length === 0"
            @click="batchRetry"
          >
            <RotateCcw :size="14" />
            {{ t("transfers.batchRetry") }}
          </button>
          <button
            class="outline-btn outline-btn--small outline-btn--danger"
            :disabled="selectedIds.length === 0"
            @click="batchDelete"
          >
            <Trash2 :size="14" />
            {{ t("transfers.batchDelete") }}
          </button>
          <button class="outline-btn outline-btn--small" @click="exitSelection">
            {{ t("transfers.cancelSelect") }}
          </button>
          <span class="selection-count">{{ selectedIds.length }}</span>
        </template>
      </div>
    </div>

    <!-- Transfer Table Card -->
    <div class="table-card">
      <div class="table-header">
        <span class="col-checkbox">
          <input
            v-if="showSelection"
            type="checkbox"
            :checked="allSelected"
            @change="toggleSelectAll"
          />
        </span>
        <span class="col-icon"></span>
        <span>{{ t("transfers.file") }}</span>
        <span>{{ t("transfers.source") }}</span>
        <span>{{ t("transfers.target") }}</span>
        <span>{{ t("home.size") }}</span>
        <span>{{ t("transfers.progress") }}</span>
        <span>{{ t("transfers.speed") }}</span>
        <span>{{ t("transfers.remaining") }}</span>
        <span>{{ t("home.status") }}</span>
        <span>{{ t("transfers.actions") }}</span>
      </div>

      <div class="table-body">
        <template v-if="filteredTransfers.length === 0">
          <div class="table-empty">{{ emptyStateText }}</div>
        </template>
        <template v-else>
          <template v-for="task in filteredTransfers" :key="task.id">
            <div
              class="transfer-row"
              :class="{ 'transfer-row--expanded': expandedId === task.id }"
            >
            <span class="col-checkbox">
              <input
                v-if="showSelection"
                type="checkbox"
                :checked="selectedIds.includes(task.id)"
                @change="toggleSelect(task.id)"
              />
            </span>
            <button class="expand-btn" @click="toggleExpand(task.id)">
              <ChevronDown v-if="expandedId === task.id" :size="14" />
              <ChevronRight v-else :size="14" />
            </button>
            <span class="col-file">
              <component
                :is="getFileIcon(task.files[0]?.name ?? '')"
                :size="16"
                class="file-icon"
              />
              <span class="file-name-wrap">
                <span class="file-name">{{ task.files[0]?.name }}</span>
                <span v-if="getRelayPath(task)" class="relay-path">{{ getRelayPath(task) }}</span>
              </span>
            </span>
            <span class="col-source">
              <ArrowUpFromLine v-if="task.direction === 'upload_to_host'" :size="12" class="dir-icon" />
              <ArrowDownToLine v-else-if="task.direction === 'download_from_host'" :size="12" class="dir-icon" />
              <RefreshCw v-else-if="task.direction === 'relay'" :size="12" class="dir-icon" />
              {{ getDeviceName(task.sourceDeviceId) }}
            </span>
            <span class="col-target">{{ getDeviceName(task.targetDeviceId) }}</span>
            <span class="col-size">{{ formatSize(task.totalBytes) }}</span>
            <span class="col-progress">
              <div class="progress-bar">
                <div
                  class="progress-fill"
                  :style="{ width: `${Math.round((task.status === 'verifying' ? task.checksumProgress ?? 0 : task.progress) * 100)}%` }"
                ></div>
              </div>
              <span class="progress-text">
                <template v-if="task.status === 'verifying'">
                  {{ Math.round((task.checksumProgress ?? 0) * 100) }}%
                </template>
                <template v-else>{{ Math.round(task.progress * 100) }}%</template>
              </span>
            </span>
            <span class="col-speed">
              <span>{{ formatSpeed(task.speedBytesPerSecond) }}</span>
              <span class="elapsed-time">{{ t("transfers.elapsed", { time: formatElapsed(task) }) }}</span>
            </span>
            <span class="col-remaining">{{ formatRemaining(task.remainingSeconds) }}</span>
            <span class="col-status">
              <span class="dir-badge">{{ getDirectionLabel(task.direction) }}</span>
              <span class="badge" :class="getStatusClass(task.status)">
                {{ getStatusLabel(task.status) }}
              </span>
            </span>
            <span class="col-actions">
              <button
                v-if="task.status === 'failed'"
                class="resume-btn"
                @click="handleRetry(task.id)"
              >
                {{ t("transfers.retry") }}
              </button>
              <button
                v-else-if="task.status === 'paused'"
                class="resume-btn"
                @click="handlePause(task)"
              >
                {{ t("transfers.resume") }}
              </button>
              <button
                v-else-if="task.status === 'transferring'"
                class="action-btn"
                :title="t('transfers.pause')"
                @click="handlePause(task)"
              >
                <Pause :size="14" />
              </button>
              <button
                v-if="task.status !== 'completed' && task.status !== 'cancelled' && task.status !== 'rejected' && task.status !== 'expired' && task.status !== 'failed'"
                class="action-btn action-btn--danger"
                :title="t('transfers.cancel')"
                @click="handleCancel(task.id)"
              >
                <X :size="14" />
              </button>
            </span>
            </div>

            <!-- Expanded Detail -->
            <div v-if="expandedId === task.id" class="transfer-detail">
              <div class="detail-grid">
              <div class="detail-item">
                <span class="detail-label">{{ t("transfers.chunkProgress") }}</span>
                <div class="chunk-viz">
                  <span
                    v-for="i in 20"
                    :key="i"
                    class="chunk"
                    :class="{ 'chunk--done': i / 20 <= task.progress }"
                  ></span>
                </div>
              </div>
              <div class="detail-item">
                <span class="detail-label">{{ t("transfers.elapsed") }}</span>
                <span class="detail-value">{{ formatElapsed(task) }}</span>
              </div>
              <div class="detail-item">
                <span class="detail-label">{{ t("transfers.startedAt") }}</span>
                <span class="detail-value">{{ new Date(task.createdAt).toLocaleTimeString() }}</span>
              </div>
              <div class="detail-item">
                <span class="detail-label">{{ t("transfers.savePath") }}</span>
                <span class="detail-value">{{ task.savePath ?? settingsStore.receiveFolder }}</span>
              </div>
              <div class="detail-item">
                <span class="detail-label">{{ t("transfers.quickChecksum") }}</span>
                <details v-if="getChecksum(task)" class="checksum-details">
                  <summary class="detail-value detail-value--mono" :title="t('transfers.fullShaHint')">
                    {{ getShortChecksum(getChecksum(task)!) }}
                  </summary>
                  <code class="checksum-full">{{ getChecksum(task) }}</code>
                </details>
                <span v-else-if="task.status === 'verifying'" class="detail-value detail-value--mono">
                  {{ t("transfers.verifyingProgress", { percent: Math.round((task.checksumProgress ?? 0) * 100) }) }}
                </span>
                <span v-else class="detail-value detail-value--mono">{{ getChecksumLabel(task) }}</span>
              </div>
              <div class="detail-item">
                <span class="detail-label">{{ t("transfers.retryCountLabel") }}</span>
                <span class="detail-value">{{ task.retryCount ?? 0 }}</span>
              </div>
              </div>
            </div>
          </template>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.transfers-page {
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
  font-size: var(--text-base);
  color: var(--color-text-secondary);
  margin: 0;
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

.outline-btn--small {
  padding: 6px 10px;
  font-size: var(--text-xs);
}

.outline-btn--danger {
  border-color: var(--color-state-error);
  color: var(--color-state-error);
}

.outline-btn--danger:hover {
  background: var(--color-state-error-soft);
}

.outline-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
  background: transparent;
}

/* Toolbar */
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.search-box {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  max-width: 360px;
  padding: 7px 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface-card);
  color: var(--color-text-tertiary);
  transition: border-color var(--transition-fast);
}

.search-box:focus-within {
  border-color: var(--color-brand-primary);
}

.search-box input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  color: var(--color-text-primary);
  font-size: var(--text-sm);
}

.search-box input::placeholder {
  color: var(--color-text-tertiary);
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.selection-count {
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-brand);
  background: var(--color-selected);
  border-radius: var(--radius-full);
  padding: 2px 10px;
  min-width: 28px;
  text-align: center;
}

/* Table Card */
.table-card {
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
  overflow: hidden;
}

.table-empty {
  padding: 48px 20px;
  color: var(--color-text-tertiary);
  font-size: var(--text-sm);
  text-align: center;
}

.table-header {
  display: grid;
  grid-template-columns: 20px 28px 1.2fr 100px 100px 72px 100px 80px 68px 72px 56px;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  background: var(--color-surface-inset);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.02em;
  position: sticky;
  top: 0;
  z-index: var(--z-sticky);
}

.transfer-row {
  display: grid;
  grid-template-columns: 20px 28px 1.2fr 100px 100px 72px 100px 80px 68px 72px 56px;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--color-border);
  font-size: var(--text-sm);
  transition: background var(--transition-fast);
}

.col-checkbox {
  display: flex;
  align-items: center;
  justify-content: center;
}

.col-checkbox input[type="checkbox"] {
  width: 14px;
  height: 14px;
  accent-color: var(--color-brand-primary);
  cursor: pointer;
}

.transfer-row:hover {
  background: var(--color-hover);
}

.transfer-row:last-child {
  border-bottom: none;
}

.expand-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.expand-btn:hover {
  background: var(--color-hover);
  color: var(--color-text-primary);
}

.col-file {
  display: flex;
  align-items: center;
  gap: 8px;
  overflow: hidden;
}

.file-icon {
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.file-name {
  color: var(--color-text-primary);
  font-weight: var(--weight-medium);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-name-wrap {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  overflow: hidden;
}

.relay-path {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.col-source,
.col-target {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--color-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir-icon {
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.col-size {
  color: var(--color-text-secondary);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
}

.col-progress {
  display: flex;
  align-items: center;
  gap: 6px;
}

.progress-bar {
  flex: 1;
  height: 4px;
  background: var(--color-surface-inset);
  border-radius: var(--radius-full);
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--color-brand-primary);
  border-radius: var(--radius-full);
  transition: width 0.4s ease;
}

.progress-text {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  font-family: var(--font-mono);
  min-width: 30px;
}

.col-speed {
  display: flex;
  flex-direction: column;
  gap: 2px;
  color: var(--color-text-secondary);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
}

.elapsed-time {
  color: var(--color-text-tertiary);
  white-space: nowrap;
}

.col-remaining {
  color: var(--color-text-tertiary);
  font-size: var(--text-xs);
  font-family: var(--font-mono);
}

.col-status {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 3px;
}

.dir-badge {
  font-size: 10px;
  color: var(--color-text-tertiary);
  white-space: nowrap;
  line-height: 1;
}

.col-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  opacity: 0;
  transition: opacity var(--transition-fast);
}

.transfer-row:hover .col-actions {
  opacity: 1;
}

.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.action-btn:hover {
  background: var(--color-hover);
  color: var(--color-text-primary);
}

.action-btn--danger:hover {
  background: var(--color-state-error-soft);
  color: var(--color-state-error);
}

.resume-btn {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border: 1px solid var(--color-brand-primary);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-brand);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  cursor: pointer;
  white-space: nowrap;
  transition: all var(--transition-fast);
}

.resume-btn:hover {
  background: var(--color-selected);
}

/* Badge */
.badge {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: var(--radius-full);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  white-space: nowrap;
}

.badge--success {
  background: var(--color-state-success-soft);
  color: var(--color-state-success);
}

.badge--warning {
  background: var(--color-state-warning-soft);
  color: var(--color-state-warning);
}

.badge--error {
  background: var(--color-state-error-soft);
  color: var(--color-state-error);
}

.badge--neutral {
  background: var(--color-surface-inset);
  color: var(--color-text-secondary);
}

/* Expanded Detail */
.transfer-detail {
  padding: 16px 16px 16px 54px;
  background: var(--color-surface-inset);
  border-bottom: 1px solid var(--color-border);
}

.detail-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 16px;
}

.detail-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.detail-label {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  font-weight: var(--weight-medium);
}

.detail-value {
  font-size: var(--text-sm);
  color: var(--color-text-primary);
}

.detail-value--mono {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
}

.checksum-details {
  min-width: 0;
}

.checksum-details summary {
  list-style: none;
  cursor: pointer;
}

.checksum-details summary::-webkit-details-marker {
  display: none;
}

.checksum-details summary::after {
  content: "展开";
  margin-left: 8px;
  color: var(--color-brand-primary);
  font-family: var(--font-sans);
  font-size: var(--text-xs);
}

.checksum-details[open] summary::after {
  content: "收起";
}

.checksum-full {
  display: block;
  max-width: 280px;
  margin-top: 6px;
  color: var(--color-text-secondary);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  line-height: 1.5;
  overflow-wrap: anywhere;
}

.chunk-viz {
  display: flex;
  gap: 2px;
}

.chunk {
  width: 12px;
  height: 8px;
  border-radius: 2px;
  background: var(--color-border);
}

.chunk--done {
  background: var(--color-brand-primary);
}
</style>
