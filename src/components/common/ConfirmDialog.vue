<script setup lang="ts">
import { ref } from "vue";
import { AlertTriangle, Info, X } from "lucide-vue-next";
import { useLocale } from "@/i18n";
import { useModalA11y } from "@/composables/useModalA11y";

const props = defineProps<{
  visible: boolean;
  title: string;
  description?: string;
  confirmLabel: string;
  cancelLabel?: string;
  /** "danger" renders destructive styling and focuses the cancel button. */
  tone?: "danger" | "primary";
  loading?: boolean;
}>();

const emit = defineEmits<{
  confirm: [];
  cancel: [];
}>();

const { t } = useLocale();
const cardElement = ref<HTMLElement | null>(null);
const cancelButton = ref<HTMLButtonElement | null>(null);

useModalA11y({
  visible: () => props.visible,
  container: cardElement,
  onEscape: () => emit("cancel"),
  initialFocus: () => cancelButton.value,
});
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="confirm-wrapper" role="presentation">
      <div class="confirm-backdrop" @click="emit('cancel')" />
      <section
        ref="cardElement"
        class="confirm-card"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        :aria-describedby="description ? 'confirm-description' : undefined"
      >
        <button class="confirm-close" type="button" :aria-label="t('common.close')" :disabled="loading" @click="emit('cancel')">
          <X :size="15" />
        </button>
        <div class="confirm-icon" :class="{ 'confirm-icon--danger': tone === 'danger' }" aria-hidden="true">
          <AlertTriangle v-if="tone === 'danger'" :size="22" />
          <Info v-else :size="22" />
        </div>
        <h2 id="confirm-title" class="confirm-title">{{ title }}</h2>
        <p v-if="description" id="confirm-description" class="confirm-description">{{ description }}</p>
        <div class="confirm-actions">
          <button
            ref="cancelButton"
            class="confirm-btn confirm-btn--secondary"
            type="button"
            :disabled="loading"
            @click="emit('cancel')"
          >
            {{ cancelLabel ?? t("common.cancel") }}
          </button>
          <button
            class="confirm-btn"
            :class="tone === 'danger' ? 'confirm-btn--danger' : 'confirm-btn--primary'"
            type="button"
            :disabled="loading"
            @click="emit('confirm')"
          >
            {{ confirmLabel }}
          </button>
        </div>
      </section>
    </div>
  </Teleport>
</template>

<style scoped>
.confirm-wrapper { position: fixed; inset: 0; z-index: calc(var(--z-modal, 900) + 10); display: grid; place-items: center; padding: 20px; }
.confirm-backdrop { position: fixed; inset: 0; background: rgba(15, 23, 42, 0.46); animation: confirm-fade 160ms ease forwards; }
.confirm-card {
  position: relative; width: min(100%, 380px); padding: 26px 24px 20px; color: var(--color-text-primary);
  background: var(--color-surface-card); border: 1px solid var(--color-border); border-radius: var(--radius-lg);
  box-shadow: var(--shadow-xl); animation: confirm-scale 180ms cubic-bezier(0.16, 1, 0.3, 1) forwards;
}
.confirm-close { position: absolute; top: 12px; right: 12px; display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: var(--radius-sm); color: var(--color-text-tertiary); background: transparent; cursor: pointer; }
.confirm-close:hover:not(:disabled) { background: var(--color-hover); color: var(--color-text-primary); }
.confirm-icon { display: grid; place-items: center; width: 44px; height: 44px; margin-bottom: 14px; color: var(--color-brand-primary); background: var(--color-brand-primary-soft); border-radius: var(--radius-full); }
.confirm-icon--danger { color: var(--color-state-error); background: var(--color-state-error-soft); }
.confirm-title { margin: 0; font-size: var(--text-lg); font-weight: var(--weight-semibold); }
.confirm-description { margin: 10px 0 0; color: var(--color-text-secondary); font-size: var(--text-sm); line-height: 1.6; }
.confirm-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 22px; }
.confirm-btn { display: inline-flex; align-items: center; justify-content: center; min-height: 38px; padding: 0 14px; border-radius: var(--radius-md); font-size: var(--text-sm); font-weight: var(--weight-medium); cursor: pointer; transition: filter var(--transition-fast), background var(--transition-fast); }
.confirm-btn:disabled { cursor: wait; opacity: 0.6; }
.confirm-btn--primary { color: var(--color-text-inverse); background: var(--color-brand-primary); border: 1px solid var(--color-brand-primary); }
.confirm-btn--primary:hover:not(:disabled) { background: var(--color-brand-primary-hover); }
.confirm-btn--danger { color: #fff; background: var(--color-state-error); border: 1px solid var(--color-state-error); }
.confirm-btn--danger:hover:not(:disabled) { filter: brightness(1.08); }
.confirm-btn--secondary { color: var(--color-text-secondary); background: var(--color-surface-card); border: 1px solid var(--color-border); }
.confirm-btn--secondary:hover:not(:disabled) { background: var(--color-hover); color: var(--color-text-primary); }
@keyframes confirm-fade { from { opacity: 0; } to { opacity: 1; } }
@keyframes confirm-scale { from { opacity: 0; transform: scale(0.96); } to { opacity: 1; transform: scale(1); } }
</style>
