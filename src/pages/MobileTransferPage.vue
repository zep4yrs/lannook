<script setup lang="ts">
/* Legacy desktop-store implementation retained as a comment during the mobile-state migration.
import { computed, onMounted, onUnmounted, ref } from "vue";
import { FileVideo, Pause, Play, XCircle, Clock } from "lucide-vue-next";
import { useTransfersStore } from "@/stores/transfers";

const transfersStore = useTransfersStore();

// Use the first active transfer as the "current" file
const currentTransfer = computed(() => {
  return (
    transfersStore.transfers.find((t) => t.status === "transferring") ??
    transfersStore.transfers[0]
  );
});

const currentFile = computed(() => currentTransfer.value?.files[0] ?? null);

const progressPercent = computed(() =>
  Math.round((currentTransfer.value?.progress ?? 0) * 100)
);

const transferredLabel = computed(() =>
  formatBytes(currentTransfer.value?.transferredBytes ?? 0)
);

const speedLabel = computed(() => {
  const speed = currentTransfer.value?.speedBytesPerSecond ?? 0;
  if (speed <= 0) return "—";
  return `${(speed / 1_048_576).toFixed(1)} MB/s`;
});

const remainingLabel = computed(() => {
  const secs = currentTransfer.value?.remainingSeconds ?? 0;
  if (secs <= 0) return "计算中...";
  return `${secs} 秒`;
});

// Queue: remaining files in the current transfer (excluding the current file)
const queuedFiles = computed(() => {
  const t = currentTransfer.value;
  if (!t || !t.files) return [];
  const currentIdx = t.files.findIndex((f) => f.id === currentFile.value?.id);
  return t.files.filter((_, i) => i !== currentIdx && i > (currentIdx >= 0 ? currentIdx : -1));
});

const targetDeviceName = computed(() => {
  // Show a generic label since we don't have the target device name in the transfer
  return "目标设备";
});

function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) {
    return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  }
  if (bytes >= 1_048_576) {
    return `${(bytes / 1_048_576).toFixed(0)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
}

function handlePause() {
  if (currentTransfer.value) {
    transfersStore.pauseTransfer(currentTransfer.value.id);
  }
}

function handleResume() {
  if (currentTransfer.value) {
    transfersStore.resumeTransfer(currentTransfer.value.id);
  }
}

function handleCancel() {
  if (currentTransfer.value) {
    transfersStore.cancelTransfer(currentTransfer.value.id);
  }
}
*/
import { computed, onMounted, onUnmounted, ref } from "vue";
import { storeToRefs } from "pinia";
import { FileVideo, Pause, Play, XCircle, Clock } from "lucide-vue-next";
import { useMobileSessionStore } from "@/stores/mobileSession";
import { useMobileTransfers } from "@/composables/useMobileTransfers";
import { useDevicesStore } from "@/stores/devices";
import { useLocale } from "@/i18n";

const mobileSession = useMobileSessionStore();
const devicesStore = useDevicesStore();
const { t } = useLocale();
const { sessionToken, isApproved } = storeToRefs(mobileSession);
const {
  currentTransfer,
  loading,
  error,
  pause,
  resume,
  cancel,
} = useMobileTransfers({ sessionToken, isApproved });

const now = ref(Date.now());
let elapsedTimer: ReturnType<typeof window.setInterval> | null = null;

onMounted(() => {
  elapsedTimer = window.setInterval(() => {
    now.value = Date.now();
  }, 1_000);
});

onUnmounted(() => {
  if (elapsedTimer !== null) window.clearInterval(elapsedTimer);
});

