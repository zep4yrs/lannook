<script setup lang="ts">
// Toast 宿主：右下角浮层提示
// audit-3: severity-scaled lifetimes (errors stay until dismissed),
// hover pauses the countdown, and every toast has an explicit close button.
// audit-4: icon colors now use the real --color-state-* tokens.
import { onBeforeUnmount, watch } from "vue";
import { CheckCircle2, XCircle, Info, AlertTriangle, X } from "lucide-vue-next";
import { useAppStore } from "@/stores/app";
import { useLocale } from "@/i18n";
import type { ToastKind } from "@/types";

const app = useAppStore();
const { t } = useLocale();

const iconMap = {
  success: CheckCircle2,
  error: XCircle,
  info: Info,
  warning: AlertTriangle,
};
const colorMap: Record<ToastKind, string> = {
  success: "var(--color-state-success)",
  error: "var(--color-state-error)",
  info: "var(--color-state-info)",
  warning: "var(--color-state-warning)",
};
/** Auto-dismiss delay per severity; 0 = persists until closed manually. */
const durationMap: Record<ToastKind, number> = {
  success: 3000,
  info: 4000,
  warning: 6000,
  error: 0,
};

const timers = new Map<number, ReturnType<typeof setTimeout>>();

function armTimer(id: number, kind: ToastKind) {
  const duration = durationMap[kind];
  if (duration <= 0) return;
  disarmTimer(id);
  timers.set(
    id,
    setTimeout(() => {
      timers.delete(id);
      app.dismissToast(id);
    }, duration)
  );
}

function disarmTimer(id: number) {
  const timer = timers.get(id);
  if (timer) {
    clearTimeout(timer);
    timers.delete(id);
  }
}

watch(
  () => app.toasts.map((toast) => toast.id).join(","),
  () => {
    for (const toast of app.toasts) {
      if (!timers.has(toast.id)) armTimer(toast.id, toast.kind);
    }
  },
  { immediate: true }
);

onBeforeUnmount(() => {
  for (const id of [...timers.keys()]) disarmTimer(id);
});
</script>

<template>
  <div class="toast-host" aria-live="polite">
    <TransitionGroup name="toast">
      <div
        v-for="toast in app.toasts"
        :key="toast.id"
        class="toast-item"
        role="status"
        @click="app.dismissToast(toast.id)"
        @mouseenter="disarmTimer(toast.id)"
        @mouseleave="armTimer(toast.id, toast.kind)"
      >
        <component
          :is="iconMap[toast.kind]"
          :size="16"
          :stroke-width="2.5"
          :style="{ color: colorMap[toast.kind] }"
        />
        <div class="min-w-0 flex-1">
          <div style="font-size: var(--text-sm); font-weight: var(--font-weight-medium); color: var(--color-text-primary)">
            {{ toast.title }}
          </div>
          <div v-if="toast.description" style="font-size: var(--text-xs); color: var(--color-text-tertiary); margin-top: 2px">
            {{ toast.description }}
          </div>
        </div>
        <button class="toast-close" type="button" :aria-label="t('common.close')" @click.stop="app.dismissToast(toast.id)">
          <X :size="13" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>

<style scoped>
.toast-host {
  position: fixed;
  bottom: 24px;
  right: 24px;
  z-index: 100;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
}
.toast-item {
  pointer-events: auto;
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  min-width: 240px;
  max-width: 360px;
  background: var(--color-surface-content);
  border: 1px solid var(--color-border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-float);
  cursor: pointer;
}
.toast-close {
  flex-shrink: 0;
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--color-text-tertiary);
  cursor: pointer;
}
.toast-close:hover {
  background: var(--color-hover);
  color: var(--color-text-primary);
}
</style>
