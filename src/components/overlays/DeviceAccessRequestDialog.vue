<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Check, MonitorSmartphone, ShieldCheck, X } from "lucide-vue-next";
import { useLocale } from "@/i18n";
import type { Device } from "@/types";
import { useModalA11y } from "@/composables/useModalA11y";

const props = defineProps<{
  device: Device | null;
  pending?: boolean;
}>();

const emit = defineEmits<{
  allow: [deviceId: string, trusted: boolean, expiryHours?: number];
  reject: [deviceId: string];
}>();

const { t } = useLocale();
const trustDevice = ref(false);
const expiryHours = ref<number | undefined>(undefined);
// Shared modal a11y baseline (audit-17): focus trap + Escape rejects.
const cardElement = ref<HTMLElement | null>(null);
const rejectButton = ref<HTMLButtonElement | null>(null);
useModalA11y({
  visible: () => props.device != null,
  container: cardElement,
  onEscape: () => reject(),
  initialFocus: () => rejectButton.value,
});

watch(
  () => props.device?.id,
  () => {
    trustDevice.value = false;
    expiryHours.value = undefined;
  }
);

const platformLabel = computed(() => {
  const platform = props.device?.platform;
  const labels: Record<string, string> = {
    android: "Android",
    ios: "iPhone / iPad",
    windows: "Windows",
    macos: "macOS",
    web: "web",
  };
  const known = labels[platform ?? ""];
  return known && known !== "web" ? known : t("device.platformWeb");
});

const expiryOptions = [
  { value: undefined, labelKey: "device.untilServiceStops" },
  { value: 1, labelKey: "device.expiryHour", labelCount: 1 },
  { value: 24, labelKey: "device.expiryHour", labelCount: 24 },
  { value: 24 * 7, labelKey: "device.expiryDay", labelCount: 7 },
];

function allow() {
  if (props.device) emit("allow", props.device.id, trustDevice.value, expiryHours.value);
}

function reject() {
  if (props.device) emit("reject", props.device.id);
}
</script>

<template>
  <Teleport to="body">
    <div v-if="device" class="access-request" role="presentation">
      <div class="access-request__backdrop" />
      <section
        ref="cardElement"
        class="access-request__card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="device-access-title"
      >
        <div class="access-request__icon" aria-hidden="true">
          <MonitorSmartphone :size="25" />
        </div>
        <h2 id="device-access-title">{{ t("device.allowTitle") }}</h2>
        <p class="access-request__description">
          <!-- audit-7: the device name previously rendered twice (bold prefix
               plus the {name} placeholder inside the i18n string). -->
          {{ t("device.allowDescription", { name: device.name }) }}
        </p>

        <dl class="device-details">
          <div>
            <dt>{{ t("device.label") }}</dt>
            <dd>{{ platformLabel }}</dd>
          </div>
          <div>
            <dt>{{ t("device.ipLabel") }}</dt>
            <dd>{{ device.ip || t("device.noIp") }}</dd>
          </div>
        </dl>

        <label class="trust-option">
          <input v-model="trustDevice" :disabled="pending" type="checkbox" />
          <span>
            <span class="trust-option__title"><ShieldCheck :size="16" /> {{ t("device.trustTitle") }}</span>
            <span class="trust-option__hint">{{ t("device.trustHint") }}</span>
          </span>
        </label>

        <div v-if="!trustDevice" class="expiry-option">
          <span class="trust-option__title">{{ t("device.expiryTitle") }}</span>
          <div class="expiry-options">
            <button
              v-for="option in expiryOptions"
              :key="String(option.value)"
              type="button"
              class="expiry-chip"
              :class="{ 'expiry-chip--active': expiryHours === option.value }"
              :disabled="pending"
              @click="expiryHours = option.value"
            >
              {{ t(option.labelKey, { count: option.labelCount ?? 1 }) }}
            </button>
          </div>
          <span class="trust-option__hint">{{ t("device.expiryHint1") }}</span>
        </div>

        <p class="access-request__notice">{{ t("device.expiryHint2") }}</p>

        <div class="access-request__actions">
          <button
            ref="rejectButton"
            class="reject-button"
            type="button"
            :disabled="pending"
            @click="reject"
          >
            <X :size="16" /> {{ t("device.reject") }}
          </button>
          <button class="allow-button" type="button" :disabled="pending" @click="allow">
            <Check :size="16" /> {{ pending ? t("device.allowPending") : t("device.allow") }}
          </button>
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.access-request {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
  display: grid;
  place-items: center;
  padding: 20px;
}

