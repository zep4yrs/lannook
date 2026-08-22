<script setup lang="ts">
import {
  Smartphone,
  Laptop,
  Monitor,
  Tablet,
  ShieldCheck,
  ShieldOff,
} from "lucide-vue-next";
import { computed, onMounted, shallowRef } from "vue";
import { useDevicesStore } from "@/stores/devices";
import DeviceAuthorizationActions from "@/components/devices/DeviceAuthorizationActions.vue";
import ConfirmDialog from "@/components/common/ConfirmDialog.vue";
import { formatRelativeTime } from "@/utils/format";
import type { Device } from "@/types";
import { useLocale } from "@/i18n";

const devicesStore = useDevicesStore();
const { t } = useLocale();
const pendingDeviceId = shallowRef<string | null>(null);
const actionError = shallowRef<string | null>(null);
// audit-12: forgetting a device requires re-pairing, so confirm first.
const forgetTarget = shallowRef<Device | null>(null);

onMounted(() => {
  void devicesStore.fetchDevices();
});

const lastSeenLabel = computed(() =>
  forgetTarget.value?.lastSeenAt
    ? formatRelativeTime(forgetTarget.value.lastSeenAt)
    : ""
);

async function approve(deviceId: string, trusted: boolean) {
  pendingDeviceId.value = deviceId;
  actionError.value = null;
  const succeeded = await devicesStore.approveDevice(deviceId, trusted);
  pendingDeviceId.value = null;
  if (!succeeded) {
    actionError.value = t("devices.authorizationFailed");
    return;
  }
  await devicesStore.fetchDevices();
}

function requestForget(device: Device) {
  forgetTarget.value = device;
}

async function confirmForget() {
  const device = forgetTarget.value;
  forgetTarget.value = null;
  if (!device) return;
  pendingDeviceId.value = device.id;
  actionError.value = null;
  const succeeded = await devicesStore.forgetDevice(device.id);
  pendingDeviceId.value = null;
  if (!succeeded) actionError.value = t("devices.actionFailed");
}

function formatApprovalExpiry(value: string): string {
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) return value;
  const date = new Date(parsed);
  const diffMs = date.getTime() - Date.now();
  if (diffMs <= 0) return t("devices.expiresSoon");
  const days = Math.floor(diffMs / 86_400_000);
  if (days > 0) return t("devices.expiresInDays", { count: days });
  const hours = Math.ceil(diffMs / 3_600_000);
  if (hours > 0) return t("devices.expiresInHours", { count: hours });
  return date.toLocaleString();
}

async function reject(deviceId: string) {
  pendingDeviceId.value = deviceId;
  actionError.value = null;
  const succeeded = await devicesStore.rejectDevice(deviceId);
  pendingDeviceId.value = null;
  if (!succeeded) {
    actionError.value = t("devices.actionFailed");
    return;
  }
  await devicesStore.fetchDevices();
}

function getDeviceIcon(device: Device) {
  switch (device.deviceType) {
    case "phone":
      return Smartphone;
    case "laptop":
      return Laptop;
    case "tablet":
      return Tablet;
    default:
      return Monitor;
  }
}

function getPlatformLabel(platform: string): string {
  const map: Record<string, string> = {
    windows: "Windows",
    macos: "macOS",
    ios: "iOS",
    android: "Android",
    web: "Web",
  };
  return map[platform] ?? platform;
}
</script>

