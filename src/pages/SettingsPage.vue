<script setup lang="ts">
import { computed, ref } from "vue";
import { Sun, Moon, MonitorSmartphone, FolderOpen, ShieldCheck, FileText, Gauge, Info } from "lucide-vue-next";
import { useSettingsStore } from "@/stores/settings";
import { useAppStore } from "@/stores/app";
import { pickDirectory } from "@/services/tauri";
import type { ThemeMode } from "@/types";
import LanguageSelector from "@/components/settings/LanguageSelector.vue";
import { useLocale, type Locale } from "@/i18n";
import type { CloseBehavior } from "@/services/tauri";
import { APP_NAME } from "@/config/brand";

const settingsStore = useSettingsStore();
const appStore = useAppStore();
const { locale, setLocale, t } = useLocale();

// audit-21: tell the user when their raw input was clamped into range.
const speedClamped = ref(false);
const maxFileSizeInput = ref<string>(String(Math.round(settingsStore.maxFileSize / (1024 * 1024))));

const themeOptions = computed<{ value: ThemeMode; label: string; icon: typeof Sun }[]>(() => [
  { value: "light", label: t("theme.light"), icon: Sun },
  { value: "dark", label: t("theme.dark"), icon: Moon },
  { value: "system", label: t("theme.system"), icon: MonitorSmartphone },
]);

function selectTheme(mode: ThemeMode) {
  settingsStore.setThemeMode(mode);
}

function selectLocale(value: Locale) {
  setLocale(value);
}

function toggleApproval() {
  settingsStore.setRequireApproval(!settingsStore.requireApproval);
}

function setAuthorizationExpiryHours(hours: number) {
  settingsStore.setAuthorizationExpiryHours(hours);
}

function onSpeedLimitChange(event: Event) {
  const raw = Number((event.target as HTMLInputElement).value);
  const value = Number.isFinite(raw) ? raw : 0;
  const clamped = Math.max(0, Math.min(1024, Math.round(value)));
  speedClamped.value = clamped !== value || (event.target as HTMLInputElement).value === "";
  settingsStore.setDownloadSpeedLimitMbps(clamped);
  // Reflect the effective value back into the field.
  (event.target as HTMLInputElement).value = String(clamped);
}

/** audit-20: surface the previously hidden maxFileSize setting (MB, 0 = unlimited). */
const MAX_FILE_BYTES_PER_MB = 1024 * 1024;

function onMaxFileSizeChange(event: Event) {
  const input = event.target as HTMLInputElement;
  let mb = Math.round(Number(input.value));
  if (!Number.isFinite(mb) || mb < 0) mb = 0;
  if (mb > 102400) mb = 102400; // 100 GB sanity ceiling
  input.value = String(mb);
  maxFileSizeInput.value = String(mb);
  settingsStore.setMaxFileSize(mb * MAX_FILE_BYTES_PER_MB);
}

const authorizationOptions = [
  { value: 0, labelKey: "settings.authorizationSession" },
  { value: 1, labelKey: "settings.authorizationHour" },
  { value: 24, labelKey: "settings.authorizationDay" },
  { value: 24 * 7, labelKey: "settings.authorizationWeek" },
] as const;

const closeOptions = computed<{ value: CloseBehavior; label: string }[]>(() => [
  { value: "minimize", label: t("settings.closeMinimize") },
  { value: "quit", label: t("settings.closeQuit") },
  { value: "ask", label: t("settings.closeAsk") },
]);

function toggleAutostart() {
  void settingsStore.setAutostart(!settingsStore.autostartEnabled);
}

async function changeFolder() {
  const directory = await pickDirectory();
  if (directory) {
    settingsStore.setReceiveFolder(directory);
  }
}
</script>