.access-request__backdrop {
  position: fixed;
  inset: 0;
  background: rgb(15 23 42 / 46%);
  animation: fade-in 180ms ease forwards;
}

.access-request__card {
  position: relative;
  width: min(100%, 420px);
  padding: 28px;
  color: var(--color-text-primary);
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-xl);
  animation: scale-in 200ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
}

.access-request__icon {
  display: grid;
  width: 48px;
  height: 48px;
  margin-bottom: 16px;
  color: var(--color-brand-primary);
  background: var(--color-brand-primary-soft);
  border-radius: var(--radius-full);
  place-items: center;
}

h2 {
  margin: 0;
  font-size: var(--text-xl);
  font-weight: var(--weight-semibold);
}

.access-request__description {
  margin: 8px 0 20px;
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  line-height: var(--leading-relaxed);
}

.device-details {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  padding: 14px;
  margin: 0 0 16px;
  background: var(--color-surface-inset);
  border-radius: var(--radius-md);
}

.device-details div {
  min-width: 0;
}

.device-details dt {
  margin-bottom: 4px;
  color: var(--color-text-tertiary);
  font-size: var(--text-xs);
}

.device-details dd {
  overflow: hidden;
  margin: 0;
  color: var(--color-text-primary);
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.trust-option {
  display: flex;
  gap: 10px;
  padding: 12px;
  cursor: pointer;
  background: var(--color-brand-primary-soft);
  border: 1px solid color-mix(in srgb, var(--color-brand-primary) 22%, transparent);
  border-radius: var(--radius-md);
}

.trust-option:has(input:disabled) {
  cursor: wait;
  opacity: 0.65;
}

.trust-option input {
  width: 16px;
  height: 16px;
  margin: 2px 0 0;
  accent-color: var(--color-brand-primary);
}

.trust-option__title {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--color-text-primary);
  font-size: var(--text-sm);
  font-weight: var(--weight-semibold);
}

.trust-option__hint,
.access-request__notice {
  display: block;
  color: var(--color-text-secondary);
  font-size: var(--text-xs);
  line-height: 1.5;
}

.expiry-option {
  margin-top: 12px;
  padding: 12px;
  background: var(--color-surface-inset);
  border-radius: var(--radius-md);
}

.expiry-options {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 8px 0 6px;
}

.expiry-chip {
  padding: 5px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-full);
  background: var(--color-surface-card);
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

.expiry-chip:disabled {
  cursor: wait;
  opacity: 0.6;
}

.trust-option__hint {
  margin-top: 3px;
}

.access-request__notice {
  margin: 12px 0 20px;
}

.access-request__actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.allow-button,
.reject-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  min-height: 38px;
  padding: 0 14px;
  border-radius: var(--radius-md);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
  cursor: pointer;
}

.allow-button {
  color: var(--color-text-inverse);
  background: var(--color-brand-primary);
  border: 1px solid var(--color-brand-primary);
}

.reject-button {
  color: var(--color-text-secondary);
  background: var(--color-surface-card);
  border: 1px solid var(--color-border);
}

.allow-button:disabled,
.reject-button:disabled {
  cursor: wait;
  opacity: 0.6;
}

@keyframes fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

@keyframes scale-in {
  from { opacity: 0; transform: scale(0.96); }
  to { opacity: 1; transform: scale(1); }
}
</style>
