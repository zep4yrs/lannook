<script setup lang="ts">
import { ref, computed, onBeforeUnmount, watch } from "vue";
import { storeToRefs } from "pinia";
import {
  ChevronRight,
  UploadCloud,
  X,
  FileVideo,
  FileImage,
  FileArchive,
  FileText,
  Monitor,
  CheckCircle,
  Loader,
  RefreshCw,
} from "lucide-vue-next";
import { useDevicesStore } from "@/stores/devices";
import { useSettingsStore } from "@/stores/settings";
import {
  createTransfer,
  cancelTransferApi,
  createRelay,
  getAvailableDevices,
} from "@/services/api";
import { FileUploader } from "@/services/upload";
import type { UploadProgress } from "@/services/upload";
import DeviceSelectorSheet from "@/components/overlays/DeviceSelectorSheet.vue";
import { useMobileSessionStore } from "@/stores/mobileSession";
import type { Device } from "@/types";
import { useLocale } from "@/i18n";

const devicesStore = useDevicesStore();
const settingsStore = useSettingsStore();
const { t } = useLocale();
const sizeWarning = ref("");

const showDeviceSheet = ref(false);
const selectedDevice = computed(() => devicesStore.selectedDevice);

const transferId = ref<string | null>(null);
const isUploading = ref(false);
const uploadComplete = ref(false);
const uploadError = ref<string | null>(null);
const uploadProgress = ref<UploadProgress | null>(null);

// Relay state
const isRelay = ref(false);
const relayStatus = ref<string | null>(null);

let uploader: FileUploader | null = null;
const mobileSession = useMobileSessionStore();
const { sessionToken, deviceId, isApproved, connectionPhase } = storeToRefs(mobileSession);
let deviceRefreshTimer: ReturnType<typeof window.setInterval> | null = null;

// Real file input
const fileInputRef = ref<HTMLInputElement | null>(null);

interface SelectedFile {
  id: string;
  name: string;
  size: number;
  type: string;
  file: File;
  /** Blob URL preview for image files; revoked on removal/unmount. */
  previewUrl?: string;
}

const selectedFiles = ref<SelectedFile[]>([]);

const totalSize = computed(() =>
  selectedFiles.value.reduce((sum, f) => sum + f.size, 0)
);

const totalSizeLabel = computed(() => formatBytes(totalSize.value));

// Determine if the selected device is a phone (relay target)
const isTargetPhone = computed(() => {
  const d = selectedDevice.value;
  return d != null && (d.deviceType === "phone" || d.platform === "ios" || d.platform === "android");
});

const successTitle = computed(() =>
  isRelay.value ? t("mobile.sendSuccessRelay") : t("mobile.sendSuccess")
);
const successHint = computed(() =>
  isRelay.value
    ? t("mobile.sendSuccessRelayDescription")
    : t("mobile.sendSuccessDescription"),
);

async function loadAvailableDevices(token: string) {
  try {
    const response = await getAvailableDevices(token) as { devices?: Device[] };
    const availableDevices = response.devices ?? [];
    devicesStore.setDevices(availableDevices);

    if (!devicesStore.selectedDeviceId) {
      const host = availableDevices.find((device) => device.id === "desktop" && device.online);
      if (host) devicesStore.selectDevice(host.id);
    }
  } catch (error) {
    console.warn("[mobile] Failed to load available devices:", error);
  }
}

watch(
  [sessionToken, isApproved],
  ([token, approved]) => {
    if (token && approved) {
      void loadAvailableDevices(token);
      if (deviceRefreshTimer !== null) window.clearInterval(deviceRefreshTimer);
      deviceRefreshTimer = window.setInterval(() => void loadAvailableDevices(token), 5000);
    } else {
      if (deviceRefreshTimer !== null) {
        window.clearInterval(deviceRefreshTimer);
        deviceRefreshTimer = null;
      }
      devicesStore.setDevices([]);
    }
  },
  { immediate: true }
);

