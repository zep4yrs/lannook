<script setup lang="ts">
import { computed, ref, shallowRef, watch, onUnmounted } from "vue";
import { DownloadCloud, FileText, Clock, CheckCircle2, XCircle, RotateCcw } from "lucide-vue-next";
import { useLocale } from "@/i18n";
import { formatBytes } from "@/utils/format";
import type { ReceiveDownloadItem } from "@/stores/mobileSession";
import { useModalA11y } from "@/composables/useModalA11y";

export interface ReceiveTransferInfo {
  id: string;
  sourceDeviceName: string;
  files: { id: string; name: string; size: number }[];
  totalBytes: number;
  expiresAt?: string;
}

const { t } = useLocale();
const props = defineProps<{
  visible: boolean;
  transfer: ReceiveTransferInfo | null;
  /** Live per-file download state (audit-30). Empty until accepting. */
  downloads?: ReceiveDownloadItem[];
  receiving?: boolean;
}>();

const emit = defineEmits<{
  accept: [transferId: string];
  reject: [transferId: string];
  retryFile: [fileId: string];
}>();

const isExpired = ref(false);
// audit-2: show the user how long they still have to decide.
const remainingSeconds = shallowRef<number | null>(null);
let expiryTimer: ReturnType<typeof setTimeout> | null = null;
let countdownTimer: ReturnType<typeof setInterval> | null = null;

function stopTimers() {
  if (expiryTimer) {
    clearTimeout(expiryTimer);
    expiryTimer = null;
  }
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
}

watch(
  () => props.transfer,
  (transfer) => {
    stopTimers();
    isExpired.value = false;
    remainingSeconds.value = null;

    if (!transfer) return;
    const deadline = transfer.expiresAt
      ? new Date(transfer.expiresAt).getTime()
      : Date.now() + 30 * 60 * 1000; // Default 30 min expiry if no expiresAt

    const tick = () => {
      const remaining = Math.max(0, Math.round((deadline - Date.now()) / 1000));
      remainingSeconds.value = remaining;
      if (remaining <= 0) {
        isExpired.value = true;
        stopTimers();
      }
    };
    tick();
    countdownTimer = setInterval(tick, 1000);
    if (transfer.expiresAt && deadline - Date.now() <= 0) {
      // Already expired server-side; tick above handled it.
    } else if (!transfer.expiresAt) {
      expiryTimer = setTimeout(() => {
        isExpired.value = true;
        stopTimers();
      }, deadline - Date.now());
    }
  },
  { immediate: true }
);

onUnmounted(stopTimers);

const cardElement = ref<HTMLElement | null>(null);
useModalA11y({
  visible: () => props.visible,
  container: cardElement,
  onEscape: () => handleReject(),
});

const fileCount = computed(() => props.transfer?.files.length ?? 0);
const formattedTotal = computed(() => formatBytes(props.transfer?.totalBytes ?? 0));

const remainingLabel = computed(() => {
  const seconds = remainingSeconds.value;
  if (seconds == null) return "";
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
});

const downloadItems = computed(() => props.downloads ?? []);
const hasDownloadState = computed(() => downloadItems.value.length > 0);
const doneCount = computed(
  () => downloadItems.value.filter((item) => item.status === "done").length
);

function percentOf(item: ReceiveDownloadItem): number {
  if (item.status === "done") return 100;
  if (item.size <= 0) return item.status === "downloading" ? 50 : 0;
  return Math.min(100, Math.round((item.loadedBytes / item.size) * 100));
}

function truncateName(name: string, maxLen = 32): string {
  if (name.length <= maxLen) return name;
  const ext = name.lastIndexOf(".");
  if (ext > 0 && name.length - ext <= 6) {
    const base = name.slice(0, maxLen - (name.length - ext) - 3);
    return `${base}...${name.slice(ext)}`;
  }
  return `${name.slice(0, maxLen - 3)}...`;
}

function handleAccept() {
  if (props.transfer && !isExpired.value) {
    emit("accept", props.transfer.id);
  }
}

