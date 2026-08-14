<script setup lang="ts">
import { computed, nextTick, shallowRef, watch } from "vue";
import { Check, Copy, LoaderCircle, RefreshCw, Wifi, X } from "lucide-vue-next";
import { useAppStore } from "@/stores/app";
import { useSettingsStore } from "@/stores/settings";
import { useConnectionDiagnostics } from "@/composables/useConnectionDiagnostics";
import { useLocale } from "@/i18n";
import { configureWindowsFirewall } from "@/services/tauri";
import ConnectionDiagnosticsPanel from "./ConnectionDiagnosticsPanel.vue";
import ConnectionAddressPicker from "./ConnectionAddressPicker.vue";

const props = defineProps<{
  visible: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

const appStore = useAppStore();
const settingsStore = useSettingsStore();
const { t } = useLocale();
const {
  diagnostics,
  loading: diagnosticsLoading,
  error: diagnosticsError,
  refresh: refreshDiagnostics,
} = useConnectionDiagnostics();

const copiedField = shallowRef<string | null>(null);
const settingPending = shallowRef(false);
const firewallPending = shallowRef(false);
const panelElement = shallowRef<HTMLElement | null>(null);
const closeButton = shallowRef<HTMLButtonElement | null>(null);
let previousFocus: HTMLElement | null = null;
let panelRefresh: Promise<void> | null = null;

const completeLanUrl = computed(() => {
  if (!appStore.serverRunning) return null;
  return appStore.qrCode?.url || null;
});

const mdnsUrl = computed(() => {
  const info = appStore.connectionInfo;
  if (
    !appStore.serverRunning
    || !info?.localDomain
    || !info.token
    || appStore.selectedConnectionIp !== info.ip
  ) return null;
  return `http://${info.localDomain}:${info.port}/mobile?token=${encodeURIComponent(info.token)}`;
});

const receiveFolder = computed(() => appStore.connectionInfo?.receiveFolder || null);

const pairingPin = computed(() => appStore.connectionInfo?.pin || null);

async function refreshPin() {
  await appStore.refreshPairingPinCode();
}

async function refreshPanelData() {
  if (panelRefresh) return panelRefresh;
  panelRefresh = (async () => {
    await appStore.refreshConnectionData();
    // Auto-generate a pairing PIN the first time the panel opens.
    if (appStore.serverRunning && !appStore.connectionInfo?.pin) {
      await appStore.refreshPairingPinCode();
    }
    await refreshDiagnostics(appStore.selectedConnectionIp || undefined);
  })().finally(() => {
    panelRefresh = null;
  });
  return panelRefresh;
}

watch(
  () => props.visible,
  (visible, _previous, onCleanup) => {
    if (!visible) {
      previousFocus?.focus();
      previousFocus = null;
      return;
    }

    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void nextTick(() => closeButton.value?.focus());
    void refreshPanelData();
    const timer = window.setInterval(async () => {
      const previousIp = appStore.selectedConnectionIp;
      await appStore.refreshConnectionData({ silent: true });
      if (previousIp !== appStore.selectedConnectionIp) {
        await refreshDiagnostics(appStore.selectedConnectionIp || undefined);
      }
    }, 3000);
    onCleanup(() => window.clearInterval(timer));
  }
);

async function toggleRequireConfirm() {
  if (settingPending.value) return;
  settingPending.value = true;
  const saved = await settingsStore.setRequireApproval(!settingsStore.requireApproval);
  settingPending.value = false;
  if (!saved) {
    appStore.pushToast(
      "error",
      t("connect.settingSaveFailed"),
      t("connect.settingSaveFailedDescription")
    );
  }
}

async function copyToClipboard(text: string | null, field: string) {
  if (!text) return;
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
    } else {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.style.position = "fixed";
      textarea.style.opacity = "0";
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand("copy");
      document.body.removeChild(textarea);
    }
    copiedField.value = field;
    window.setTimeout(() => {
      if (copiedField.value === field) copiedField.value = null;
    }, 2000);
  } catch (error) {
    appStore.pushToast(
      "error",
      t("connect.copyFailed"),
      error instanceof Error ? error.message : undefined
    );
  }
}

async function selectAddress(ip: string) {
  try {
    await appStore.selectConnectionAddress(ip);
    await refreshDiagnostics(ip);
  } catch (error) {
    appStore.pushToast(
      "error",
      t("connect.addressPicker.failed"),
      error instanceof Error && error.message === "selected_address_inactive"
        ? t("connect.warning.selected_address_inactive")
        : error instanceof Error ? error.message : undefined
    );
    await appStore.refreshConnectionData();
  }
}

async function configureFirewall() {
  if (firewallPending.value) return;
  firewallPending.value = true;
  try {
    const result = await configureWindowsFirewall();
    if (!result.success) throw new Error(result.error ?? t("connect.diagnostics.firewallConfigureFailed"));
    appStore.pushToast("success", t("connect.diagnostics.firewallConfigured"));
    await refreshDiagnostics(appStore.selectedConnectionIp || undefined);
  } catch (error) {
    appStore.pushToast(
      "error",
      t("connect.diagnostics.firewallConfigureFailed"),
      error instanceof Error ? error.message : undefined
    );
  } finally {
    firewallPending.value = false;
  }
}