onBeforeUnmount(() => {
  if (deviceRefreshTimer !== null) window.clearInterval(deviceRefreshTimer);
  for (const file of selectedFiles.value) {
    if (file.previewUrl) URL.revokeObjectURL(file.previewUrl);
  }
});

function triggerFileInput() {
  fileInputRef.value?.click();
}

function onFileSelected(event: Event) {
  const input = event.target as HTMLInputElement;
  if (!input.files || input.files.length === 0) return;

  const limit = settingsStore.maxFileSize;
  const allFiles = Array.from(input.files);
  const rejected = limit > 0 ? allFiles.filter((f) => f.size > limit) : [];
  const accepted = limit > 0 ? allFiles.filter((f) => f.size <= limit) : allFiles;

  const newFiles: SelectedFile[] = accepted.map((file, i) => ({
    id: `f-${Date.now()}-${i}`,
    name: file.name,
    size: file.size,
    type: getFileType(file.name),
    file,
    // Blob preview for images so the phone shows a real thumbnail, not just
    // a generic icon.
    previewUrl: getFileType(file.name) === "image" ? URL.createObjectURL(file) : undefined,
  }));

  selectedFiles.value = [...selectedFiles.value, ...newFiles];
  // A successful previous send must never lock the composer. Selecting the
  // next batch is the explicit start of the next transfer.
  if (newFiles.length > 0) {
    uploadComplete.value = false;
    uploadError.value = null;
  }
  sizeWarning.value =
    rejected.length > 0
      ? t("mobile.skippedOversized", { count: rejected.length })
      : "";
  // Reset input so same file can be selected again
  input.value = "";
}

function getFileType(name: string): string {
  if (/\.(mp4|mov|avi|mkv|webm)$/i.test(name)) return "video";
  if (/\.(png|jpg|jpeg|gif|svg|webp|heic)$/i.test(name)) return "image";
  if (/\.(zip|rar|7z|tar|gz)$/i.test(name)) return "archive";
  return "file";
}

function dropPreview(file: SelectedFile) {
  if (file.previewUrl) {
    URL.revokeObjectURL(file.previewUrl);
    file.previewUrl = undefined;
  }
}

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

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return "—";
  if (bytesPerSec < 1024 * 1024)
    return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
}

function formatRemaining(seconds: number | null): string {
  if (seconds == null || seconds <= 0) return "—";
  if (seconds < 60) return `${seconds}s`;
  const min = Math.floor(seconds / 60);
  const sec = seconds % 60;
  return `${min}m ${sec}s`;
}

function getFileIcon(type: string) {
  switch (type) {
    case "video":
      return FileVideo;
    case "image":
      return FileImage;
    case "archive":
      return FileArchive;
    default:
      return FileText;
  }
}

function formatElapsed(seconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(seconds));
  const minutes = Math.floor(safeSeconds / 60);
  const remainder = safeSeconds % 60;
  return minutes > 0 ? `${minutes}m ${remainder}s` : `${remainder}s`;
}

function getFileColor(type: string): string {
  switch (type) {
    case "video":
      return "var(--color-state-info)";
    case "image":
      return "var(--color-state-success)";
    case "archive":
      return "var(--color-state-warning)";
    default:
      return "var(--color-text-tertiary)";
  }
}

function removeFile(id: string) {
  const removed = selectedFiles.value.find((f) => f.id === id);
  if (removed?.previewUrl) URL.revokeObjectURL(removed.previewUrl);
  selectedFiles.value = selectedFiles.value.filter((f) => f.id !== id);
}

function handleDeviceSelect(deviceId: string) {
  devicesStore.selectDevice(deviceId);
}

function getDeviceMeta(): string {
  const d = selectedDevice.value;
  if (!d) return t("mobile.selectTarget");
  const platformMap: Record<string, string> = {
    macos: "macOS",
    ios: "iOS",
    android: "Android",
    windows: "Windows",
    web: "Web",
  };
  const platform = platformMap[d.platform] || d.platform;
  const status = d.online ? t("mobile.online") : t("mobile.offline");
  return `${platform} · ${status}`;
}

