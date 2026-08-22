<script setup lang="ts">
import { computed, onMounted, onUnmounted, provide, ref, shallowRef } from "vue";
import { RouterLink, RouterView } from "vue-router";
import {
  Home,
  ArrowLeftRight,
  Download,
  Monitor,
  Settings,
  HelpCircle,
} from "lucide-vue-next";
import { useAppStore } from "../stores/app";
import { useDevicesStore } from "../stores/devices";
import { useTransfersStore } from "../stores/transfers";
import { useSettingsStore } from "../stores/settings";
import ThemeToggle from "../components/common/ThemeToggle.vue";
import AppLogo from "../components/common/AppLogo.vue";
import ConfirmDialog from "../components/common/ConfirmDialog.vue";
import ConnectDevicePanel from "../components/overlays/ConnectDevicePanel.vue";
import DeviceAccessRequestDialog from "../components/overlays/DeviceAccessRequestDialog.vue";
import { openConnectPanelKey } from "../composables/useConnectPanel";
import { wsClient } from "@/services/websocket";
import { useLocale } from "@/i18n";
import { APP_NAME } from "@/config/brand";

const appStore = useAppStore();
const devicesStore = useDevicesStore();
const transfersStore = useTransfersStore();
const settingsStore = useSettingsStore();
const { t } = useLocale();

const showConnectPanel = ref(false);
const accessDecisionPending = shallowRef(false);
const pendingDeviceAccess = computed(() => devicesStore.currentPendingApproval);

function refreshDevicesAfterSocketConnect() {
  void devicesStore.fetchDevices();
}

const emit = defineEmits<{
  (e: "connect-device"): void;
}>();

onMounted(async () => {
  await appStore.initialize();
  await settingsStore.fetchSettings();
  // Subscribe before opening the socket. Otherwise a phone that reconnects
  // during startup can emit its approval request before the desktop listens.
  devicesStore.setupWebSocketListeners();
  transfersStore.setupWebSocketListeners();
  wsClient.on("connected", refreshDevicesAfterSocketConnect);
  appStore.setupConnectionMonitor();
  await appStore.startServer();
  appStore.connectWebSocket();
  await devicesStore.fetchDevices();
  await transfersStore.fetchTransfers();
});

onUnmounted(() => {
  wsClient.off("connected", refreshDevicesAfterSocketConnect);
});

function openConnectPanel() {
  showConnectPanel.value = true;
}

// audit-13: stopping the service kills in-flight transfers, so warn first.
const stopServiceConfirmVisible = ref(false);
const activeTransferCount = computed(() => transfersStore.activeTransfers.length);

function handleServiceToggle() {
  if (appStore.serverRunning && activeTransferCount.value > 0) {
    stopServiceConfirmVisible.value = true;
    return;
  }
  void appStore.toggleServer();
}

function confirmStopService() {
  stopServiceConfirmVisible.value = false;
  void appStore.toggleServer();
}

// Let nested pages (e.g. HomePage) open the connect panel
provide(openConnectPanelKey, openConnectPanel);

function handleConnectDevice() {
  showConnectPanel.value = !showConnectPanel.value;
  emit("connect-device");
}

async function allowDeviceAccess(deviceId: string, trusted: boolean, expiryHours?: number) {
  const deviceName = devicesStore.devices.find((device) => device.id === deviceId)?.name ?? t("device.thisDevice");
  accessDecisionPending.value = true;
  try {
    const succeeded = await devicesStore.approveDevice(deviceId, trusted, expiryHours);
    if (succeeded) {
      appStore.pushToast(
        "success",
        trusted ? t("device.trusted") : t("device.allowedOnce"),
        trusted
          ? t("device.trustedDescription", { name: deviceName })
          : t("device.allowedOnceDescription", { name: deviceName })
      );
    } else {
      appStore.pushToast("error", t("device.permissionUpdateFailed"), t("device.permissionUpdateFailedDescription"));
    }
  } finally {
    accessDecisionPending.value = false;
    await devicesStore.fetchDevices();
  }
}

