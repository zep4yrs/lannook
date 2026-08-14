<script setup lang="ts">
import { computed } from "vue";
import { X, Check, Smartphone, Laptop, Monitor, Tablet } from "lucide-vue-next";
import { useDevicesStore } from "@/stores/devices";
import { useAppStore } from "@/stores/app";
import { useLocale } from "@/i18n";
import type { Device } from "@/types";

defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  close: [];
  select: [deviceId: string];
}>();

const devicesStore = useDevicesStore();
const appStore = useAppStore();
const { t } = useLocale();
const devices = computed(() => devicesStore.devices);
const selectedDeviceId = computed(() => devicesStore.selectedDeviceId);

function getDeviceIcon(device: Device) {
  switch (device.deviceType) {
    case "phone":
      return Smartphone;
    case "laptop":
      return Laptop;
    case "desktop":
      return Monitor;
    case "tablet":
      return Tablet;
    default:
      return Smartphone;
  }
}

function getDeviceMeta(device: Device): string {
  const platformMap: Record<string, string> = {
    macos: "macOS",
    ios: "iOS",
    android: "Android",
    windows: "Windows",
    web: "Web",
  };
  const platform = platformMap[device.platform] || device.platform;
  const status = device.online ? t("device.online") : t("device.offline");
  return `${platform} · ${status}`;
}

function handleSelect(device: Device) {
  if (!device.online) {
    appStore.pushToast("warning", t("device.offlineTitle"), t("device.offlineSendDescription", { name: device.name }));
    return;
  }
  emit("select", device.id);
  emit("close");
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="sheet-wrapper">
      <div class="backdrop" @click="emit('close')" />
      <div class="sheet">
        <div class="drag-handle" />
        <div class="sheet-header">
          <span class="sheet-title">{{ t("device.selectTitle") }}</span>
          <button class="close-btn" @click="emit('close')">
            <X :size="16" />
          </button>
        </div>
        <div class="device-list">
          <p v-if="devices.length === 0" class="empty-state">
            {{ t("device.noDevices") }}
          </p>
          <button
            v-for="device in devices"
            :key="device.id"
            class="device-item"
            :class="{ selected: device.id === selectedDeviceId, offline: !device.online }"
            :disabled="!device.online"
            @click="handleSelect(device)"
          >
            <div class="device-icon">
              <component :is="getDeviceIcon(device)" :size="18" />
            </div>
            <div class="device-info">
              <span class="device-name">{{ device.name }}</span>
              <span class="device-meta">{{ getDeviceMeta(device) }}</span>
            </div>
            <Check
              v-if="device.id === selectedDeviceId"
              :size="16"
              class="check-icon"
            />
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.sheet-wrapper {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
  display: flex;
  align-items: flex-end;
}

.backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.3);
  animation: fade-in 180ms ease forwards;
}

.sheet {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  min-height: 45vh;
  background: var(--color-surface-card);
  border-radius: 16px 16px 0 0;
  box-shadow: var(--shadow-xl);
  padding: 12px 20px calc(20px + env(safe-area-inset-bottom));
  animation: slide-down-sheet 260ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
  display: flex;
  flex-direction: column;
}

.drag-handle {
  width: 36px;
  height: 4px;
  border-radius: var(--radius-full);
  background: var(--color-border-strong);
  margin: 0 auto 16px;
}

.sheet-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.sheet-title {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
}

.close-btn {
  width: 28px;
  height: 28px;
  border-radius: var(--radius-full);
  border: none;
  background: var(--color-surface-inset);
  color: var(--color-text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background var(--transition-fast);
}

.close-btn:hover {
  background: var(--color-active);
}

.device-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-y: auto;
  flex: 1;
}

.empty-state {
  margin: 24px 0;
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  line-height: 1.6;
  text-align: center;
}

.device-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px;
  border-radius: var(--radius-md);
  border: none;
  background: transparent;
  cursor: pointer;
  text-align: left;
  width: 100%;
  transition: background var(--transition-fast);
  border-left: 3px solid transparent;
}

.device-item:hover {
  background: var(--color-hover);
}

.device-item.selected {
  background: var(--color-brand-primary-soft);
  border-left-color: var(--color-state-success);
}

.device-item.offline {
  opacity: 0.5;
  cursor: not-allowed;
}

.device-item:disabled {
  pointer-events: none;
}

.device-icon {
  width: 40px;
  height: 40px;
  border-radius: var(--radius-full);
  background: var(--color-surface-inset);
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--color-text-secondary);
  flex-shrink: 0;
}

.device-item.selected .device-icon {
  background: var(--color-brand-primary-soft);
  color: var(--color-brand-primary);
}

.device-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 0;
}

.device-name {
  font-size: var(--text-base);
  font-weight: var(--weight-medium);
  color: var(--color-text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.device-meta {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.check-icon {
  color: var(--color-state-success);
  flex-shrink: 0;
}

@keyframes fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes slide-down-sheet {
  from {
    transform: translateY(100%);
  }
  to {
    transform: translateY(0);
  }
}
</style>