const currentFile = computed(() => currentTransfer.value?.files[0] ?? null);
const progressPercent = computed(() =>
  Math.round((currentTransfer.value?.progress ?? 0) * 100)
);
const transferredLabel = computed(() =>
  formatBytes(currentTransfer.value?.transferredBytes ?? 0)
);
const speedLabel = computed(() => {
  const speed = currentTransfer.value?.speedBytesPerSecond ?? 0;
  if (speed <= 0) return "—";
  return `${(speed / 1_048_576).toFixed(1)} MB/s`;
});
const remainingLabel = computed(() => {
  const seconds = currentTransfer.value?.remainingSeconds;
  if (!seconds || seconds <= 0) return "—";
  return t("mobile.seconds", { seconds });
});
const elapsedLabel = computed(() => {
  const transfer = currentTransfer.value;
  if (!transfer) return "—";
  const start = parseTimestamp(transfer.createdAt);
  const end = transfer.completedAt ? parseTimestamp(transfer.completedAt) : now.value;
  if (!start || !end || end < start) return "—";
  return formatDuration((end - start) / 1_000);
});
const isActionable = computed(() => {
  const transfer = currentTransfer.value;
  if (!transfer || progressPercent.value >= 100) return false;
  return !["completed", "cancelled", "rejected", "expired", "failed"].includes(transfer.status);
});
const queuedFiles = computed(() => currentTransfer.value?.files.slice(1) ?? []);
const targetDeviceName = computed(() => {
  const targetId = currentTransfer.value?.targetDeviceId;
  if (!targetId || targetId === "local") return t("mobile.localComputer");
  return devicesStore.devices.find((device) => device.id === targetId)?.name ?? t("mobile.targetDevice");
});

function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(0)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function parseTimestamp(value: string | undefined): number | null {
  if (!value) return null;
  const parsed = Date.parse(value);
  if (!Number.isNaN(parsed)) return parsed;
  const seconds = Number(value);
  return Number.isFinite(seconds) ? seconds * 1_000 : null;
}

function formatDuration(seconds: number): string {
  const safe = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safe / 60);
  const remainder = safe % 60;
  return minutes > 0 ? `${minutes}m ${remainder}s` : `${remainder}s`;
}

function handlePause() {
  if (currentTransfer.value) void pause(currentTransfer.value.id);
}

function handleResume() {
  if (currentTransfer.value) void resume(currentTransfer.value.id);
}

function handleCancel() {
  if (currentTransfer.value && isActionable.value) void cancel(currentTransfer.value.id);
}
</script>

<template>
  <div class="mobile-transfer-page">
    <p v-if="error" class="transfer-error">{{ error }}</p>
    <p v-if="loading && !currentTransfer" class="empty-state">{{ t("mobile.loadingTransfers") }}</p>
    <template v-else-if="currentTransfer">
    <!-- Current File Section -->
    <section class="current-file-section">
      <div class="file-icon-large">
        <FileVideo :size="36" />
      </div>
      <span class="current-file-name">{{ currentFile?.name ?? t("mobile.noActiveTransfer") }}</span>
      <span class="current-file-size">{{ formatBytes(currentFile?.size ?? 0) }}</span>
      <span class="current-file-target">{{ t("mobile.sendToLabel", { name: targetDeviceName }) }}</span>
    </section>

    <!-- Progress Section -->
    <section class="progress-section">
      <div class="progress-bar-track">
        <div
          class="progress-bar-fill"
          :style="{ width: `${progressPercent}%` }"
        />
      </div>
      <span class="progress-percent">{{ progressPercent }}%</span>
    </section>

    <!-- Stats Row -->
    <section class="stats-row">
      <div class="stat-item">
        <span class="stat-label">{{ t("mobile.transferredStat") }}</span>
        <span class="stat-value">{{ transferredLabel }}</span>
      </div>
      <div class="stat-divider" />
      <div class="stat-item">
        <span class="stat-label">{{ t("mobile.speedStat") }}</span>
        <span class="stat-value">{{ speedLabel }}</span>
      </div>
      <div class="stat-divider" />
      <div class="stat-item">
        <span class="stat-label">{{ t("mobile.remainingStat") }}</span>
        <span class="stat-value">{{ remainingLabel }}</span>
      </div>
      <div class="stat-divider" />
      <div class="stat-item">
        <span class="stat-label">{{ t("mobile.elapsedStat") }}</span>
        <span class="stat-value">{{ elapsedLabel }}</span>
      </div>
    </section>

    <!-- Action Buttons -->
    <section v-if="isActionable" class="actions-section">
      <button
        v-if="currentTransfer?.status === 'paused'"
        class="btn-pause"
        @click="handleResume"
      >
        <Play :size="15" />
        <span>{{ t("mobile.continue") }}</span>
      </button>
      <button
        v-else
        class="btn-pause"
        :disabled="currentTransfer?.status !== 'transferring'"
        @click="handlePause"
      >
        <Pause :size="15" />
        <span>{{ t("mobile.pauseAction") }}</span>
      </button>
      <button class="btn-cancel" @click="handleCancel">
        <XCircle :size="15" />
        <span>{{ t("transfers.cancel") }}</span>
      </button>
    </section>

    <!-- File Queue Section -->
    <section v-if="queuedFiles.length > 0" class="queue-section">
      <span class="queue-label">{{ t("mobile.queuedWaiting", { count: queuedFiles.length }) }}</span>
      <div class="queue-list">
        <div
          v-for="item in queuedFiles"
          :key="item.id"
          class="queue-item"
        >
          <div class="queue-item-icon">
            <FileVideo :size="16" />
          </div>
          <div class="queue-item-info">
            <span class="queue-item-name">{{ item.name }}</span>
            <span class="queue-item-size">{{ formatBytes(item.size) }}</span>
          </div>
          <span class="queue-badge">
            <Clock :size="11" />
            {{ t("mobile.queuePending") }}
          </span>
        </div>
      </div>
    </section>
    </template>
    <section v-else class="empty-state">
      {{ t("mobile.noTransfersEmpty") }}
    </section>
  </div>