async function handleSend() {
  const token = sessionToken.value;
  if (isUploading.value || selectedFiles.value.length === 0) return;
  if (!token || !isApproved.value) {
    uploadError.value = t("mobile.waitForApprovalBeforeSend");
    return;
  }
  if (selectedDevice.value && !selectedDevice.value.online) {
    uploadError.value = t("mobile.targetOffline");
    return;
  }

  isUploading.value = true;
  uploadComplete.value = false;
  uploadError.value = null;
  uploadProgress.value = null;
  isRelay.value = false;
  relayStatus.value = null;

  try {
    // If target is another phone, use relay through the host
    if (isTargetPhone.value && selectedDevice.value) {
      isRelay.value = true;
      relayStatus.value = t("mobile.uploadingToHost");

      const res = await createRelay(
        selectedFiles.value.map((f) => ({
          name: f.name,
          size: f.size,
          mimeType: f.file.type || undefined,
        })),
        deviceId.value || "",
        selectedDevice.value.id,
        token
      ) as {
        id?: string;
        transferId?: string;
        files?: { id: string; name: string; chunkSize?: number }[];
      };

      transferId.value = res.id || res.transferId || null;
      if (!transferId.value) {
        throw new Error(t("mobile.missingTransferId"));
      }

      // Upload files to host (relay stage 1)
      uploader = new FileUploader(token, (p: UploadProgress) => {
        uploadProgress.value = p;
      });
      uploader.beginTransfer(totalSize.value);

      for (let idx = 0; idx < selectedFiles.value.length; idx++) {
        const sf = selectedFiles.value[idx];
        const fileId = res.files?.[idx]?.id;
        await uploader.uploadFile(
          transferId.value,
          sf.file,
          fileId,
          res.files?.[idx]?.chunkSize
        );
      }
      await uploader.complete(transferId.value);

      relayStatus.value = t("mobile.uploadedWaitingTarget");
      selectedFiles.value = [];
      uploadComplete.value = true;
      return;
    }

    // Standard upload to host
    const res = await createTransfer(
      selectedFiles.value.map((f) => ({
        name: f.name,
        size: f.size,
        mimeType: f.file.type || undefined,
      })),
      token
    ) as {
      id?: string;
      transferId?: string;
      files?: { id: string; name: string; chunkSize?: number }[];
    };

    transferId.value = res.id || res.transferId || null;
    if (!transferId.value) {
      throw new Error(t("mobile.missingTransferId"));
    }

    // Create uploader and upload files sequentially
    uploader = new FileUploader(token, (p: UploadProgress) => {
      uploadProgress.value = p;
    });
    uploader.beginTransfer(totalSize.value);

    for (let idx = 0; idx < selectedFiles.value.length; idx++) {
      const sf = selectedFiles.value[idx];
      const fileId = res.files?.[idx]?.id;
      await uploader.uploadFile(
        transferId.value,
        sf.file,
        fileId,
        res.files?.[idx]?.chunkSize
      );
    }
    await uploader.complete(transferId.value);

    selectedFiles.value = [];
    uploadComplete.value = true;
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : t("mobile.uploadFailed");
    if (msg !== "cancelled") {
      uploadError.value = msg;
    }
  } finally {
    isUploading.value = false;
  }
}

async function handleCancel() {
  if (uploader) {
    uploader.cancel();
  }
  if (transferId.value && sessionToken.value) {
    try {
      await cancelTransferApi(transferId.value, sessionToken.value);
    } catch {
      // best effort
    }
  }
  isUploading.value = false;
  uploadProgress.value = null;
  transferId.value = null;
  uploader = null;
  isRelay.value = false;
  relayStatus.value = null;
}
</script>

