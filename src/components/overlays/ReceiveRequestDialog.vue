<script setup lang="ts">
import { computed, ref, watch, onUnmounted } from "vue";
import { DownloadCloud, FileText, Clock } from "lucide-vue-next";
import { useLocale } from "@/i18n";

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
}>();

const emit = defineEmits<{
  accept: [transferId: string];
  reject: [transferId: string];
}>();

const isExpired = ref(false);
let expiryTimer: ReturnType<typeof setTimeout> | null = null;

// Check expiry (30 min timeout)
watch(
  () => props.transfer,
  (transfer) => {
    if (expiryTimer) {
      clearTimeout(expiryTimer);
      expiryTimer = null;
    }
    isExpired.value = false;

    if (transfer?.expiresAt) {
      const expiresAt = new Date(transfer.expiresAt).getTime();
      const now = Date.now();
      const remaining = expiresAt - now;
      if (remaining <= 0) {
        isExpired.value = true;
      } else {
        expiryTimer = setTimeout(() => {
          isExpired.value = true;
        }, remaining);
      }
    } else if (transfer) {
      // Default 30 min expiry if no expiresAt provided
      expiryTimer = setTimeout(() => {
        isExpired.value = true;
      }, 30 * 60 * 1000);
    }
  },
  { immediate: true }
);

onUnmounted(() => {
  if (expiryTimer) clearTimeout(expiryTimer);
});

const fileCount = computed(() => props.transfer?.files.length ?? 0);

const formattedTotal = computed(() => formatBytes(props.transfer?.totalBytes ?? 0));

function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) {
    return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  }
  if (bytes >= 1_048_576) {
    return `${(bytes / 1_048_576).toFixed(1)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
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
      <div class="dialog-card">
        <!-- Header -->
        <div class="dialog-header">
          <div class="dialog-icon">
            <DownloadCloud :size="24" />
          </div>
          <h2 class="dialog-title">
            {{ t("receive.incomingRequest", { name: transfer.sourceDeviceName, count: fileCount }) }}
          </h2>
        </div>

        <!-- Expired State -->
        <div v-if="isExpired" class="expired-state">
          <Clock :size="20" />
          <span class="expired-text">{{ t("receive.expired") }}</span>
          <button class="reject-btn" @click="handleReject">{{ t("receive.close") }}</button>
        </div>

        <!-- Normal State -->
        <template v-else>
          <!-- File List -->
          <div class="file-list">
            <div
              v-for="(file, idx) in transfer.files"
              :key="idx"
              class="file-row"
            >
              <FileText :size="14" class="file-row-icon" />
              <span class="file-row-name">{{ truncateName(file.name) }}</span>
              <span class="file-row-size">{{ formatBytes(file.size) }}</span>
            </div>
          </div>

          <!-- Total -->
          <div class="total-row">
            <span class="total-label">{{ t("receive.total") }}</span>
            <span class="total-value">{{ formattedTotal }}</span>
          </div>

          <!-- Actions -->
          <div class="dialog-actions">
            <button class="accept-btn" @click="handleAccept">
              {{ t("receive.accept") }}
            </button>
            <button class="reject-btn" @click="handleReject">
              {{ t("receive.reject") }}
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

.file-row-name {
  flex: 1;
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
  transition: background var(--transition-fast);
}

.accept-btn:hover {
  background: var(--color-brand-primary-hover);
}

.accept-btn:active {
  background: var(--color-brand-primary-active);
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

.reject-btn:hover {
  color: var(--color-text-primary);
  background: var(--color-hover);
}

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