</template>

<style scoped>
.mobile-transfer-page {
  max-width: 375px;
  margin: 0 auto;
  padding: 24px 20px;
  min-height: 100vh;
  background: var(--color-surface-page);
}

.empty-state,
.transfer-error {
  margin: 48px 0;
  padding: 16px;
  border-radius: var(--radius-lg);
  text-align: center;
  color: var(--color-text-secondary);
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
}

.transfer-error {
  margin: 0 0 16px;
  color: var(--color-state-error);
  background: var(--color-state-error-soft);
}

/* Current File */
.current-file-section {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 32px 0 24px;
}

.file-icon-large {
  width: 80px;
  height: 80px;
  border-radius: 16px;
  background: var(--color-brand-primary-soft);
  color: var(--color-brand-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 8px;
}

.current-file-name {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  text-align: center;
  word-break: break-all;
}

.current-file-size {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.current-file-target {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

/* Progress */
.progress-section {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  margin-bottom: 20px;
}

.progress-bar-track {
  width: 100%;
  height: 6px;
  border-radius: var(--radius-full);
  background: var(--color-surface-inset);
  overflow: hidden;
}

.progress-bar-fill {
  height: 100%;
  border-radius: var(--radius-full);
  background: var(--color-state-success);
  transition: width 400ms ease;
}

.progress-percent {
  font-size: 28px;
  font-weight: var(--weight-bold);
  color: var(--color-state-success);
}

/* Stats Row */
.stats-row {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 16px;
  padding: 14px 16px;
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  margin-bottom: 20px;
}

.stat-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
}

.stat-label {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.stat-value {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text-primary);
}

.stat-divider {
  width: 1px;
  height: 28px;
  background: var(--color-border);
}

/* Actions */
.actions-section {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  margin-bottom: 28px;
}

.btn-pause {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 20px;
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  background: var(--color-surface-card);
  color: var(--color-text-primary);
  font-size: var(--text-base);
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: background var(--transition-fast), border-color var(--transition-fast);
}

.btn-pause:hover {
  background: var(--color-hover);
  border-color: var(--color-border-strong);
}

.btn-cancel {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 20px;
  border: none;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--color-state-error);
  font-size: var(--text-base);
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: background var(--transition-fast);
}

.btn-cancel:hover {
  background: var(--color-state-error-soft);
}

/* Queue */
.queue-section {
  margin-top: 4px;
}

.queue-label {
  display: block;
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--color-text-tertiary);
  margin-bottom: 10px;
}

.queue-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.queue-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
}

.queue-item-icon {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-sm);
  background: var(--color-surface-inset);
  color: var(--color-text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.queue-item-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.queue-item-name {
  font-size: var(--text-sm);
  color: var(--color-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.queue-item-size {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.queue-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  border-radius: var(--radius-full);
  background: var(--color-state-warning-soft);
  color: var(--color-state-warning);
  font-size: 11px;
  font-weight: var(--weight-medium);
  flex-shrink: 0;
}
</style>
