<script setup lang="ts">
import { computed, ref, onMounted, watch } from "vue";
import { storeToRefs } from "pinia";
import { RouterLink, RouterView, useRoute } from "vue-router";
import { Sun, Moon, Menu, Send, ArrowLeftRight, HelpCircle, Scale } from "lucide-vue-next";
import { useSettingsStore } from "../stores/settings";
import { useMobileSessionStore } from "@/stores/mobileSession";
import AppLogo from "../components/common/AppLogo.vue";
import ReceiveRequestDialog from "@/components/overlays/ReceiveRequestDialog.vue";
import { fetchHostStatus } from "../services/api";
import { useLocale } from "@/i18n";
import { APP_NAME } from "@/config/brand";

const settingsStore = useSettingsStore();
const mobileSession = useMobileSessionStore();
const route = useRoute();
const { t, locale, setLocale } = useLocale();
const {
  connectionError,
  connectionPhase,
  isReady,
  receiveError,
  pendingReceiveTransfer,
  showReceiveDialog,
} = storeToRefs(mobileSession);

const showMenu = ref(false);
const navItems = computed(() => [
  { name: "mobile-send", label: t("mobile.send"), icon: Send },
  { name: "mobile-transfers", label: t("mobile.transfers"), icon: ArrowLeftRight },
] as const);
const hostName = ref("");

const connectionLabel = computed(() => {
  switch (connectionPhase.value) {
    case "pin_entry":
      return t("mobile.pinEntry");
    case "connected":
      return t("mobile.connectedTo", { name: hostName.value || t("mobile.host") });
    case "pending_approval":
      return t("mobile.pendingApproval");
    case "reconnecting":
      return t("mobile.reconnecting");
    case "rejected":
      return t("mobile.rejected");
    case "revoked":
      return t("mobile.revoked");
    case "error":
      return t("mobile.connectionFailed");
    default:
      return isReady.value ? t("mobile.connecting") : t("mobile.initializing");
  }
});

const connectionTone = computed(() => {
  if (connectionPhase.value === "connected") return "online";
  if (["rejected", "revoked", "error"].includes(connectionPhase.value)) return "error";
  return "pending";
});

const canRequestAccess = computed(() =>
  ["rejected", "revoked"].includes(connectionPhase.value)
);

onMounted(async () => {
  settingsStore.applyTheme();
  try {
    const status = await fetchHostStatus();
    if (status.name) {
      hostName.value = status.name;
    }
  } catch {
    // Keep the default label if the host status cannot be fetched.
  }
});

watch(
  () => route.query.token,
  (token) => {
    void mobileSession.initialize(typeof token === "string" ? token : null);
  },
  { immediate: true }
);

const pinInput = ref("");

async function submitPin() {
  const ok = await mobileSession.submitPin(pinInput.value);
  if (ok) pinInput.value = "";
}

function toggleTheme() {
  const current = settingsStore.themeMode;
  if (current === "light") {
    settingsStore.setThemeMode("dark");
  } else {
    settingsStore.setThemeMode("light");
  }
}

function toggleMenu() {
  showMenu.value = !showMenu.value;
}

function closeMenu() {
  showMenu.value = false;
}
</script>