async function rejectDeviceAccess(deviceId: string) {
  const deviceName = devicesStore.devices.find((device) => device.id === deviceId)?.name ?? t("device.thisDevice");
  accessDecisionPending.value = true;
  try {
    const succeeded = await devicesStore.rejectDevice(deviceId);
    if (succeeded) {
      appStore.pushToast("info", t("device.rejected"), t("device.rejectedDescription", { name: deviceName }));
    } else {
      appStore.pushToast("error", t("device.permissionUpdateFailed"), t("device.permissionUpdateFailedDescription"));
    }
  } finally {
    accessDecisionPending.value = false;
    await devicesStore.fetchDevices();
  }
}

const navItems = computed(() => [
  { label: t("nav.home"), icon: Home, path: "/" },
  { label: t("nav.transfers"), icon: ArrowLeftRight, path: "/transfers" },
  { label: t("nav.received"), icon: Download, path: "/received" },
  { label: t("nav.devices"), icon: Monitor, path: "/devices" },
  { label: t("nav.settings"), icon: Settings, path: "/settings" },
]);
</script>

<template>
  <div class="desktop-layout">
    <!-- Top Bar -->
    <header class="topbar">
      <div class="topbar-left">
        <div class="logo">
          <AppLogo :size="28" />
          <span class="logo-text">{{ APP_NAME }}</span>
        </div>
        <span class="network-badge">{{ appStore.networkName }}</span>
      </div>

      <div class="topbar-right">
        <span class="status-indicator">
          <span
            class="status-dot"
            :class="appStore.serverRunning ? 'status-dot--running' : 'status-dot--stopped'"
          ></span>
          <span class="status-label">{{ appStore.serverRunning ? t("app.running") : t("app.stopped") }}</span>
        </span>
        <button class="service-toggle-btn" type="button" @click="handleServiceToggle">
          {{ appStore.serverRunning ? t("app.stopService") : t("app.startService") }}
        </button>

        <span class="device-count-badge">
          {{ t("app.deviceCount", { count: devicesStore.onlineDevices.length }) }}
        </span>

        <button class="btn-primary" @click="handleConnectDevice">
          {{ t("app.connectDevice") }}
        </button>

        <ThemeToggle />

        <RouterLink to="/settings" class="icon-btn icon-btn--link" :title="t('nav.settings')">
          <Settings :size="16" />
        </RouterLink>

      </div>
    </header>

    <!-- Sidebar -->
    <aside class="sidebar">
      <nav class="sidebar-nav">
        <RouterLink
          v-for="item in navItems"
          :key="item.path"
          :to="item.path"
          class="nav-item"
          exact-active-class="nav-item--active"
        >
          <component :is="item.icon" :size="16" />
          <span>{{ item.label }}</span>
        </RouterLink>
      </nav>

      <div class="sidebar-footer">
        <RouterLink to="/about" class="version-link">
          <span class="version">v{{ appStore.appVersion }}</span>
        </RouterLink>
        <span class="tray-status">
          <span class="status-dot" :class="appStore.trayReady ? 'status-dot--running' : 'status-dot--stopped'"></span>
          {{ appStore.trayReady ? t("app.trayReady") : t("app.trayUnavailable") }}
        </span>
        <RouterLink to="/help" class="help-link">
          <HelpCircle :size="13" />
          {{ t("nav.help") }}
        </RouterLink>
      </div>
    </aside>

    <!-- Main Content -->
    <main class="main-content">
      <div class="content-wrapper">
        <RouterView />
      </div>
    </main>

    <!-- Connect Panel -->
    <ConnectDevicePanel
      :visible="showConnectPanel"
      @close="showConnectPanel = false"
    />
    <DeviceAccessRequestDialog
      :device="pendingDeviceAccess"
      :pending="accessDecisionPending"
      @allow="allowDeviceAccess"
      @reject="rejectDeviceAccess"
    />
    <ConfirmDialog
      :visible="stopServiceConfirmVisible"
      :title="t('app.stopServiceConfirmTitle')"
      :description="t('app.stopServiceConfirmDescription', { count: activeTransferCount })"
      :confirm-label="t('app.stopService')"
      tone="danger"
      @confirm="confirmStopService"
      @cancel="stopServiceConfirmVisible = false"
    />
  </div>