function handleReject() {
  if (props.transfer) {
    emit("reject", props.transfer.id);
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible && transfer" class="dialog-wrapper">
      <div class="backdrop" />
      <div ref="cardElement" class="dialog-card" role="dialog" aria-modal="true" :aria-label="t('receive.incomingRequest', { name: transfer.sourceDeviceName, count: fileCount })">
        <!-- Header -->
        <div class="dialog-header">
          <div class="dialog-icon">
            <DownloadCloud :size="24" />
          </div>
          <h2 class="dialog-title">
            {{ t("receive.incomingRequest", { name: transfer.sourceDeviceName, count: fileCount }) }}
          </h2>
          <p v-if="remainingLabel && !isExpired && !hasDownloadState" class="expiry-countdown" :class="{ 'expiry-countdown--urgent': (remainingSeconds ?? 0) < 60 }">
            <Clock :size="13" /> {{ t("receive.expiresIn", { time: remainingLabel }) }}
          </p>
        </div>

        <!-- Expired State -->
        <div v-if="isExpired" class="expired-state">
          <Clock :size="20" />
          <span class="expired-text">{{ t("receive.expired") }}</span>
          <button class="reject-btn" @click="handleReject">{{ t("receive.close") }}</button>
        </div>

        <!-- Normal State -->
        <template v-else>
          <!-- File List: live per-file state while downloading, plain list before -->
          <div class="file-list">
            <template v-if="!hasDownloadState">
              <div
                v-for="(file, idx) in transfer.files"
                :key="file.id || idx"
                class="file-row"
              >
                <FileText :size="14" class="file-row-icon" />
                <span class="file-row-name">{{ truncateName(file.name) }}</span>
                <span class="file-row-size">{{ formatBytes(file.size) }}</span>
              </div>
            </template>
            <template v-else>
              <div v-for="item in downloadItems" :key="item.fileId" class="file-row file-row--live">
                <CheckCircle2 v-if="item.status === 'done'" :size="14" class="file-status file-status--done" />
                <XCircle v-else-if="item.status === 'failed'" :size="14" class="file-status file-status--failed" />
                <FileText v-else :size="14" class="file-row-icon" />
                <span class="file-row-body">
                  <span class="file-row-name">{{ truncateName(item.name) }}</span>
                  <span v-if="item.status === 'downloading'" class="file-progress">
                    <span class="file-progress-fill" :style="{ width: `${percentOf(item)}%` }" />
                  </span>
                  <span v-if="item.status === 'failed' && item.error" class="file-error">{{ item.error }}</span>
                </span>
                <span v-if="item.status === 'downloading'" class="file-row-size">
                  {{ formatBytes(item.loadedBytes) }}
                </span>
                <button
                  v-if="item.status === 'failed'"
                  class="retry-file-btn"
                  type="button"
                  :disabled="receiving"
                  @click="emit('retryFile', item.fileId)"
                >
                  <RotateCcw :size="12" /> {{ t("transfers.retry") }}
                </button>
              </div>
            </template>
          </div>

          <!-- Total -->
          <div class="total-row">
            <span class="total-label">{{ t("receive.total") }}</span>
            <span class="total-value">
              {{ formattedTotal }}
              <span v-if="hasDownloadState" class="done-count">{{ t("receive.doneCount", { done: doneCount, total: fileCount }) }}</span>
            </span>
          </div>

          <!-- Actions -->
          <div class="dialog-actions">
            <button
              v-if="!hasDownloadState"
              class="accept-btn"
              :disabled="receiving"
              @click="handleAccept"
            >
              {{ receiving ? t("receive.preparing") : t("receive.accept") }}
            </button>
            <button
              class="reject-btn"
              :disabled="receiving && !hasDownloadState"
              @click="handleReject"
            >
              {{ hasDownloadState ? t("receive.closeAfterBatch") : t("receive.reject") }}
            </button>
          </div>
        </template>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.dialog-wrapper {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}

.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  animation: fade-in 180ms ease forwards;
}

.dialog-card {
  position: relative;
  width: 100%;
  max-width: 340px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-xl);
  padding: 24px 20px;
  animation: scale-in 220ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

.dialog-header {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  margin-bottom: 16px;
}

.dialog-icon {
  width: 48px;
  height: 48px;
  border-radius: var(--radius-full);
  background: var(--color-brand-primary-soft);
  color: var(--color-brand-primary);
  display: flex;
  align-items: center;
  justify-content: center;
}

.dialog-title {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  text-align: center;
  margin: 0;
  line-height: var(--leading-tight);
}

.expiry-countdown {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  margin: 0;
  color: var(--color-text-tertiary);
  font-size: var(--text-xs);
  font-family: var(--font-mono);
}
.expiry-countdown--urgent { color: var(--color-state-error); }

/* File List */
.file-list {
  flex: 1;
  overflow-y: auto;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  margin-bottom: 12px;
  max-height: 200px;
}

.file-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
}

