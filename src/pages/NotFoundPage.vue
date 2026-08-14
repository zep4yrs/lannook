<script setup lang="ts">
import { useRouter } from "vue-router";
import { Home } from "lucide-vue-next";
import { useLocale } from "@/i18n";

const router = useRouter();
const { t } = useLocale();

function goHome() {
  router.push("/");
}
</script>

<template>
  <div class="not-found">
    <div class="not-found-code">404</div>
    <p class="not-found-text">{{ t("notFound.title") }}</p>
    <p class="not-found-hint">{{ t("notFound.hint") }}</p>
    <button class="not-found-btn" @click="goHome">
      <Home :size="16" />
      {{ t("notFound.back") }}
    </button>
  </div>
</template>

<style scoped>
.not-found {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 60vh;
  text-align: center;
  padding: 32px;
}

.not-found-code {
  font-size: 72px;
  font-weight: var(--weight-semibold, 600);
  color: var(--color-brand-primary);
  line-height: 1;
  margin-bottom: 12px;
}

.not-found-text {
  font-size: var(--text-xl, 18px);
  font-weight: var(--weight-medium, 500);
  color: var(--color-text-primary);
  margin: 0 0 6px;
}

.not-found-hint {
  font-size: var(--text-sm, 13px);
  color: var(--color-text-tertiary);
  margin: 0 0 24px;
}

.not-found-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 8px 18px;
  border: none;
  border-radius: var(--radius-md, 8px);
  background: var(--color-brand-primary);
  color: var(--color-text-inverse);
  font-size: var(--text-base, 14px);
  font-weight: var(--weight-medium, 500);
  cursor: pointer;
  transition: background var(--transition-fast, 140ms ease);
}

.not-found-btn:hover {
  background: var(--color-brand-primary-hover);
}
</style>