</template>

<style scoped>
.desktop-layout {
  min-height: 100vh;
}

/* ─── Top Bar ─── */
.topbar {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: var(--topbar-height);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 20px;
  background: color-mix(in srgb, var(--color-surface-card) 80%, transparent);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border-bottom: 1px solid var(--color-border);
  z-index: var(--z-sticky);
}

.topbar-left {
  display: flex;
  align-items: center;
  gap: 12px;
}

.logo {
  display: flex;
  align-items: center;
  gap: 8px;
}

.logo-text {
  font-size: var(--text-md);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  letter-spacing: 0.02em;
}

.network-badge {
  font-size: var(--text-xs);
  color: var(--color-text-brand);
  background: var(--color-brand-primary-soft);
  padding: 2px 8px;
  border-radius: var(--radius-full);
  font-weight: var(--weight-medium);
}

.topbar-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.status-indicator {
  display: flex;
  align-items: center;
  gap: 6px;
}

.status-dot {
  width: 7px;
  height: 7px;
  border-radius: var(--radius-full);
}

.status-dot--running {
  background: var(--color-state-success);
  box-shadow: 0 0 0 2px var(--color-state-success-soft);
}

.status-dot--stopped {
  background: var(--color-text-tertiary);
}

.status-label {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
}

.service-toggle-btn {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-surface-card);
  color: var(--color-text-brand);
  padding: 5px 10px;
  font-size: var(--text-xs);
  cursor: pointer;
}

.service-toggle-btn:hover {
  background: var(--color-hover);
  border-color: var(--color-brand-primary);
}

.device-count-badge {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  background: var(--color-surface-inset);
  padding: 3px 8px;
  border-radius: var(--radius-full);
}

.btn-primary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  color: #fff;
  background: var(--color-brand-primary);
  border: none;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: background var(--transition-fast);
}

.btn-primary:hover {
  background: var(--color-brand-primary-hover);
}

.btn-primary:active {
  background: var(--color-brand-primary-active);
}

.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
  transition: background var(--transition-fast), color var(--transition-fast);
}

.icon-btn:hover {
  background: var(--color-hover);
  color: var(--color-text-secondary);
}

.icon-btn--link {
  text-decoration: none;
}

/* ─── Sidebar ─── */
.sidebar {
  position: fixed;
  top: var(--topbar-height);
  left: 0;
  bottom: 0;
  width: var(--sidebar-width);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 16px 12px;
  background: var(--color-surface-card);
  border-right: 1px solid var(--color-border);
  z-index: var(--z-base);
}

.sidebar-nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  border-radius: var(--radius-md);
  border-left: 2.5px solid transparent;
  font-size: var(--text-base);
  color: var(--color-text-secondary);
  text-decoration: none;
  transition: background var(--transition-fast), color var(--transition-fast),
    border-color var(--transition-fast);
}

.nav-item:hover {
  background: var(--color-hover);
  color: var(--color-text-primary);
}

.nav-item--active {
  color: var(--color-brand-primary);
  background: var(--color-brand-primary-soft);
  border-left-color: var(--color-brand-primary);
  font-weight: var(--weight-medium);
}

/* ─── Sidebar Footer ─── */
.sidebar-footer {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-top: 12px;
  border-top: 1px solid var(--color-border);
}

.version {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.version-link {
  text-decoration: none;
  transition: color var(--transition-fast);
}

.version-link:hover .version {
  color: var(--color-text-brand);
}

.tray-status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.help-link {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
  text-decoration: none;
  transition: color var(--transition-fast);
}

.help-link:hover {
  color: var(--color-text-brand);
}

/* ─── Main Content ─── */
.main-content {
  margin-left: var(--sidebar-width);
  padding-top: var(--topbar-height);
  min-height: 100vh;
}

.content-wrapper {
  max-width: 1100px;
  margin: 0 auto;
  padding: 24px 32px;
}
</style>