.file-row + .file-row {
  border-top: 1px solid var(--color-border);
}

.file-row-icon {
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

.file-status--done { color: var(--color-state-success); flex-shrink: 0; }
.file-status--failed { color: var(--color-state-error); flex-shrink: 0; }

.file-row-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.file-row-name {
  font-size: var(--text-sm);
  color: var(--color-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-row-size {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  font-family: var(--font-mono);
  white-space: nowrap;
}

.file-progress {
  height: 4px;
  border-radius: var(--radius-full);
  background: var(--color-surface-inset);
  overflow: hidden;
}
.file-progress-fill {
  display: block;
  height: 100%;
  background: var(--color-brand-primary);
  transition: width 200ms ease;
}

.file-error {
  color: var(--color-state-error);
  font-size: var(--text-xs);
  line-height: 1.4;
  overflow-wrap: anywhere;
}

.retry-file-btn {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border: 1px solid var(--color-brand-primary);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-brand);
  font-size: var(--text-xs);
  cursor: pointer;
}
.retry-file-btn:disabled { opacity: 0.5; cursor: wait; }

.done-count { margin-left: 8px; color: var(--color-text-tertiary); font-weight: var(--weight-normal); }

/* Total */
.total-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 4px;
  margin-bottom: 16px;
}

.total-label {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.total-value {
  font-size: var(--text-sm);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  font-family: var(--font-mono);
}

/* Actions */
.dialog-actions {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.accept-btn {
  width: 100%;
  height: 44px;
  border: none;
  border-radius: var(--radius-md);
  background: var(--color-brand-primary);
  color: var(--color-text-inverse);
  font-size: var(--text-base);
  font-weight: var(--weight-semibold);
  cursor: pointer;
  transition: background var(--transition-fast), opacity var(--transition-fast);
}

.accept-btn:hover:not(:disabled) {
  background: var(--color-brand-primary-hover);
}

.accept-btn:active:not(:disabled) {
  background: var(--color-brand-primary-active);
}

.accept-btn:disabled {
  opacity: 0.6;
  cursor: wait;
}

.reject-btn {
  width: 100%;
  height: 40px;
  border: none;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: var(--text-base);
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: color var(--transition-fast), background var(--transition-fast);
}

.reject-btn:hover:not(:disabled) {
  color: var(--color-text-primary);
  background: var(--color-hover);
}

.reject-btn:disabled { opacity: 0.5; cursor: not-allowed; }

/* Expired State */
.expired-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 24px 0 8px;
  color: var(--color-text-tertiary);
}

.expired-text {
  font-size: var(--text-base);
  color: var(--color-text-secondary);
}

.expired-state .reject-btn {
  margin-top: 8px;
  max-width: 160px;
}

@keyframes fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

@keyframes scale-in {
  from {
    opacity: 0;
    transform: scale(0.92);
  }
  to {
    opacity: 1;
    transform: scale(1);
  }
}
</style>