<template>
  <div class="settings-page">
    <header class="page-header">
      <h1 class="page-title">{{ t("settings.title") }}</h1>
    </header>

    <div class="settings-card">
      <LanguageSelector :model-value="locale" @update-model-value="selectLocale" />

      <hr class="settings-divider" />

      <section class="settings-section">
        <div class="section-header">
          <MonitorSmartphone :size="16" class="section-icon" />
          <h2 class="section-title">{{ t("settings.lifecycle") }}</h2>
        </div>
        <div class="toggle-row">
          <div class="toggle-info">
            <span class="toggle-label">{{ t("settings.autostart") }}</span>
            <span class="toggle-desc">{{ t("settings.autostartDescription") }}</span>
          </div>
          <button
            class="toggle-switch"
            :class="{ 'toggle-switch--on': settingsStore.autostartEnabled }"
            role="switch"
            :aria-checked="settingsStore.autostartEnabled"
            @click="toggleAutostart"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
        <div class="close-behavior-row">
          <span class="toggle-label">{{ t("settings.closeBehavior") }}</span>
          <label v-for="option in closeOptions" :key="option.value" class="close-option">
            <input
              type="radio"
              name="close-behavior"
              :value="option.value"
              :checked="settingsStore.closeBehavior === option.value"
              @change="settingsStore.setCloseBehavior(option.value)"
            />
            <span>{{ option.label }}</span>
          </label>
        </div>
      </section>

      <hr class="settings-divider" />

      <!-- Theme Section -->
      <section class="settings-section">
        <div class="section-header">
          <Sun :size="16" class="section-icon" />
          <h2 class="section-title">{{ t("theme.title") }}</h2>
        </div>
        <div class="theme-options">
          <label
            v-for="opt in themeOptions"
            :key="opt.value"
            class="theme-option"
            :class="{ 'theme-option--active': settingsStore.themeMode === opt.value }"
          >
            <input
              type="radio"
              name="theme"
              :value="opt.value"
              :checked="settingsStore.themeMode === opt.value"
              class="theme-radio"
              @change="selectTheme(opt.value)"
            />
            <component :is="opt.icon" :size="18" class="theme-option-icon" />
            <span class="theme-option-label">{{ opt.label }}</span>
          </label>
        </div>
      </section>

      <hr class="settings-divider" />

      <!-- Receive Folder Section -->
      <section class="settings-section">
        <div class="section-header">
          <FolderOpen :size="16" class="section-icon" />
          <h2 class="section-title">{{ t("settings.receiveFolder") }}</h2>
        </div>
        <div class="folder-row">
          <code class="folder-path">{{ settingsStore.receiveFolder }}</code>
          <button class="change-btn" @click="changeFolder">{{ t("settings.change") }}</button>
        </div>
      </section>

      <hr class="settings-divider" />

      <!-- Security Section -->
      <section class="settings-section">
        <div class="section-header">
          <ShieldCheck :size="16" class="section-icon" />
          <h2 class="section-title">{{ t("settings.security") }}</h2>
        </div>
        <div class="toggle-row">
          <div class="toggle-info">
            <span class="toggle-label">{{ t("settings.requireApproval") }}</span>
            <span class="toggle-desc">{{ t("settings.requireApprovalDescription") }}</span>
          </div>
          <button
            class="toggle-switch"
            :class="{ 'toggle-switch--on': settingsStore.requireApproval }"
            role="switch"
            :aria-checked="settingsStore.requireApproval"
            @click="toggleApproval"
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
        <div class="toggle-row">
          <div class="toggle-info">
            <span class="toggle-label">{{ t("settings.authorizationExpiry") }}</span>
            <span class="toggle-desc">{{ t("settings.authorizationExpiryDescription") }}</span>
          </div>
          <div class="expiry-picker">
            <button
              v-for="option in authorizationOptions"
              :key="String(option.value)"
              class="expiry-chip"
              :class="{ 'expiry-chip--active': settingsStore.authorizationExpiryHours === option.value }"
              @click="setAuthorizationExpiryHours(option.value)"
            >
              {{ t(option.labelKey) }}
            </button>
          </div>
        </div>
      </section>

      <hr class="settings-divider" />

      <!-- Transfer Section -->
      <section class="settings-section">
        <div class="section-header">
          <Gauge :size="16" class="section-icon" />
          <h2 class="section-title">{{ t("settings.transfer") }}</h2>
        </div>
        <div class="toggle-row">
          <div class="toggle-info">
            <span class="toggle-label">{{ t("settings.downloadSpeedLimit") }}</span>
            <span class="toggle-desc">{{ t("settings.downloadSpeedLimitDescription") }}</span>
          </div>
          <div class="speed-limit-picker">
            <input
              type="number"
              min="0"
              max="1024"
              step="1"
              class="speed-limit-input"
              :value="settingsStore.downloadSpeedLimitMbps"
              @change="onSpeedLimitChange"
            />
            <!-- audit-9: label now matches the stored Mbps semantics -->
            <span class="speed-limit-unit">Mbps</span>
          </div>
        </div>
        <p v-if="speedClamped" class="clamp-hint">{{ t("settings.valueClamped") }}</p>
        <div class="toggle-row">
          <div class="toggle-info">
            <span class="toggle-label">{{ t("settings.maxFileSize") }}</span>
            <span class="toggle-desc">{{ t("settings.maxFileSizeDescription") }}</span>
          </div>
          <div class="speed-limit-picker">
            <input
              type="number"
              min="0"
              step="1"
              class="speed-limit-input"
              :value="maxFileSizeInput"
              @change="onMaxFileSizeChange"
            />
            <span class="speed-limit-unit">MB</span>
          </div>
        </div>
      </section>

      <hr class="settings-divider" />

      <!-- Open-source license and legal documents -->
      <section class="settings-section">
        <div class="section-header">
          <FileText :size="16" class="section-icon" />
          <h2 class="section-title">{{ t("settings.legal") }}</h2>
        </div>
        <p class="legal-description">
          {{ t("settings.legalDescription") }}
        </p>
        <nav class="legal-links" :aria-label="t('settings.legal')">
          <RouterLink to="/legal/terms">{{ t("settings.terms") }}</RouterLink>
          <RouterLink to="/legal/privacy">{{ t("settings.privacy") }}</RouterLink>
          <RouterLink to="/legal/disclaimer">{{ t("settings.disclaimer") }}</RouterLink>
        </nav>
      </section>

      <hr class="settings-divider" />

      <!-- About Section -->
      <section class="settings-section">
        <div class="section-header">
          <Info :size="16" class="section-icon" />
          <h2 class="section-title">{{ t("settings.about") }}</h2>
        </div>
        <div class="about-grid">
          <div class="about-item">
            <span class="about-label">{{ t("settings.appName") }}</span>
            <span class="about-value">{{ APP_NAME }}</span>
          </div>
          <div class="about-item">
            <span class="about-label">{{ t("settings.version") }}</span>
            <span class="about-value">{{ appStore.appVersion }}</span>
          </div>
          <div class="about-item">
            <span class="about-label">{{ t("settings.deviceName") }}</span>
            <span class="about-value">{{ appStore.deviceName }}</span>
          </div>
          <div class="about-item">
            <span class="about-label">{{ t("settings.network") }}</span>
            <span class="about-value">{{ appStore.networkName }}</span>
          </div>
          <div class="about-item">
            <span class="about-label">{{ t("settings.localIp") }}</span>
            <span class="about-value">{{ appStore.localIp }}</span>
          </div>
          <div class="about-item">
            <span class="about-label">{{ t("settings.connectionCode") }}</span>
            <span class="about-value about-value--mono">{{ appStore.connectionToken }}</span>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.settings-page {
  padding: 32px;
  max-width: 720px;
  margin: 0 auto;
}

