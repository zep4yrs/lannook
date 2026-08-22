<script setup lang="ts">
import { computed } from "vue";
import { useRouter, RouterLink } from "vue-router";
import { ArrowLeft, Download, Send, ShieldCheck, Wifi } from "lucide-vue-next";
import { useLocale } from "@/i18n";

// audit-33: phones only ever saw Send/Transfers; a lightweight FAQ plus the
// legal entries give the mobile web app its first help surface.
const router = useRouter();
const { t } = useLocale();

const faqs = computed(() => [
  { q: t("mobileHelp.faq1Q"), a: t("mobileHelp.faq1A") },
  { q: t("mobileHelp.faq2Q"), a: t("mobileHelp.faq2A") },
  { q: t("mobileHelp.faq3Q"), a: t("mobileHelp.faq3A") },
  { q: t("mobileHelp.faq4Q"), a: t("mobileHelp.faq4A") },
]);
</script>

<template>
  <div class="mobile-help">
    <button class="back-btn" type="button" @click="router.back()">
      <ArrowLeft :size="15" /> {{ t("common.back") }}
    </button>

    <header class="help-header">
      <h1>{{ t("mobileHelp.title") }}</h1>
      <p>{{ t("mobileHelp.subtitle") }}</p>
    </header>

    <section class="help-card">
      <div class="section-heading"><Wifi :size="17" /><h2>{{ t("mobileHelp.connectTitle") }}</h2></div>
      <ol>
        <li>{{ t("mobileHelp.step1") }}</li>
        <li>{{ t("mobileHelp.step2") }}</li>
        <li>{{ t("mobileHelp.step3") }}</li>
      </ol>
    </section>

    <section class="help-card">
      <div class="section-heading"><Download :size="17" /><h2>{{ t("mobileHelp.receiveTitle") }}</h2></div>
      <p>{{ t("mobileHelp.receiveDesc") }}</p>
    </section>

    <section class="help-card">
      <div class="section-heading"><Send :size="17" /><h2>{{ t("mobileHelp.sendTitle") }}</h2></div>
      <p>{{ t("mobileHelp.sendDesc") }}</p>
    </section>

    <section class="help-card">
      <div class="section-heading"><ShieldCheck :size="17" /><h2>{{ t("mobileHelp.privacyTitle") }}</h2></div>
      <p>{{ t("mobileHelp.privacyDesc") }}</p>
      <nav class="legal-links" :aria-label="t('settings.legal')">
        <RouterLink :to="{ name: 'legal-document', params: { documentType: 'terms' } }">{{ t("settings.terms") }}</RouterLink>
        <RouterLink :to="{ name: 'legal-document', params: { documentType: 'privacy' } }">{{ t("settings.privacy") }}</RouterLink>
        <RouterLink :to="{ name: 'legal-document', params: { documentType: 'disclaimer' } }">{{ t("settings.disclaimer") }}</RouterLink>
      </nav>
    </section>

    <section class="help-card">
      <div class="section-heading"><Send :size="17" /><h2>{{ t("mobileHelp.faqTitle") }}</h2></div>
      <details v-for="faq in faqs" :key="faq.q" class="faq-item">
        <summary>{{ faq.q }}</summary>
        <p>{{ faq.a }}</p>
      </details>
    </section>
  </div>
</template>

<style scoped>
.mobile-help { padding: 20px 16px 40px; }
.back-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-height: 44px;
  border: 0;
  background: transparent;
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  cursor: pointer;
  padding: 0 4px;
}
.help-header { margin: 10px 0 20px; }
.help-header h1 { margin: 0; font-size: var(--text-xl); color: var(--color-text-primary); }
.help-header p { margin: 8px 0 0; color: var(--color-text-secondary); font-size: var(--text-sm); line-height: 1.6; }
.help-card {
  padding: 16px;
  margin-bottom: 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-surface-card);
  box-shadow: var(--shadow-card);
  color: var(--color-text-secondary);
  font-size: var(--text-sm);
  line-height: 1.7;
}
.section-heading { display: flex; align-items: center; gap: 7px; margin-bottom: 8px; }
.section-heading svg { color: var(--color-brand-primary); }
.section-heading h2 { margin: 0; font-size: var(--text-base); color: var(--color-text-primary); }
.help-card ol, .help-card ul { margin: 0; padding-left: 20px; }
.help-card p { margin: 0; }
.legal-links { display: flex; flex-wrap: wrap; gap: 14px; margin-top: 10px; }
.legal-links a { color: var(--color-text-brand); text-decoration: none; }
.legal-links a:hover { text-decoration: underline; }
.faq-item { border-top: 1px solid var(--color-border); padding: 10px 0; }
.faq-item summary { cursor: pointer; color: var(--color-text-primary); font-weight: var(--weight-medium); min-height: 32px; display: flex; align-items: center; }
.faq-item p { margin-top: 8px; }
</style>
