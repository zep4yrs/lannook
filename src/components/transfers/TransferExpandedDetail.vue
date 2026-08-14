<script setup lang="ts">
// 任务展开详情面板（设计稿 任务展开详情.html）
// 显示：分片进度、开始时间、文件校验、重试次数、保存目录、文件大小、分片大小、传输协议
import { computed } from "vue";
import { useLocale } from "@/i18n";
import type { TransferTask } from "@/types";
import { formatBytes, formatClock } from "@/utils/format";
import ChunkBar from "./ChunkBar.vue";

const { t } = useLocale();
const props = defineProps<{ task: TransferTask }>();

const chunkTotal = computed(() => props.task.chunkTotal ?? 0);
const chunkDone = computed(() => props.task.chunkDone ?? 0);
</script>

<template>
  <div class="px-3 pb-3 pt-1" style="margin-left: 16px">
    <div
      class="rounded-md px-4 py-3"
      style="background: var(--color-surface-secondary); border-left: 3px solid var(--color-primary); border-radius: var(--radius-md)"
    >
      <!-- Chunk Progress -->
      <div class="mb-3">
        <div class="flex items-center justify-between mb-1.5">
          <span style="font-size: var(--text-xs); color: var(--color-text-tertiary)">分片进度</span>
          <span style="font-size: var(--text-xs); color: var(--color-text-tertiary); font-family: var(--font-mono)">
            {{ chunkDone }} / {{ chunkTotal }}
          </span>
        </div>
        <ChunkBar v-if="chunkTotal > 0" :total="chunkTotal" :done="chunkDone" />
      </div>

      <!-- Detail Info Grid -->
      <div class="grid gap-x-8 gap-y-2" style="grid-template-columns: 1fr 1fr">
        <!-- Left Column -->
        <div>
          <div class="flex items-center justify-between mb-2">
            <span class="detail-label">{{ t("transfers.startedAt") }}</span>
            <span class="detail-value">{{ formatClock(task.startedAt) }}</span>
          </div>
          <div class="flex items-center justify-between mb-2">
            <span class="detail-label">{{ t("transfers.checksum") }}</span>
            <span class="detail-value">SHA-256</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="detail-label">{{ t("transfers.retryCountLabel") }}</span>
            <span class="detail-value">{{ task.retryCount ?? 0 }}</span>
          </div>
        </div>
        <!-- Right Column -->
        <div>
          <div class="flex items-center justify-between mb-2">
            <span class="detail-label">{{ t("transfers.saveDir") }}</span>
            <span class="detail-value" style="font-family: var(--font-sans)">{{ task.savePath || "—" }}</span>
          </div>
          <div class="flex items-center justify-between mb-2">
            <span class="detail-label">{{ t("transfers.fileSize") }}</span>
            <span class="detail-value">{{ formatBytes(task.totalBytes) }}</span>
          </div>
          <div class="flex items-center justify-between mb-2">
            <span class="detail-label">{{ t("transfers.chunkSize") }}</span>
            <span class="detail-value">{{ task.chunkSize ? formatBytes(task.chunkSize, 0) : "—" }}</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="detail-label">{{ t("transfers.protocol") }}</span>
            <span class="detail-value">{{ task.protocol || "HTTP" }}</span>
          </div>
        </div>
      </div>

      <!-- Error -->
      <div
        v-if="task.error"
        class="mt-3 px-3 py-2 rounded-md flex items-start gap-2"
        style="background: rgba(217,78,78,0.08); color: var(--state-error); font-size: var(--text-xs)"
      >
        <span>{{ task.error }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.detail-label {
  font-size: 12px;
  color: var(--color-text-tertiary);
  line-height: var(--leading-normal);
}
.detail-value {
  font-size: 13px;
  color: var(--color-text-primary);
  line-height: var(--leading-normal);
  font-family: var(--font-mono);
}
</style>
