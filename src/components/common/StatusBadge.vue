<script setup lang="ts">
// 传输状态徽章：彩色圆点 + 文案，颜色使用设计令牌
import { computed } from "vue";
import { useLocale } from "@/i18n";
import type { TransferStatus } from "@/types";

const props = defineProps<{
  status: TransferStatus;
}>();
const { t } = useLocale();

// 全量映射，新增状态时 TS 会强制补齐
const statusStyles: Record<
  TransferStatus,
  { labelKey: string; color: string; bg: string }
> = {
  pending: { labelKey: "status.pending", color: "var(--color-state-warning)", bg: "var(--color-state-warning-soft)" },
  waiting: { labelKey: "status.waiting", color: "var(--color-state-info)", bg: "var(--color-state-info-soft)" },
  requesting: { labelKey: "status.requesting", color: "var(--color-state-info)", bg: "var(--color-state-info-soft)" },
  awaiting_acceptance: { labelKey: "status.awaitingAcceptance", color: "var(--color-state-warning)", bg: "var(--color-state-warning-soft)" },
  accepted: { labelKey: "status.accepted", color: "var(--color-state-info)", bg: "var(--color-state-info-soft)" },
  transferring: { labelKey: "status.transferring", color: "var(--color-state-info)", bg: "var(--color-state-info-soft)" },
  paused: { labelKey: "status.paused", color: "var(--color-state-warning)", bg: "var(--color-state-warning-soft)" },
  verifying: { labelKey: "status.verifying", color: "var(--color-state-info)", bg: "var(--color-state-info-soft)" },
  completed: { labelKey: "status.completed", color: "var(--color-state-success)", bg: "var(--color-state-success-soft)" },
  rejected: { labelKey: "status.rejected", color: "var(--color-state-error)", bg: "var(--color-state-error-soft)" },
  expired: { labelKey: "status.expired", color: "var(--color-text-tertiary)", bg: "var(--color-surface-secondary)" },
  cancelled: { labelKey: "status.cancelled", color: "var(--color-text-tertiary)", bg: "var(--color-surface-secondary)" },
  failed: { labelKey: "status.failed", color: "var(--color-state-error)", bg: "var(--color-state-error-soft)" },
};

const config = computed(() => statusStyles[props.status]);
</script>

<template>
  <span
    class="status-badge"
    :style="{ color: config.color, backgroundColor: config.bg }"
  >
    <span class="status-dot" :style="{ backgroundColor: config.color }" />
    {{ t(config.labelKey) }}
  </span>
</template>

<style scoped>
.status-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  flex-shrink: 0;
  padding: 1px 8px;
  border-radius: 999px;
  font-size: var(--text-xs);
  font-weight: var(--font-weight-medium);
  line-height: var(--leading-normal);
  white-space: nowrap;
}
.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}
</style>