<template>
  <div class="mobile-layout">
    <!-- Top Bar -->
    <header class="mobile-topbar">
      <div class="topbar-left">
        <AppLogo :size="24" />
        <span class="logo-text">{{ APP_NAME }}</span>
      </div>

      <div class="topbar-center">
        <span class="status-dot" :class="`status-dot--${connectionTone}`"></span>
        <span class="status-text">{{ connectionLabel }}</span>
      </div>

      <div class="topbar-right">
        <button class="icon-btn" :title="t('mobile.toggleTheme')" @click="toggleTheme">
          <Sun v-if="settingsStore.getResolvedTheme() === 'dark'" :size="16" />
          <Moon v-else :size="16" />
        </button>
        <button class="icon-btn" :title="t('mobile.menu')" @click="toggleMenu">
          <Menu :size="16" />
        </button>
      </div>
    </header>

    <Teleport to="body">
      <div v-if="showMenu" class="mobile-menu-layer" @click.self="closeMenu">
        <button class="menu-backdrop" :aria-label="t('mobile.closeMenu')" @click="closeMenu" />
        <nav class="mobile-menu" :aria-label="t('mobile.navigation')">
          <RouterLink
            v-for="item in navItems"
            :key="item.name"
            class="menu-link"
            :to="{ name: item.name, query: route.query }"
            @click="closeMenu"
          >
            <component :is="item.icon" :size="18" />
            <span>{{ item.label }}</span>
          </RouterLink>
          <div class="menu-extra">
            <RouterLink class="menu-link" :to="{ name: 'mobile-help', query: route.query }" @click="closeMenu">
              <HelpCircle :size="18" />
              <span>{{ t("nav.help") }}</span>
            </RouterLink>
            <RouterLink class="menu-link" :to="{ name: 'legal-document', params: { documentType: 'terms' } }" @click="closeMenu">
              <Scale :size="18" />
              <span>{{ t("mobile.legalInfo") }}</span>
            </RouterLink>
          </div>
          <div class="menu-language" role="group" :aria-label="t('language.title')">
            <button
              type="button"
              :class="{ active: locale === 'zh-CN' }"
              @click="setLocale('zh-CN')"
            >
              {{ t("language.zh") }}
            </button>
            <button
              type="button"
              :class="{ active: locale === 'en-US' }"
              @click="setLocale('en-US')"
            >
              English
            </button>
          </div>
        </nav>
      </div>
    </Teleport>

    <!-- Main Content -->
    <main class="mobile-content">
      <div v-if="connectionPhase === 'pin_entry'" class="pin-entry">
        <p class="pin-entry-hint">{{ t("mobile.pinHint") }}</p>
        <div class="pin-entry-box">
          <input
            v-model="pinInput"
            inputmode="numeric"
            autocomplete="one-time-code"
            pattern="[0-9]*"
            maxlength="6"
            class="pin-entry-input"
            :placeholder="t('mobile.pinPlaceholder')"
            @keyup.enter="submitPin"
          />
          <button
            type="button"
            class="pin-entry-submit"
            :disabled="pinInput.length !== 6"
            @click="submitPin"
          >
            {{ t("mobile.pinSubmit") }}
          </button>
        </div>
      </div>
      <p v-if="connectionError" class="session-error">{{ connectionError }}</p>
      <div v-if="canRequestAccess" class="access-retry">
        <p>{{ t("mobile.accessRequired") }}</p>
        <button type="button" @click="mobileSession.requestAccess()">
          {{ t("mobile.requestAccess") }}
        </button>
      </div>
      <p v-if="receiveError" class="session-error">{{ receiveError }}</p>
      <RouterView />
    </main>

    <ReceiveRequestDialog
      :visible="showReceiveDialog"
      :transfer="pendingReceiveTransfer"
      :downloads="mobileSession.receiveDownloads"
      :receiving="mobileSession.isReceiving"
      @accept="mobileSession.acceptIncomingTransfer"
      @reject="mobileSession.rejectIncomingTransfer"
      @retry-file="mobileSession.retryDownloadFile"
    />
  </div>
</template>

<style scoped>
.mobile-layout {
  min-height: 100vh;
  background: var(--color-surface-page);
}

/* ─── Top Bar ─── */
.mobile-topbar {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: 48px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  background: color-mix(in srgb, var(--color-surface-card) 85%, transparent);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border-bottom: 1px solid var(--color-border);
  z-index: var(--z-sticky);
}

.topbar-left {
  display: flex;
  align-items: center;
  gap: 6px;
}

.logo-text {
  font-size: var(--text-base);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  letter-spacing: 0.02em;
}

