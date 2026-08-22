<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { useSettingsStore } from "./stores/settings";
import { useAppStore } from "./stores/app";
import { isTauri, openReceiveFolder, quitApplication } from "./services/tauri";
import { useLegalConsent } from "./composables/useLegalConsent";
import ToastHost from "./components/overlays/ToastHost.vue";
import ConfirmDialog from "./components/common/ConfirmDialog.vue";
import LegalConsentDialog from "./components/legal/LegalConsentDialog.vue";
import FirstLaunchGuide from "./components/onboarding/FirstLaunchGuide.vue";
import { useLocale } from "./i18n";

const settingsStore = useSettingsStore();
const appStore = useAppStore();
const router = useRouter();
const isDesktopApp = isTauri();
const { t } = useLocale();
const {
  status: legalConsentStatus,
  accept: acceptLegalConsent,
  decline: declineLegalConsent,
  reconsider: reconsiderLegalConsent,
} = useLegalConsent();

// Tray event subscriptions (Tauri mode only); cleaned up on unmount
let unlisteners: UnlistenFn[] = [];

onMounted(async () => {
  settingsStore.applyTheme();

  // Surface WebSocket connection loss / recovery to the user (BUG #6).
  appStore.setupConnectionMonitor();

  // The Rust tray emits these events; react to them so the tray menu works.
  if (isTauri()) {
    try {
      unlisteners = await Promise.all([
        listen("tray-start-service", () => {
          void appStore.startServer();
        }),
        listen("tray-stop-service", () => {
          void appStore.stopServer();
        }),
        listen("tray-open-folder", () => {
          void openReceiveFolder();
        }),
        listen<string>("navigate", (event) => {
          const path = typeof event.payload === "string" ? event.payload : "";
          if (path) {
            void router.push(path);
          }
        }),
        listen("close-requested", () => {
          // audit-18: native window.confirm replaced by the in-app dialog so
          // the confirmation matches the rest of the product's modals.
          quitPromptVisible.value = true;
        }),
      ]);
    } catch (err) {
      console.error("[app] Failed to subscribe to tray events:", err);
    }
  }
});

const quitPromptVisible = ref(false);

function confirmQuit() {
  quitPromptVisible.value = false;
  void quitApplication();
}

onUnmounted(() => {
  for (const unlisten of unlisteners) {
    unlisten();
  }
  unlisteners = [];
});
</script>

<template>
  <div v-if="appStore.connectionLost" class="reconnect-banner">
    <span>{{ t("app.connectionLost") }}</span>
    <button class="reconnect-btn" @click="appStore.manualReconnect()">{{ t("app.reconnect") }}</button>
  </div>
  <router-view />
  <ToastHost />
  <LegalConsentDialog
    v-if="isDesktopApp"
    :status="legalConsentStatus"
    @accept="acceptLegalConsent"
    @decline="declineLegalConsent"
    @reconsider="reconsiderLegalConsent"
  />
  <ConfirmDialog
    :visible="quitPromptVisible"
    :title="t('settings.closeConfirm')"
    :confirm-label="t('settings.closeQuit')"
    tone="primary"
    @confirm="confirmQuit"
    @cancel="quitPromptVisible = false"
  />
  <FirstLaunchGuide v-if="isDesktopApp && legalConsentStatus === 'accepted'" />
</template>

<style scoped>
.reconnect-banner {
  position: fixed;
  top: 12px;
  left: 50%;
  transform: translateX(-50%);
  z-index: var(--z-toast, 1000);
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 16px;
  border-radius: var(--radius-md, 8px);
  background: var(--color-state-error, #d94e4e);
  color: #fff;
  font-size: var(--text-sm, 13px);
  box-shadow: var(--shadow-float, 0 4px 12px rgba(0, 0, 0, 0.15));
}

.reconnect-btn {
  padding: 4px 12px;
  border: 1px solid rgba(255, 255, 255, 0.6);
  border-radius: var(--radius-sm, 4px);
  background: transparent;
  color: #fff;
  font-size: var(--text-xs, 12px);
  cursor: pointer;
  white-space: nowrap;
  transition: background var(--transition-fast, 140ms ease);
}

.reconnect-btn:hover {
  background: rgba(255, 255, 255, 0.15);
}
</style>