<template>
  <div class="mobile-send-page">
    <!-- Title Section -->
    <section class="title-section">
      <h1 class="page-title">{{ t("mobile.sendTitle") }}</h1>
      <p class="page-subtitle">{{ t("mobile.sendSubtitle") }}</p>
    </section>

    <!-- Device Selector -->
    <section class="section">
      <span class="section-label">{{ t("mobile.sendTo") }}</span>
      <button class="device-card" :disabled="!isApproved" @click="showDeviceSheet = true">
        <div class="device-card-icon">
          <Monitor :size="20" />
        </div>
        <div class="device-card-info">
          <span class="device-card-name">
            {{ selectedDevice?.name || t("mobile.selectDevice") }}
          </span>
          <span class="device-card-meta">{{ getDeviceMeta() }}</span>
        </div>
        <ChevronRight :size="16" class="device-card-chevron" />
      </button>
    </section>

    <p
      v-if="sessionToken && !isApproved && connectionPhase === 'pending_approval'"
      class="approval-wait"
    >
      {{ t("mobile.waitingApprovalHint") }}
    </p>

    <!-- File Selection Area -->
    <section class="section">
      <input
        ref="fileInputRef"
        type="file"
        multiple
        style="display: none"
        @change="onFileSelected"
      />
      <div class="file-drop-area">
        <div class="upload-icon-circle">
          <UploadCloud :size="24" />
        </div>
        <span class="drop-title">{{ t("mobile.pickFiles") }}</span>
        <span class="drop-hint">{{ t("mobile.pickFilesHint") }}</span>
        <button class="select-file-btn" @click="triggerFileInput">{{ t("mobile.chooseFiles") }}</button>
      </div>
      <p v-if="sizeWarning" class="warning-msg">{{ sizeWarning }}</p>
      <p v-if="uploadError" class="error-msg">{{ uploadError }}</p>
    </section>

    <!-- Selected Files List -->
    <section v-if="selectedFiles.length > 0" class="section">
      <div class="files-header">
        <span class="files-count">{{ t("mobile.selectedCount", { count: selectedFiles.length }) }}</span>
        <span class="files-total">{{ t("mobile.totalFiles", { label: totalSizeLabel }) }}</span>
      </div>
      <div class="files-list">
        <div
          v-for="file in selectedFiles"
          :key="file.id"
          class="file-item"
        >
          <div
            class="file-icon-square"
            :style="{ color: getFileColor(file.type), background: `color-mix(in srgb, ${getFileColor(file.type)} 12%, transparent)` }"
          >
            <img
              v-if="file.previewUrl"
              :src="file.previewUrl"
              class="file-thumb"
              alt=""
              @error="dropPreview(file)"
            />
            <component v-else :is="getFileIcon(file.type)" :size="16" />
          </div>
          <div class="file-info">
            <span class="file-name">{{ file.name }}</span>
            <span class="file-size">{{ formatBytes(file.size) }}</span>
          </div>
          <button
            type="button"
            class="file-remove-btn"
            :disabled="isUploading"
            @click="removeFile(file.id)"
          >
            <X :size="14" />
          </button>
        </div>
      </div>
    </section>

    <!-- Upload Progress -->
    <section v-if="isUploading && uploadProgress" class="section">
      <div class="upload-progress-card">
        <div class="upload-progress-header">
          <Loader :size="14" class="spin" />
          <span class="upload-progress-name">{{ uploadProgress.fileName }}</span>
          <span v-if="uploadProgress.status === 'retrying'" class="retry-badge">
            网络中断，自动重试中…
          </span>
        </div>
        <div class="upload-progress-bar">
          <div
            class="upload-progress-fill"
            :style="{ width: `${Math.min(uploadProgress.progress, 100)}%` }"
          />
        </div>
        <div class="upload-progress-meta">
          <span>{{ Math.round(uploadProgress.progress) }}%</span>
          <span>{{ formatBytes(uploadProgress.transferredBytes) }} / {{ formatBytes(uploadProgress.totalBytes) }}</span>
          <span v-if="uploadProgress.status !== 'retrying'">{{ formatSpeed(uploadProgress.speedBytesPerSecond) }}</span>
          <span v-if="uploadProgress.status !== 'retrying'">{{ t("mobile.elapsedStat") }} {{ formatElapsed(uploadProgress.elapsedSeconds) }}</span>
          <span v-if="uploadProgress.status !== 'retrying'">{{ t("mobile.remainingStat") }} {{ formatRemaining(uploadProgress.remainingSeconds) }}</span>
        </div>
      </div>
    </section>

    <!-- Success State -->
    <section v-if="uploadComplete" class="section">
      <div class="success-card">
        <CheckCircle :size="32" class="success-icon" />
        <span class="success-title">{{ successTitle }}</span>
        <span class="success-hint">{{ successHint }}</span>
      </div>
    </section>

    <!-- Relay Status -->
    <section v-if="isRelay && relayStatus" class="section">
      <div class="relay-status-card">
        <RefreshCw :size="16" class="relay-icon spin" />
        <span class="relay-text">{{ relayStatus }}</span>
      </div>
    </section>

    <!-- Fixed Bottom CTA -->
    <div class="bottom-cta">
      <button
        v-if="isUploading"
        class="send-btn send-btn--cancel"
        @click="handleCancel"
      >
        {{ t("transfers.cancel") }}
      </button>
      <button
        v-else
        class="send-btn"
        :disabled="selectedFiles.length === 0 || !sessionToken || !isApproved"
        @click="handleSend"
      >
        {{ (isTargetPhone ? t("mobile.relaySend") : t("mobile.send")) }} {{ t("mobile.sendCount", { count: selectedFiles.length }) }}
      </button>
    </div>

    <!-- Device Selector Sheet -->
    <DeviceSelectorSheet
      :visible="showDeviceSheet"
      @close="showDeviceSheet = false"
      @select="handleDeviceSelect"
    />

    <!-- Incoming transfer approval is rendered once by MobileLayout. -->
  </div>