.page-header {
  margin-bottom: 24px;
}

.page-title {
  font-size: var(--text-2xl);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  margin: 0;
  line-height: var(--leading-tight);
}

.settings-card {
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
  padding: 24px;
}

.settings-section {
  padding: 4px 0;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 16px;
}

.section-icon {
  color: var(--color-brand-primary);
}

.section-title {
  font-size: var(--text-base);
  font-weight: var(--weight-semibold);
  color: var(--color-text-primary);
  margin: 0;
}

.settings-divider {
  border: none;
  border-top: 1px solid var(--color-border);
  margin: 20px 0;
}

.clamp-hint {
  margin: 6px 0 0;
  color: var(--color-state-warning);
  font-size: var(--text-xs);
}

/* Theme Options */
.theme-options {
  display: flex;
  gap: 12px;
}

.theme-option {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 16px 24px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: all var(--transition-fast);
  flex: 1;
}

.theme-option:hover {
  border-color: var(--color-border-strong);
  background: var(--color-hover);
}

.theme-option--active {
  border-color: var(--color-brand-primary);
  background: var(--color-selected);
}

.theme-radio {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

.theme-option-icon {
  color: var(--color-text-secondary);
}

.theme-option--active .theme-option-icon {
  color: var(--color-brand-primary);
}

.theme-option-label {
  font-size: var(--text-sm);
  color: var(--color-text-primary);
  font-weight: var(--weight-medium);
}

/* Folder Row */
.folder-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.folder-path {
  flex: 1;
  padding: 8px 12px;
  background: var(--color-surface-inset);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-family: var(--font-mono);
  color: var(--color-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.change-btn {
  padding: 8px 14px;
  border: 1px solid var(--color-border-strong);
  border-radius: var(--radius-md);
  background: var(--color-surface-card);
  color: var(--color-text-primary);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: all var(--transition-fast);
  white-space: nowrap;
}

.change-btn:hover {
  background: var(--color-hover);
  border-color: var(--color-brand-primary);
  color: var(--color-text-brand);
}

/* Toggle */
.toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.toggle-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.toggle-label {
  font-size: var(--text-base);
  color: var(--color-text-primary);
  font-weight: var(--weight-medium);
}

.toggle-desc {
  font-size: var(--text-sm);
  color: var(--color-text-secondary);
}

.expiry-picker {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
  max-width: 320px;
}

.expiry-chip {
  padding: 5px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-full);
  background: transparent;
  color: var(--color-text-secondary);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.expiry-chip:hover {
  border-color: var(--color-brand-primary);
  color: var(--color-text-brand);
}

.expiry-chip--active {
  border-color: var(--color-brand-primary);
  background: var(--color-brand-primary-soft);
  color: var(--color-text-brand);
}

.toggle-switch {
  position: relative;
  width: 44px;
  height: 24px;
  border: none;
  border-radius: var(--radius-full);
  background: var(--color-border-strong);
  cursor: pointer;
  transition: background var(--transition-normal);
  flex-shrink: 0;
}

.toggle-switch--on {
  background: var(--color-brand-primary);
}

.toggle-knob {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 18px;
  height: 18px;
  border-radius: var(--radius-full);
  background: white;
  box-shadow: var(--shadow-sm);
  transition: transform var(--transition-normal);
}

.speed-limit-picker {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 0 0 auto;
}

.speed-limit-input {
  width: 84px;
  min-height: 36px;
  padding: 0 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-surface-page);
  color: var(--color-text-primary);
  font: inherit;
  text-align: right;
}

.speed-limit-input:focus {
  outline: none;
  border-color: var(--color-brand-primary);
}

.speed-limit-unit {
  color: var(--color-text-tertiary);
  font-size: var(--text-xs);
  white-space: nowrap;
}

.toggle-switch--on .toggle-knob {
  transform: translateX(20px);
}

.close-behavior-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px 18px;
  margin-top: 18px;
}

.close-option {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  cursor: pointer;
}

.legal-description {
  margin: 0;
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  line-height: 1.7;
}

.legal-links {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  margin-top: 12px;
}

.legal-links a {
  color: var(--color-brand-primary);
  font-size: var(--text-sm);
  text-decoration: none;
}

.legal-links a:hover {
  text-decoration: underline;
}

/* About Grid */
.about-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.about-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.about-label {
  font-size: var(--text-xs);
  color: var(--color-text-tertiary);
}

.about-value {
  font-size: var(--text-sm);
  color: var(--color-text-primary);
  font-weight: var(--weight-medium);
}

.about-value--mono {
  font-family: var(--font-mono);
}
</style>
