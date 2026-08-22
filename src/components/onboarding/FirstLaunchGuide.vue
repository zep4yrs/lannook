<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, shallowRef } from "vue";
import { Check, Monitor, QrCode, Send, X } from "lucide-vue-next";
import { useLocale } from "@/i18n";
import { readAndMigrateLocalStorageValue } from "@/utils/storage";
import { useModalA11y } from "@/composables/useModalA11y";

const STORAGE_KEY = "lannook.first-launch-guide.v1";
const LEGACY_STORAGE_KEYS = ["lynqo.first-launch-guide.v1"] as const;
/** Window event used by the Help page to re-open this guide (audit-24). */
const REOPEN_GUIDE_EVENT = "lannook:reopen-guide";
const { locale, setLocale, t } = useLocale();
const visible = shallowRef(readDismissed() === false);
const dialogElement = ref<HTMLElement | null>(null);

useModalA11y({
  visible: () => visible.value,
  container: dialogElement,
  onEscape: () => dismiss(),
});

function readDismissed(): boolean {
  try {
    return readAndMigrateLocalStorageValue(STORAGE_KEY, LEGACY_STORAGE_KEYS) === "done";
  } catch {
    return false;
  }
}

function dismiss() {
  try {
    window.localStorage.setItem(STORAGE_KEY, "done");
  } catch {
    // The guide can still be dismissed for the current session.
  }
  visible.value = false;
}

function handleReopen() {
  visible.value = true;
}

onMounted(() => window.addEventListener(REOPEN_GUIDE_EVENT, handleReopen));
onUnmounted(() => window.removeEventListener(REOPEN_GUIDE_EVENT, handleReopen));

function toggleLocale() {
  setLocale(locale.value === "zh-CN" ? "en-US" : "zh-CN");
}

const steps = computed(() => [
  { number: "1", icon: Monitor, title: t("onboarding.step1Title"), text: t("onboarding.step1Text") },
  { number: "2", icon: QrCode, title: t("onboarding.step2Title"), text: t("onboarding.step2Text") },
  { number: "3", icon: Check, title: t("onboarding.step3Title"), text: t("onboarding.step3Text") },
  { number: "4", icon: Send, title: t("onboarding.step4Title"), text: t("onboarding.step4Text") },
]);
</script>

<template>
  <div v-if="visible" class="guide-overlay">
    <section ref="dialogElement" class="guide-dialog" role="dialog" aria-modal="true" aria-labelledby="guide-title">
      <button class="guide-close" type="button" :aria-label="t('onboarding.close')" @click="dismiss">
        <X :size="18" />
      </button>
      <button class="guide-language" type="button" @click="toggleLocale">
        {{ locale === "zh-CN" ? t("language.en") : t("language.zh") }}
      </button>
      <p class="guide-eyebrow">{{ t("onboarding.eyebrow") }}</p>
      <h1 id="guide-title" class="guide-title">{{ t("onboarding.title") }}</h1>
      <p class="guide-intro">{{ t("onboarding.intro") }}</p>
      <div class="guide-steps">
        <article v-for="step in steps" :key="step.number" class="guide-step">
          <div class="step-icon"><component :is="step.icon" :size="19" /></div>
          <div class="step-content">
            <span class="step-number">{{ step.number }}</span>
            <div>
              <h2>{{ step.title }}</h2>
              <p>{{ step.text }}</p>
            </div>
          </div>
        </article>
      </div>
      <button class="guide-primary" type="button" @click="dismiss">{{ t("onboarding.start") }}</button>
      <button class="guide-secondary" type="button" @click="dismiss">{{ t("onboarding.skip") }}</button>
    </section>
  </div>
</template>

<style scoped>
.guide-overlay { position: fixed; inset: 0; z-index: 2900; display: grid; place-items: center; padding: 24px; background: rgba(16, 24, 40, .5); backdrop-filter: blur(3px); }
.guide-dialog { position: relative; width: min(600px, 100%); padding: 32px; color: var(--color-text-primary); background: var(--color-surface-card); border: 1px solid var(--color-border); border-radius: var(--radius-xl); box-shadow: var(--shadow-float); }
.guide-close { position: absolute; top: 16px; right: 16px; display: grid; place-items: center; width: 32px; height: 32px; border: 0; border-radius: 50%; color: var(--color-text-secondary); background: var(--color-surface-inset); cursor: pointer; }
.guide-language { position: absolute; top: 16px; right: 56px; padding: 7px 9px; color: var(--color-text-brand); background: transparent; border: 1px solid var(--color-border); border-radius: var(--radius-sm); font-size: var(--text-xs); cursor: pointer; }
.guide-eyebrow { margin: 0 0 6px; color: var(--color-brand-primary); font-size: var(--text-xs); font-weight: var(--weight-semibold); letter-spacing: .08em; text-transform: uppercase; }
.guide-title { margin: 0; font-size: var(--text-2xl); }
.guide-intro { margin: 10px 0 24px; color: var(--color-text-secondary); line-height: 1.65; }
.guide-steps { display: grid; gap: 12px; }
.guide-step { display: flex; gap: 12px; padding: 12px; border: 1px solid var(--color-border); border-radius: var(--radius-md); background: var(--color-surface-inset); }
.step-icon { display: grid; place-items: center; flex: 0 0 36px; height: 36px; color: var(--color-brand-primary); background: var(--color-brand-primary-light); border-radius: var(--radius-md); }
.step-content { display: flex; gap: 9px; min-width: 0; }
.step-number { color: var(--color-brand-primary); font-weight: var(--weight-semibold); }
.step-content h2 { margin: 0; font-size: var(--text-sm); }
.step-content p { margin: 4px 0 0; color: var(--color-text-secondary); font-size: var(--text-sm); line-height: 1.5; }
.guide-primary, .guide-secondary { width: 100%; margin-top: 20px; padding: 10px 14px; border-radius: var(--radius-md); font-size: var(--text-sm); cursor: pointer; }
.guide-primary { color: #fff; background: var(--color-brand-primary); border: 1px solid var(--color-brand-primary); }
.guide-secondary { margin-top: 8px; color: var(--color-text-secondary); background: transparent; border: 0; }
@media (max-width: 640px) { .guide-overlay { padding: 12px; } .guide-dialog { padding: 24px 18px; } }
</style>
