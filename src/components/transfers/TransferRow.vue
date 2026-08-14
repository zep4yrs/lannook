<script setup lang="ts">
// 传输中心任务行（紧凑 + 展开详情）
// 列：文件图标/名 | 进度(条+%) | 速度 | 剩余 | 大小 | 展开箭头
import { computed } from "vue";
import { ChevronDown, Pause, Play, X, RotateCcw } from "lucide-vue-next";
import { useLocale } from "@/i18n";
import type { TransferTask } from "@/types";
import { formatBytes, formatSpeed, formatRemaining } from "@/utils/format";
import StatusBadge from "@/components/common/StatusBadge.vue";
import TransferExpandedDetail from "./TransferExpandedDetail.vue";
import FileIcon from "@/components/files/FileIcon.vue";

const { t } = useLocale();
const props = defineProps<{
  task: TransferTask;
  expanded?: boolean;
  sourceName: string;
  targetName: string;
}>();

const emit = defineEmits<{
  toggleExpand: [];
  pause: [];
  resume: [];
  cancel: [];
  retry: [];
}>();

const firstFile = computed(() => props.task.files[0]);
const canPause = computed(() => props.task.status === "transferring");
const canResume = computed(() => props.task.status === "paused");
const canCancel = computed(
  () => props.task.status === "transferring" || props.task.status === "paused" || props.task.status === "verifying",
);
const canRetry = computed(() => props.task.status === "failed");
</script>

<template>
  <div class="task-row border-b" style="border-color: var(--color-border-default)">
    <div
      class="grid items-center px-3 py-2.5 cursor-pointer transition-colors row-head"
      style="grid-template-columns: 28px 1fr 120px 80px 72px 64px 32px"
      :style="expanded ? { background: 'var(--color-surface-active)' } : undefined"
      @click="emit('toggleExpand')"
    >
      <FileIcon
        v-if="firstFile"
        :name="firstFile.name"
        :mimeType="firstFile.mimeType"
        :size="20"
      />
      <div class="flex flex-col min-w-0">
        <div class="flex items-center gap-2 min-w-0">
          <span
            class="cell-truncate"
            :style="{
              fontSize: 'var(--text-sm)',
              fontWeight: 'var(--font-weight-medium)',
              color: 'var(--color-text-primary)',
            }"
            >{{ firstFile?.name ?? "—" }}</span
          >
          <StatusBadge :status="task.status" />
        </div>
        <span
          class="cell-truncate"
          style="font-size: var(--text-xs); color: var(--color-text-tertiary)"
          >{{ sourceName }} → {{ targetName }}</span
        >
      </div>
      <!-- Progress -->
      <div class="flex items-center gap-2">
        <div class="progress-track flex-1">
          <div
            class="progress-fill"
            :style="{ width: `${Math.round(task.progress * 100)}%` }"
          />
        </div>
        <span
          style="font-size: var(--text-xs); color: var(--color-primary); font-weight: var(--font-weight-medium); font-family: var(--font-mono); white-space: nowrap"
          >{{ Math.round(task.progress * 100) }}%</span
        >
      </div>
      <span style="font-size: var(--text-xs); color: var(--color-text-secondary); font-family: var(--font-mono)">{{
        formatSpeed(task.speedBytesPerSecond)
      }}</span>
      <span style="font-size: var(--text-xs); color: var(--color-text-tertiary); text-align: right; font-family: var(--font-mono)">{{
        formatRemaining(task.remainingSeconds)
      }}</span>
      <span style="font-size: var(--text-xs); color: var(--color-text-tertiary); text-align: right; font-family: var(--font-mono)">{{
        formatBytes(task.totalBytes)
      }}</span>
      <div
        class="flex items-center justify-center expand-arrow"
        :class="{ 'expand-arrow-expanded': expanded }"
        style="color: var(--color-text-tertiary)"
      >
        <ChevronDown :size="16" :stroke-width="2" />
      </div>
    </div>

    <!-- Actions row (hover-only) -->
    <div
      v-if="canPause || canResume || canCancel || canRetry"
      class="flex items-center justify-end gap-1 px-3 pb-1 actions"
    >
      <button
        v-if="canPause"
        class="icon-btn"
        style="width: 26px; height: 26px"
        :title="t('transfers.pause')"
        :aria-label="t('transfers.pause')"
        @click.stop="emit('pause')"
      >
        <Pause :size="14" :stroke-width="2.5" />
      </button>
      <button
        v-if="canResume"
        class="icon-btn"
        style="width: 26px; height: 26px"
        :title="t('transfers.resume')"
        :aria-label="t('transfers.resume')"
        @click.stop="emit('resume')"
      >
        <Play :size="14" :stroke-width="2.5" />
      </button>
      <button
        v-if="canRetry"
        class="icon-btn"
        style="width: 26px; height: 26px"
        :title="t('transfers.retry')"
        :aria-label="t('transfers.retry')"
        @click.stop="emit('retry')"
      >
        <RotateCcw :size="14" :stroke-width="2.5" />
      </button>
      <button
        v-if="canCancel"
        class="icon-btn cancel-btn"
        style="width: 26px; height: 26px"
        :title="t('transfers.cancel')"
        :aria-label="t('transfers.cancel')"
        @click.stop="emit('cancel')"
      >
        <X :size="14" :stroke-width="2.5" />
      </button>
    </div>

    <!-- Expanded detail -->
    <Transition name="height-expand">
      <div v-if="expanded" class="grid" style="grid-template-rows: 1fr">
        <div class="overflow-hidden">
          <TransferExpandedDetail :task="task" />
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.row-head:hover {
  background: var(--color-surface-secondary);
}
.actions {
  opacity: 0;
  transition: opacity var(--transition-fast);
}
.task-row:hover .actions {
  opacity: 1;
}
.cancel-btn:hover {
  color: var(--state-error);
}
</style>