.topbar-center {
  position: absolute;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  display: flex;
  align-items: center;
  gap: 5px;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: var(--radius-full);
  background: var(--color-state-warning);
}

.status-dot--online {
  background: var(--color-state-success);
}

.status-dot--error {
  background: var(--color-state-error);
}

.status-dot--pending {
  background: var(--color-state-warning);
}

.status-text {
  font-size: var(--text-xs);
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.topbar-right {
  display: flex;
  align-items: center;
  gap: 4px;
}

.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  /* audit-34: 44px hit target on touch screens; icon stays visually small. */
  width: 44px;
  height: 44px;
  margin-right: -6px;
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

.icon-btn:active {
  background: var(--color-active);
}

/* ─── Main Content ─── */
.mobile-menu-layer {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
}

.menu-backdrop {
  position: absolute;
  inset: 0;
  width: 100%;
  border: 0;
  background: rgba(0, 0, 0, 0.25);
}

.mobile-menu {
  position: absolute;
  top: calc(48px + env(safe-area-inset-top));
  right: 8px;
  display: flex;
  flex-direction: column;
  min-width: 156px;
  padding: 6px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface-card);
  box-shadow: var(--shadow-lg);
}

.menu-link {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 42px;
  padding: 0 12px;
  border-radius: var(--radius-sm);
  color: var(--color-text-primary);
  font-size: var(--text-sm);
  text-decoration: none;
}

.menu-link.router-link-exact-active {
  color: var(--color-brand-primary);
  background: var(--color-brand-primary-soft);
}

.menu-extra {
  display: flex;
  flex-direction: column;
  margin-top: 5px;
  padding-top: 6px;
  border-top: 1px solid var(--color-border);
}

.menu-language {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px;
  margin-top: 5px;
  padding-top: 6px;
  border-top: 1px solid var(--color-border);
}

.menu-language button {
  min-height: 34px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-secondary);
  font: inherit;
  font-size: var(--text-xs);
}

.menu-language button.active {
  background: var(--color-brand-primary-soft);
  color: var(--color-text-brand);
  font-weight: var(--weight-medium);
}

.mobile-content {
  max-width: 375px;
  margin: 0 auto;
  padding-top: 48px;
  min-height: 100vh;
}

.pin-entry {
  margin: 24px 12px 0;
  padding: 16px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  background: var(--color-surface-card);
}

.pin-entry-hint {
  margin: 0 0 12px;
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  line-height: 1.5;
}

.pin-entry-box {
  display: flex;
  gap: 8px;
}

.pin-entry-input {
  flex: 1;
  min-width: 0;
  min-height: 44px;
  padding: 0 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-surface-page);
  color: var(--color-text-primary);
  font: inherit;
  font-size: var(--text-lg);
  letter-spacing: 0.35em;
  text-align: center;
}

.pin-entry-input:focus {
  outline: none;
  border-color: var(--color-brand-primary);
  box-shadow: 0 0 0 2px var(--color-brand-primary-soft);
}

.pin-entry-submit {
  min-height: 44px;
  padding: 0 16px;
  border: 0;
  border-radius: var(--radius-sm);
  background: var(--color-brand-primary);
  color: #fff;
  font: inherit;
  font-size: var(--text-sm);
}

.pin-entry-submit:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.session-error {
  max-width: 375px;
  margin: 12px auto 0;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  background: var(--color-state-error-soft);
  color: var(--color-state-error);
  font-size: var(--text-sm);
  line-height: 1.5;
}

.access-retry {
  margin: 12px;
  padding: 12px;
  border-radius: var(--radius-md);
  background: var(--color-state-warning-soft);
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  line-height: 1.5;
}

.access-retry p {
  margin: 0 0 8px;
}

.access-retry button {
  min-height: 38px;
  padding: 0 14px;
  border: 0;
  border-radius: var(--radius-sm);
  background: var(--color-brand-primary);
  color: #fff;
  font: inherit;
}
</style>
