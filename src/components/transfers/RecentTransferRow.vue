<script setup lang="ts">
// 桌面首页"最近传输"行
import { computed } from "vue";
import { CheckCircle2 } from "lucide-vue-next";
import type { TransferTask } from "@/types";
import { formatBytes, formatRelativeTime } from "@/utils/format";
import FileIcon from "@/components/files/FileIcon.vue";
import { useLocale } from "@/i18n";

const props = defineProps<{
  task: TransferTask;
  sourceName: string;
  targetName: string;
}>();
const { t } = useLocale();

const firstFile = computed(() => props.task.files[0]);
const timeText = computed(() =>
  props.task.completedAt
    ? formatRelativeTime(props.task.completedAt)
    : formatRelativeTime(props.task.createdAt),
);
</script>

<template>
  <div
    class="grid items-center px-3 py-2.5 transition-colors row"
    style="grid-template-columns: 1fr 1fr 80px 72px 64px; transition: background var(--transition-fast)"
  >
    <div class="flex items-center gap-2 min-w-0">
      <FileIcon
        v-if="firstFile"
        :name="firstFile.name"
        :mimeType="firstFile.mimeType"
        :size="14"
      />
      <span class="cell-truncate" style="font-size: var(--text-sm); color: var(--color-text-primary)">{{ firstFile?.name ?? "—" }}</span>
    </div>
    <span class="cell-truncate" style="font-size: var(--text-xs); color: var(--color-text-secondary)"
      >{{ sourceName }} → {{ targetName }}</span
    >
    <span style="font-size: var(--text-xs); color: var(--color-text-tertiary)">{{ timeText }}</span>
    <span style="font-size: var(--text-xs); color: var(--color-text-tertiary); text-align: right; font-family: var(--font-mono)">{{
      formatBytes(firstFile?.size ?? 0)
    }}</span>
    <div
      class="flex items-center justify-end gap-1"
      style="font-size: var(--text-xs); color: var(--state-success)"
    >
      <CheckCircle2 :size="12" :stroke-width="2.5" />
      <span>{{ t("transfers.completed") }}</span>
    </div>
  </div>
</template>

<style scoped>
.row:hover {
  background: var(--color-surface-secondary);
}
</style>
