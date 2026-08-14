<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { AlertTriangle, Check, FileText, Scale, ShieldCheck } from "lucide-vue-next";
import { legalConfig } from "@/config/legal";
import {
  getLegalDocument,
  type LegalDocumentType,
} from "@/content/legalDocuments";
import type { LegalConsentStatus } from "@/composables/useLegalConsent";
import { APP_NAME } from "@/config/brand";
import { useLocale } from "@/i18n";

interface Props {
  status: LegalConsentStatus;
}

interface Emits {
  accept: [];
  decline: [];
  reconsider: [];
}

const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const { t } = useLocale();

const activeDocumentType = shallowRef<LegalDocumentType>("terms");
const hasReviewed = shallowRef(false);

const document = computed(() => getLegalDocument(activeDocumentType.value, legalConfig));
const isDeclined = computed(() => props.status === "declined");

const documentTabs = [
  { type: "terms", labelKey: "legal.terms", icon: FileText },
  { type: "privacy", labelKey: "legal.privacy", icon: ShieldCheck },
  { type: "disclaimer", labelKey: "legal.disclaimer", icon: Scale },
] as const;
</script>

<template>
  <div v-if="status !== 'accepted'" class="consent-overlay">
    <section class="consent-dialog" role="dialog" aria-modal="true" aria-labelledby="consent-title">
      <template v-if="!isDeclined">
        <header class="consent-header">
          <div class="consent-icon"><ShieldCheck :size="24" /></div>
          <div>
            <p class="consent-eyebrow">{{ t("legal.consentEyebrow") }}</p>
            <h1 id="consent-title" class="consent-title">{{ t("legal.consentTitle") }}</h1>
          </div>
        </header>

        <p class="consent-intro">
          {{ t("legal.consentDescription", { name: APP_NAME }) }}
        </p>

        <div class="document-tabs" role="tablist" :aria-label="t('legal.tabsLabel')">
          <button
            v-for="tab in documentTabs"
            :key="tab.type"
            class="document-tab"
            :class="{ 'document-tab--active': activeDocumentType === tab.type }"
            type="button"
            role="tab"
            :aria-selected="activeDocumentType === tab.type"
            @click="activeDocumentType = tab.type"
          >
            <component :is="tab.icon" :size="15" />
            {{ t(tab.labelKey) }}
          </button>
        </div>

        <article class="document-preview" tabindex="0">
          <p class="document-eyebrow">{{ document.eyebrow }}</p>
          <h2 class="document-title">{{ document.title }}</h2>
          <p class="document-summary">{{ document.summary }}</p>
          <section
            v-for="section in document.sections"
            :key="section.title"
            class="document-section"
            :class="{ 'document-section--important': section.important }"
          >
            <h3 class="document-section-title">{{ section.title }}</h3>
            <p v-for="paragraph in section.paragraphs" :key="paragraph" class="document-paragraph">
              {{ paragraph }}
            </p>
          </section>
        </article>

        <label class="review-check">
          <input v-model="hasReviewed" type="checkbox" />
          <span>{{ t("legal.acceptStatement") }}</span>
        </label>

        <footer class="consent-actions">
          <button class="decline-button" type="button" @click="emit('decline')">{{ t("legal.decline") }}</button>
          <button class="accept-button" type="button" :disabled="!hasReviewed" @click="emit('accept')">
            <Check :size="16" />
            {{ t("legal.acceptAndEnter", { name: APP_NAME }) }}
          </button>
        </footer>
      </template>

      <template v-else>
        <div class="declined-icon"><AlertTriangle :size="26" /></div>
        <h1 id="consent-title" class="consent-title">{{ t("legal.deniedTitle") }}</h1>
        <p class="declined-message">
          {{ t("legal.deniedDescription") }}
        </p>
        <button class="accept-button" type="button" @click="emit('reconsider')">{{ t("legal.reread") }}</button>
      </template>
    </section>
  </div>
</template>

