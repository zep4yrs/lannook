<script setup lang="ts">
import { computed } from "vue";
import { useRouter } from "vue-router";
import { ArrowLeft, FileText, Scale, ShieldCheck } from "lucide-vue-next";
import { legalConfig } from "@/config/legal";
import { useLocale } from "@/i18n";
import {
  getLegalDocument,
  isLegalDocumentType,
  type LegalDocumentType,
} from "@/content/legalDocuments";

interface Props {
  documentType: string;
}

const props = defineProps<Props>();
const router = useRouter();
const { t, locale } = useLocale();

const resolvedDocumentType = computed<LegalDocumentType>(() =>
  isLegalDocumentType(props.documentType) ? props.documentType : "privacy"
);

const document = computed(() =>
  getLegalDocument(resolvedDocumentType.value, legalConfig)
);

const documentIcon = computed(() => {
  if (resolvedDocumentType.value === "privacy") return ShieldCheck;
  if (resolvedDocumentType.value === "terms") return FileText;
  return Scale;
});
</script>

<template>
  <div class="legal-page">
    <button class="back-button" type="button" @click="router.back()">
      <ArrowLeft :size="16" />
      {{ t("common.back") }}
    </button>

    <header class="legal-header">
      <div class="legal-icon"><component :is="documentIcon" :size="24" /></div>
      <p class="legal-eyebrow">{{ document.eyebrow }}</p>
      <h1 class="legal-title">{{ document.title }}</h1>
      <p class="legal-summary">{{ document.summary }}</p>
    </header>

    <div class="open-source-notice" role="note">
      {{ t("legal.openSourceNotice", { license: legalConfig.license }) }}
    </div>

    <p v-if="locale !== 'zh-CN'" class="legal-lang-notice">{{ t("legal.chineseOnly") }}</p>

    <article class="legal-document">
      <section
        v-for="section in document.sections"
        :key="section.title"
        class="legal-section"
        :class="{ 'legal-section--important': section.important }"
      >
        <h2 class="legal-section-title">{{ section.title }}</h2>
        <p v-for="paragraph in section.paragraphs" :key="paragraph" class="legal-paragraph">
          {{ paragraph }}
        </p>
      </section>
    </article>

    <nav class="legal-navigation" :aria-label="t('legal.navigation')">
      <RouterLink :to="{ name: 'legal-document', params: { documentType: 'privacy' } }">{{ t("legal.privacy") }}</RouterLink>
      <RouterLink :to="{ name: 'legal-document', params: { documentType: 'terms' } }">{{ t("legal.terms") }}</RouterLink>
      <RouterLink :to="{ name: 'legal-document', params: { documentType: 'disclaimer' } }">{{ t("legal.disclaimer") }}</RouterLink>
    </nav>
  </div>
</template>

<style scoped>
.legal-page { max-width: 840px; margin: 0 auto; padding: 28px 32px 48px; }
.back-button { display: inline-flex; align-items: center; gap: 6px; border: 0; background: transparent; color: var(--color-text-secondary); cursor: pointer; padding: 6px 0; font-size: var(--text-sm); }
.back-button:hover { color: var(--color-brand-primary); }
.legal-header { margin: 20px 0 24px; }
.legal-icon { display: inline-flex; padding: 10px; color: var(--color-brand-primary); background: var(--color-brand-primary-light); border-radius: var(--radius-lg); }
.legal-eyebrow { margin: 16px 0 0; color: var(--color-brand-primary); font-size: var(--text-xs); font-weight: var(--weight-semibold); letter-spacing: .08em; }
.legal-title { margin: 8px 0 0; color: var(--color-text-primary); font-size: var(--text-2xl); }
.legal-summary { margin: 10px 0 0; color: var(--color-text-secondary); line-height: 1.7; }
.legal-lang-notice { margin: 0 0 12px; color: var(--color-text-tertiary); font-size: var(--text-xs); }
.open-source-notice { margin-bottom: 16px; padding: 14px 16px; color: var(--color-text-secondary); background: var(--color-surface-inset); border: 1px solid var(--color-border); border-radius: var(--radius-md); line-height: 1.65; }
.open-source-notice strong { color: var(--color-text-primary); }
.legal-document { padding: 4px 26px; background: var(--color-surface-card); border: 1px solid var(--color-border); border-radius: var(--radius-xl); box-shadow: var(--shadow-card); }
.legal-section { padding: 24px 0; border-bottom: 1px solid var(--color-border); }
.legal-section:last-child { border-bottom: 0; }
.legal-section--important { margin: 0 -12px; padding: 24px 12px; background: var(--color-surface-inset); }
.legal-section-title { margin: 0; color: var(--color-text-primary); font-size: var(--text-lg); }
.legal-paragraph { margin: 12px 0 0; color: var(--color-text-secondary); line-height: 1.8; }
.legal-navigation { display: flex; flex-wrap: wrap; gap: 16px; margin-top: 22px; }
.legal-navigation a { color: var(--color-brand-primary); font-size: var(--text-sm); text-decoration: none; }
.legal-navigation a:hover, .legal-navigation a.router-link-active { text-decoration: underline; }
@media (max-width: 640px) { .legal-page { padding: 20px 16px 36px; } .legal-document { padding: 4px 18px; } }
</style>