function trapFocus(event: KeyboardEvent) {
  const panel = panelElement.value;
  if (!panel) return;
  const focusable = Array.from(
    panel.querySelectorAll<HTMLElement>(
      'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])'
    )
  );
  if (!focusable.length) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="visible"
      class="panel-wrapper"
      @keydown.esc.stop="emit('close')"
      @keydown.tab="trapFocus"
    >
      <div class="backdrop" aria-hidden="true" @click="emit('close')" />
      <aside
        ref="panelElement"
        class="panel"
        role="dialog"
        aria-modal="true"
        :aria-label="t('connect.title')"
      >
        <div class="panel-header">
          <span class="panel-title">{{ t("connect.title") }}</span>
          <button ref="closeButton" class="close-btn" :aria-label="t('connect.close')" @click="emit('close')">
            <X :size="16" />
          </button>
        </div>

        <div class="qr-section">
          <div class="qr-code">
            <div
              v-if="appStore.serverRunning && appStore.qrCode?.svg"
              v-html="appStore.qrCode.svg"
              class="qr-svg"
            />
            <div v-else class="qr-empty">
              <LoaderCircle v-if="appStore.connectionInfoLoading" :size="22" class="spin" />
              <span v-else>
                {{ appStore.serverRunning ? t("connect.qrUnavailable") : t("connect.serviceStopped") }}
              </span>
            </div>
          </div>
          <p class="qr-hint">{{ t("connect.qrHint") }}</p>
        </div>

        <p v-if="appStore.connectionInfoError" class="panel-error">
          {{ appStore.connectionInfoError }}
        </p>

        <ConnectionAddressPicker
          :addresses="appStore.connectionInfo?.addresses ?? []"
          :selected-ip="appStore.selectedConnectionIp"
          :loading="appStore.connectionInfoLoading"
          @select="selectAddress"
        />

        <p class="browser-warning">
          {{ t("connect.browserPrivateNetworkWarning") }}
        </p>

        <div class="info-section">
          <div class="info-row">
            <span class="info-label">{{ t("connect.completeAddress") }}</span>
            <div class="info-value-group">
              <span class="info-value">{{ completeLanUrl ?? t("connect.unavailable") }}</span>
              <button
                class="copy-btn"
                :disabled="!completeLanUrl"
                :aria-label="t('connect.copyCompleteAddress')"
                @click="copyToClipboard(completeLanUrl, 'lan')"
              >
                <Check v-if="copiedField === 'lan'" :size="13" class="copied" />
                <Copy v-else :size="13" />
              </button>
            </div>
          </div>
          <div class="info-row">
            <span class="info-label">{{ t("connect.mdnsAddress") }}</span>
            <div class="info-value-group">
              <span class="info-value">{{ mdnsUrl ?? t("connect.unavailable") }}</span>
              <button
                class="copy-btn"
                :disabled="!mdnsUrl"
                :aria-label="t('connect.copyMdnsAddress')"
                @click="copyToClipboard(mdnsUrl, 'mdns')"
              >
                <Check v-if="copiedField === 'mdns'" :size="13" class="copied" />
                <Copy v-else :size="13" />
              </button>
            </div>
          </div>
        </div>

        <p class="add-to-home-hint">{{ t("connect.addToHome") }}</p>

        <div v-if="pairingPin" class="pin-section">
          <div class="pin-row">
            <div class="pin-info">
              <span class="pin-label">{{ t("connect.pairingPin") }}</span>
              <small class="pin-hint">{{ t("connect.pairingPinHint") }}</small>
            </div>
            <div class="pin-controls">
              <span class="pin-value">{{ pairingPin }}</span>
              <button
                class="pin-refresh"
                :aria-label="t('connect.pairingPinRefresh')"
                :title="t('connect.pairingPinRefresh')"
                @click="refreshPin"
              >
                <RefreshCw :size="13" />
              </button>
            </div>
          </div>
        </div>

        <div class="settings-section">
          <div class="setting-row">
            <div>
              <span class="setting-label">{{ t("connect.requireApproval") }}</span>
              <small>{{ t("connect.requireApprovalHint") }}</small>
            </div>
            <button
              class="toggle"
              :class="{ active: settingsStore.requireApproval }"
              :disabled="settingPending"
              role="switch"
              :aria-checked="settingsStore.requireApproval"
              :aria-label="t('connect.requireApproval')"
              @click="toggleRequireConfirm"
            >
              <span class="toggle-knob" />
            </button>
          </div>
          <div class="setting-row">
            <span class="setting-label">{{ t("connect.receiveFolder") }}</span>
            <span class="setting-value">{{ receiveFolder ?? t("connect.unavailable") }}</span>
          </div>
        </div>

        <ConnectionDiagnosticsPanel
          :diagnostics="diagnostics"
          :loading="diagnosticsLoading"
          :error="diagnosticsError"
          :firewall-pending="firewallPending"
          @retry="refreshPanelData"
          @configure-firewall="configureFirewall"
        />

        <div class="network-badge">
          <Wifi :size="12" />
          <span>{{ appStore.networkName }}</span>
        </div>
      </aside>
    </div>
  </Teleport>
</template>

<style scoped>
.panel-wrapper { position: fixed; inset: 0; z-index: var(--z-overlay); }
.backdrop { position: fixed; inset: 0; border: 0; background: rgba(0, 0, 0, 0.12); animation: fade-in 180ms ease forwards; }
.panel {
  position: fixed;
  top: calc(var(--topbar-height) + 6px);
  right: 32px;
  width: min(400px, calc(100vw - 32px));
  max-height: calc(100vh - var(--topbar-height) - 22px);
  overflow-y: auto;
  padding: 20px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-surface-card);
  box-shadow: var(--shadow-lg);
  animation: panel-in 200ms ease forwards;
}
.panel-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 14px; }
.panel-title { color: var(--color-text-primary); font-size: var(--text-base); font-weight: var(--weight-semibold); }
.close-btn, .copy-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
}
.close-btn { width: 28px; height: 28px; }
.copy-btn { flex: 0 0 24px; width: 24px; height: 24px; }
.close-btn:hover, .copy-btn:hover:not(:disabled) { background: var(--color-hover); color: var(--color-text-primary); }
.copy-btn:disabled { opacity: 0.35; cursor: default; }
.copied { color: var(--color-state-success); }
.qr-section { display: flex; flex-direction: column; align-items: center; margin-bottom: 15px; }
.qr-code {
  display: flex;
  width: 160px;
  height: 160px;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: #fff;
}
.qr-svg { display: flex; width: 140px; height: 140px; align-items: center; justify-content: center; }
.qr-svg :deep(svg) { width: 100%; height: 100%; }
.qr-empty { display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 14px; color: var(--color-text-tertiary); font-size: var(--text-xs); text-align: center; }
.qr-hint { margin: 8px 0 0; color: var(--color-text-tertiary); font-size: var(--text-xs); }
.panel-error { margin: 0 0 12px; padding: 8px 10px; border-radius: var(--radius-sm); background: var(--color-state-error-soft); color: var(--color-state-error); font-size: var(--text-xs); }
.browser-warning { margin: 0 0 14px; padding: 8px 10px; border-radius: var(--radius-sm); background: var(--color-state-warning-soft); color: var(--color-text-secondary); font-size: 11px; line-height: 1.5; }
.info-section, .settings-section, .add-to-home-hint { margin: 0 0 12px; color: var(--color-text-tertiary); font-size: 11px; line-height: 1.5; }
.pin-section { display: flex; flex-direction: column; gap: 11px; margin-bottom: 14px; }
.pin-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.pin-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.pin-label { color: var(--color-text-secondary); font-size: var(--text-xs); }
.pin-hint { color: var(--color-text-tertiary); font-size: 11px; line-height: 1.4; }
.pin-controls { display: flex; align-items: center; gap: 8px; flex: 0 0 auto; }
.pin-value {
  font-family: var(--font-mono);
  font-size: 22px;
  font-weight: 700;
  letter-spacing: 0.2em;
  color: var(--color-brand-primary);
}
.pin-refresh {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
}
.pin-refresh:hover { background: var(--color-hover); color: var(--color-text-secondary); }
.pin-refresh:active { background: var(--color-active); }
.info-row, .setting-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.info-label, .setting-label { flex: 0 0 auto; color: var(--color-text-secondary); font-size: var(--text-xs); }
.info-value-group { display: flex; min-width: 0; align-items: center; gap: 4px; }
.info-value, .setting-value {
  overflow: hidden;
  color: var(--color-text-primary);
  font-family: var(--font-mono);
  font-size: 11px;
  text-align: right;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.setting-row > div { display: flex; flex-direction: column; gap: 2px; }
.setting-row small { color: var(--color-text-tertiary); font-size: 10px; }
.toggle { position: relative; flex: 0 0 36px; width: 36px; height: 20px; padding: 0; border: 0; border-radius: var(--radius-full); background: var(--color-border-strong); cursor: pointer; transition: background var(--transition-normal); }
.toggle.active { background: var(--color-state-success); }
.toggle:disabled { opacity: 0.55; cursor: default; }
.toggle-knob { position: absolute; top: 2px; left: 2px; width: 16px; height: 16px; border-radius: var(--radius-full); background: #fff; box-shadow: var(--shadow-sm); transition: transform var(--transition-normal); }
.toggle.active .toggle-knob { transform: translateX(16px); }
.network-badge { display: inline-flex; align-items: center; gap: 6px; margin-top: 14px; padding: 5px 10px; border-radius: var(--radius-full); background: var(--color-brand-primary-soft); color: var(--color-text-brand); font-size: var(--text-xs); font-weight: var(--weight-medium); }
.spin { animation: panel-spin 0.9s linear infinite; }
@keyframes panel-spin { to { transform: rotate(360deg); } }
@keyframes fade-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes panel-in { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: translateY(0); } }
@media (max-width: 600px) { .panel { top: 8px; right: 8px; width: calc(100vw - 16px); max-height: calc(100vh - 16px); } }
</style>