</template>

<style scoped>
.file-thumb {
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: inherit;
}
.mobile-send-page {
  max-width: 375px;
  margin: 0 auto;
  padding: 24px 20px 100px;
  min-height: 100vh;
  background: var(--color-surface-page);
}

/* Title */
.title-section {
  margin-bottom: 24px;
}

.page-title {
  font-size: 22px;
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  margin: 0 0 4px;
}

.page-subtitle {
  font-size: var(--text-base);
  color: var(--color-text-secondary);
  margin: 0;
}

/* Sections */
.section {
  margin-bottom: 20px;
}

.section-label {
  display: block;
  font-size: var(--text-xs);
  font-weight: var(--weight-semibold);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--color-text-tertiary);
  margin-bottom: 8px;
}

.approval-wait {
  margin: 0 16px 16px;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  background: var(--color-state-warning-soft);
  color: var(--color-state-warning);
  font-size: var(--text-sm);
}

/* Device Card */
.device-card {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 12px;
  border-radius: var(--radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface-card);
  cursor: pointer;
  text-align: left;
  transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
}

.device-card:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.device-card:hover {
  border-color: var(--color-border-strong);
  box-shadow: var(--shadow-sm);
}

.device-card-icon {
  width: 40px;
  height: 40px;
  border-radius: var(--radius-md);
  background: var(--color-brand-primary-soft);
  color: var(--color-brand-primary);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.device-card-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.device-card-name {
  font-size: var(--text-base);
  font-weight: var(--weight-medium);
  color: var(--color-text-primary);
}

.device-card-meta {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.device-card-chevron {
  color: var(--color-text-tertiary);
  flex-shrink: 0;
}

/* File Drop Area */
.file-drop-area {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 32px 20px;
  border: 2px dashed var(--color-border-strong);
  border-radius: var(--radius-lg);
  background: var(--color-surface-card);
}

.upload-icon-circle {
  width: 48px;
  height: 48px;
  border-radius: var(--radius-full);
  background: var(--color-brand-primary);
  color: var(--color-text-inverse);
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 4px;
}

.drop-title {
  font-size: var(--text-base);
  font-weight: var(--weight-medium);
  color: var(--color-text-primary);
}

.drop-hint {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.select-file-btn {
  margin-top: 8px;
  max-width: 200px;
  width: 100%;
  padding: 10px 20px;
  border: none;
  border-radius: var(--radius-md);
  background: var(--color-brand-primary);
  color: var(--color-text-inverse);
  font-size: var(--text-base);
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: background var(--transition-fast);
}

.select-file-btn:hover {
  background: var(--color-brand-primary-hover);
}

.select-file-btn:active {
  background: var(--color-brand-primary-active);
}

/* Files Header */
.files-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.files-count {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text-primary);
}

.files-total {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

/* Files List */
.files-list {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  overflow: hidden;
  background: var(--color-surface-card);
}

.file-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
}

.file-item + .file-item {
  border-top: 1px solid var(--color-border);
}

.file-icon-square {
  width: 32px;
  height: 32px;
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.file-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.file-name {
  font-size: var(--text-sm);
  color: var(--color-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.file-size {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.file-remove-btn {
  width: 24px;
  height: 24px;
  border: none;
  background: transparent;
  color: var(--color-text-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  border-radius: var(--radius-sm);
  flex-shrink: 0;
  transition: color var(--transition-fast), background var(--transition-fast);
}

.file-remove-btn:hover {
  color: var(--color-state-error);
  background: var(--color-state-error-soft);
}

/* Bottom CTA */
.bottom-cta {
  position: fixed;
  bottom: 0;
  left: 50%;
  transform: translateX(-50%);
  width: 100%;
  max-width: 375px;
  padding: 12px 20px calc(12px + env(safe-area-inset-bottom));
  background: color-mix(in srgb, var(--color-surface-card) 85%, transparent);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border-top: 1px solid var(--color-border);
}

.send-btn {
  width: 100%;
  height: 48px;
  border: none;
  border-radius: var(--radius-md);
  background: var(--color-brand-primary);
  color: var(--color-text-inverse);
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  cursor: pointer;
  transition: background var(--transition-fast), opacity var(--transition-fast);
}

.send-btn:hover:not(:disabled) {
  background: var(--color-brand-primary-hover);
}

.send-btn:active:not(:disabled) {
  background: var(--color-brand-primary-active);
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.send-btn--cancel {
  background: var(--color-state-error);
}

.send-btn--cancel:hover {
  background: color-mix(in srgb, var(--color-state-error) 85%, #000);
}

/* Upload Progress */
.upload-progress-card {
  padding: 14px 16px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-surface-card);
}

.upload-progress-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}

.retry-badge {
  margin-left: auto;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-warning, #b45309);
  background: var(--color-warning-soft, rgba(180, 83, 9, 0.12));
  padding: 2px 8px;
  border-radius: var(--radius-full);
}

.upload-progress-name {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.upload-progress-bar {
  height: 6px;
  border-radius: var(--radius-full);
  background: var(--color-surface-inset);
  overflow: hidden;
  margin-bottom: 8px;
}

.upload-progress-fill {
  height: 100%;
  border-radius: var(--radius-full);
  background: var(--color-brand-primary);
  transition: width 0.3s ease;
}

.upload-progress-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 12px;
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  font-family: var(--font-mono);
}

.spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}

/* Success Card */
.success-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 24px 16px;
  border: 1px solid var(--color-state-success);
  border-radius: var(--radius-lg);
  background: var(--color-state-success-soft);
}

.success-icon {
  color: var(--color-state-success);
}

.success-title {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
}

.success-hint {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

/* Error Message */
.error-msg {
  margin: 10px 0 0;
  font-size: var(--text-sm);
  color: var(--color-state-error);
}

/* Warning Message */
.warning-msg {
  margin: 10px 0 0;
  font-size: var(--text-sm);
  color: var(--color-state-warning);
}

/* Relay Status */
.relay-status-card {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 16px;
  border: 1px solid var(--color-state-warning);
  border-radius: var(--radius-lg);
  background: var(--color-state-warning-soft);
}

.relay-icon {
  color: var(--color-state-warning);
  flex-shrink: 0;
}

.relay-text {
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: var(--color-text-primary);
}
</style>