<style scoped>
.consent-overlay { position: fixed; inset: 0; z-index: 3000; display: grid; place-items: center; padding: 24px; background: rgba(16, 24, 40, 0.56); backdrop-filter: blur(3px); }
.consent-dialog { width: min(760px, 100%); max-height: min(860px, calc(100vh - 48px)); display: flex; flex-direction: column; padding: 28px; overflow: hidden; color: var(--color-text-primary); background: var(--color-surface-card); border: 1px solid var(--color-border); border-radius: var(--radius-xl); box-shadow: var(--shadow-float, 0 18px 60px rgba(0, 0, 0, 0.24)); }
.consent-header { display: flex; align-items: center; gap: 12px; }
.consent-icon, .declined-icon { display: inline-flex; align-items: center; justify-content: center; width: 44px; height: 44px; color: var(--color-brand-primary); background: var(--color-brand-primary-light); border-radius: var(--radius-lg); }
.declined-icon { margin-bottom: 16px; color: var(--color-warning-text, #8a5b00); background: var(--color-warning-bg, #fff7df); }
.consent-eyebrow, .document-eyebrow { margin: 0; color: var(--color-brand-primary); font-size: var(--text-xs); font-weight: var(--weight-semibold); letter-spacing: .08em; }
.consent-title { margin: 3px 0 0; color: var(--color-text-primary); font-size: var(--text-xl); }
.consent-intro, .declined-message { margin: 18px 0 0; color: var(--color-text-secondary); line-height: 1.7; }
.document-tabs { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 20px; }
.document-tab { display: inline-flex; align-items: center; gap: 6px; padding: 8px 12px; color: var(--color-text-secondary); background: transparent; border: 1px solid var(--color-border); border-radius: var(--radius-md); cursor: pointer; font-size: var(--text-sm); }
.document-tab:hover, .document-tab--active { color: var(--color-brand-primary); background: var(--color-selected); border-color: var(--color-brand-primary); }
.document-preview { min-height: 0; margin-top: 14px; padding: 18px; overflow-y: auto; background: var(--color-surface-inset); border: 1px solid var(--color-border); border-radius: var(--radius-lg); outline: none; }
.document-preview:focus-visible { box-shadow: 0 0 0 3px var(--color-brand-primary-light); }
.document-title { margin: 7px 0 0; font-size: var(--text-lg); }
.document-summary { margin: 8px 0 0; color: var(--color-text-secondary); line-height: 1.65; }
.document-section { margin-top: 18px; }
.document-section--important { margin: 18px -8px 0; padding: 12px 8px; background: var(--color-surface-card); border-radius: var(--radius-md); }
.document-section-title { margin: 0; font-size: var(--text-sm); }
.document-paragraph { margin: 8px 0 0; color: var(--color-text-secondary); font-size: var(--text-sm); line-height: 1.65; }
.review-check { display: flex; align-items: flex-start; gap: 9px; margin-top: 16px; color: var(--color-text-secondary); font-size: var(--text-sm); line-height: 1.55; cursor: pointer; }
.review-check input { width: 16px; height: 16px; margin-top: 2px; accent-color: var(--color-brand-primary); }
.consent-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 20px; }
.decline-button, .accept-button { display: inline-flex; align-items: center; justify-content: center; gap: 7px; padding: 10px 15px; border-radius: var(--radius-md); font-size: var(--text-sm); font-weight: var(--weight-medium); cursor: pointer; }
.decline-button { color: var(--color-text-secondary); background: transparent; border: 1px solid var(--color-border); }
.decline-button:hover { background: var(--color-hover); color: var(--color-text-primary); }
.accept-button { color: #fff; background: var(--color-brand-primary); border: 1px solid var(--color-brand-primary); }
.accept-button:hover:not(:disabled) { filter: brightness(0.96); }
.accept-button:disabled { opacity: .45; cursor: not-allowed; }
@media (max-width: 640px) { .consent-overlay { padding: 12px; } .consent-dialog { max-height: calc(100vh - 24px); padding: 20px; } .consent-actions { flex-direction: column-reverse; } .decline-button, .accept-button { width: 100%; } }
</style>