<template>
  <div class="devices-page">
    <header class="page-header">
      <h1 class="page-title">{{ t("devices.title") }}</h1>
      <p class="page-subtitle">{{ t("devices.pairedCount", { count: devicesStore.devices.length }) }}</p>
    </header>
    <p v-if="actionError" class="action-error">{{ actionError }}</p>

    <div class="devices-grid">
      <div
        v-for="device in devicesStore.devices"
        :key="device.id"
        class="device-card"
        :class="{ 'device-card--offline': !device.online }"
      >
        <div class="device-card-top">
          <div class="device-icon-wrap">
            <component :is="getDeviceIcon(device)" :size="22" />
          </div>
          <span
            class="online-dot"
            :class="device.online ? 'online-dot--on' : 'online-dot--off'"
          ></span>
        </div>

        <div class="device-card-body">
          <h3 class="device-name">{{ device.name }}</h3>
          <span class="device-platform">{{ getPlatformLabel(device.platform) }}</span>
        </div>

        <div class="device-card-meta">
          <div class="meta-row">
            <span class="meta-label">{{ t("devices.ip") }}</span>
            <span class="meta-value">{{ device.ip }}</span>
          </div>
          <div class="meta-row">
            <span class="meta-label">{{ t("devices.status") }}</span>
            <span class="meta-value" :class="device.online ? 'meta-value--online' : 'meta-value--offline'">
              {{ device.online ? t("home.online") : t("home.offline") }}
            </span>
          </div>
          <div class="meta-row">
            <span class="meta-label">{{ t("devices.authorization") }}</span>
            <span class="meta-value">
              <span v-if="device.trusted" class="approved-badge">
                <ShieldCheck :size="13" /> {{ t("devices.trusted") }}
              </span>
              <span v-else-if="device.approved" class="approved-badge">
                <ShieldCheck :size="13" /> {{ t("devices.allowedOnce") }}
              </span>
              <span v-else class="unapproved-badge">
                <ShieldOff :size="13" /> {{ t("devices.pendingApproval") }}
              </span>
            </span>
          </div>
          <div v-if="device.approved && !device.trusted && device.approvedUntil" class="meta-row">
            <span class="meta-label">{{ t("devices.expires") }}</span>
            <span class="meta-value">{{ formatApprovalExpiry(device.approvedUntil) }}</span>
          </div>
          <!-- audit-25: help users judge stale offline devices -->
          <div v-if="!device.online && device.lastSeenAt" class="meta-row">
            <span class="meta-label">{{ t("devices.lastSeen") }}</span>
            <span class="meta-value">{{ formatRelativeTime(device.lastSeenAt) }}</span>
          </div>
        </div>
        <DeviceAuthorizationActions
          :approved="device.approved"
          :trusted="device.trusted"
          :pending="pendingDeviceId === device.id"
          @allow-once="approve(device.id, false)"
          @trust="approve(device.id, true)"
          @reject="reject(device.id)"
          @forget="requestForget(device)"
        />
      </div>
    </div>

    <ConfirmDialog
      :visible="forgetTarget != null"
      :title="t('devices.forgetConfirmTitle')"
      :description="t('devices.forgetConfirmDescription', { name: forgetTarget?.name ?? '', seen: lastSeenLabel })"
      :confirm-label="t('devices.forget')"
      tone="danger"
      @confirm="confirmForget"
      @cancel="forgetTarget = null"
    />
  </div>
</template>

<style scoped>
.devices-page {
  padding: 32px;
  max-width: var(--content-max-width);
  margin: 0 auto;
}

.page-header {
  margin-bottom: 24px;
}

.action-error {
  margin: 0 0 16px;
  color: var(--color-state-error);
  font-size: var(--text-sm);
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

.devices-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 16px;
}

.device-card {
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
  padding: 20px;
  transition: all var(--transition-normal);
}

.device-card:hover {
  box-shadow: var(--shadow-md);
  border-color: var(--color-border-strong);
}

.device-card--offline {
  opacity: 0.6;
}

.device-card-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}

.device-icon-wrap {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 42px;
  height: 42px;
  border-radius: var(--radius-md);
  background: var(--color-selected);
  color: var(--color-brand-primary);
}

.online-dot {
  width: 9px;
  height: 9px;
  border-radius: var(--radius-full);
}

.online-dot--on {
  background: var(--color-state-success);
  box-shadow: 0 0 0 3px var(--color-state-success-soft);
}

.online-dot--off {
  background: var(--color-text-tertiary);
}

.device-card-body {
  margin-bottom: 16px;
}

.device-name {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  margin: 0 0 2px;
}

.device-platform {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.device-card-meta {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 14px;
  border-top: 1px solid var(--color-border);
}

.meta-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.meta-label {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.meta-value {
  font-size: var(--text-sm);
  color: var(--color-text-primary);
  font-family: var(--font-mono);
}

.meta-value--online {
  color: var(--color-state-success);
}

.meta-value--offline {
  color: var(--color-text-tertiary);
}

.approved-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-state-success);
  background: var(--color-state-success-soft);
  padding: 2px 8px;
  border-radius: var(--radius-full);
  font-family: var(--font-sans);
}

.unapproved-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  color: var(--color-text-tertiary);
  background: var(--color-surface-inset);
  padding: 2px 8px;
  border-radius: var(--radius-full);
  font-family: var(--font-sans);
}
</style>
