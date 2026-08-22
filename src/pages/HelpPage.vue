<script setup lang="ts">
import { useRouter } from "vue-router";
import { ArrowLeft, Download, Send, ShieldCheck, Wifi, Wrench, RotateCcw } from "lucide-vue-next";
import { useLocale } from "@/i18n";

const router = useRouter();
const { t } = useLocale();

// audit-24: the first-launch guide was unreachable after dismissal.
function reopenGuide() {
  window.dispatchEvent(new CustomEvent("lannook:reopen-guide"));
}
</script>

<template>
  <div class="help-page">
    <button class="back-btn" @click="router.back()">
      <ArrowLeft :size="16" /> {{ t("about.back") }}
    </button>

    <header class="page-header">
      <h1>{{ t("help.title") }}</h1>
      <p>{{ t("help.subtitle") }}</p>
    </header>

    <section class="help-card">
      <div class="section-heading"><RotateCcw :size="18" /><h2>{{ t("help.reopenGuideTitle") }}</h2></div>
      <p>{{ t("help.reopenGuideDesc") }}</p>
      <button class="reopen-guide-btn" type="button" @click="reopenGuide">
        <RotateCcw :size="14" /> {{ t("help.reopenGuide") }}
      </button>
    </section>

    <section class="help-card">
      <div class="section-heading"><Wifi :size="18" /><h2>{{ t("help.connectTitle") }}</h2></div>
      <ol>
        <li>{{ t("help.connect.1") }}</li>
        <li>{{ t("help.connect.2") }}</li>
        <li>{{ t("help.connect.3") }}</li>
      </ol>
    </section>

    <section class="help-card">
      <div class="section-heading"><Send :size="18" /><h2>{{ t("help.sendTitle") }}</h2></div>
      <p>{{ t("help.send.1") }}</p>
      <p>{{ t("help.send.2") }}</p>
    </section>

    <section class="help-card">
      <div class="section-heading"><Download :size="18" /><h2>{{ t("help.receiveTitle") }}</h2></div>
      <p>{{ t("help.receive.1") }}</p>
    </section>

    <section class="help-card">
      <div class="section-heading"><Wrench :size="18" /><h2>{{ t("help.troubleshootTitle") }}</h2></div>
      <ul>
        <li>{{ t("help.troubleshoot.1") }}</li>
        <li>{{ t("help.troubleshoot.2") }}</li>
        <li>{{ t("help.troubleshoot.3") }}</li>
        <li>{{ t("help.troubleshoot.4") }}</li>
      </ul>
    </section>

    <section id="privacy" class="help-card">
      <div class="section-heading"><ShieldCheck :size="18" /><h2>{{ t("help.privacyTitle") }}</h2></div>
      <p>{{ t("help.privacy.1") }}</p>
      <p class="legal-links">
        <RouterLink to="/legal/privacy">{{ t("settings.privacy") }}</RouterLink>
        <RouterLink to="/legal/terms">{{ t("settings.terms") }}</RouterLink>
        <RouterLink to="/legal/disclaimer">{{ t("settings.disclaimer") }}</RouterLink>
      </p>
    </section>
  </div>
</template>

<style scoped>
.help-page { max-width: 760px; margin: 0 auto; padding: 28px 32px 48px; }
.back-btn { display: inline-flex; align-items: center; gap: 6px; border: 0; background: transparent; color: var(--color-text-secondary); cursor: pointer; padding: 6px 0; }
.page-header { margin: 18px 0 24px; }
.page-header h1 { margin: 0; font-size: var(--text-2xl); color: var(--color-text-primary); }
.page-header p { margin: 8px 0 0; color: var(--color-text-secondary); }
.help-card { padding: 20px 22px; margin-bottom: 14px; border: 1px solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-surface-card); box-shadow: var(--shadow-card); color: var(--color-text-secondary); line-height: 1.7; }
.section-heading { display: flex; align-items: center; gap: 8px; color: var(--color-text-primary); }
.section-heading svg { color: var(--color-brand-primary); }
.section-heading h2 { margin: 0; font-size: var(--text-lg); }
.help-card p { margin: 12px 0 0; }
.help-card ol, .help-card ul { margin: 12px 0 0; padding-left: 22px; }
.help-card code { padding: 1px 4px; border-radius: 4px; background: var(--color-surface-inset); color: var(--color-text-primary); }
.reopen-guide-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-top: 12px;
  padding: 7px 14px;
  border: 1px solid var(--color-brand-primary);
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--color-text-brand);
  font-size: var(--text-sm);
  cursor: pointer;
}
.reopen-guide-btn:hover { background: var(--color-selected); }
.legal-links { display: flex; flex-wrap: wrap; gap: 14px; }
.legal-links a { color: var(--color-brand-primary); text-decoration: none; }
.legal-links a:hover { text-decoration: underline; }
@media (max-width: 640px) { .help-page { padding: 20px 16px 36px; } }
</style>
