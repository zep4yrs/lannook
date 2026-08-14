<script setup lang="ts">
// 手机端已选文件项
import { X } from "lucide-vue-next";
import FileIcon from "./FileIcon.vue";
import { useLocale } from "@/i18n";
import type { TransferFile } from "@/types";
import { formatBytes } from "@/utils/format";

defineProps<{ file: TransferFile; last?: boolean }>();
const { t } = useLocale();
const emit = defineEmits<{ remove: [] }>();
</script>

<template>
  <div
    class="flex items-center gap-3 px-3.5 py-3"
    :style="{
      background: 'var(--color-surface-content)',
      borderBottom: last ? 'none' : '1px solid var(--color-border-default)',
    }"
  >
    <div
      class="w-9 h-9 rounded-md flex items-center justify-center flex-shrink-0"
      :style="{ background: 'var(--color-surface-secondary)' }"
    >
      <FileIcon :name="file.name" :mimeType="file.mimeType" :size="16" />
    </div>
    <div class="flex-1 min-w-0">
      <div class="text-sm font-medium cell-truncate" style="color: var(--color-text-primary)">{{ file.name }}</div>
      <div class="text-xs" style="color: var(--color-text-tertiary)">{{ formatBytes(file.size) }}</div>
    </div>
    <button
      class="remove-btn w-8 h-8 rounded-full flex items-center justify-center flex-shrink-0 transition-colors duration-200"
      style="color: var(--color-text-tertiary)"
      :aria-label="t('common.remove')"
      @click="emit('remove')"
    >
      <X :size="16" :stroke-width="2" />
    </button>
  </div>
</template>

<style scoped>
.remove-btn:hover {
  background: rgba(217, 78, 78, 0.08);
  color: var(--state-error);
}
</style>
