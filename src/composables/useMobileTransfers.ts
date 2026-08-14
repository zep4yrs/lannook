import { computed, onUnmounted, ref, watch, type Ref } from "vue";
import {
  cancelTransferApi,
  getMyTransfers,
  pauseTransferApi,
  resumeTransferApi,
} from "@/services/api";
import { wsClient, type WsMessage } from "@/services/websocket";
import { useLocale } from "@/i18n";
import type { TransferTask } from "@/types";

interface MobileTransfersOptions {
  sessionToken: Ref<string | null>;
  isApproved: Ref<boolean>;
}

/**
 * Browser clients are deliberately limited to their own transfers. This
 * composable keeps that scoped list fresh without borrowing desktop-only
 * Pinia state or Tauri commands.
 */
export function useMobileTransfers({ sessionToken, isApproved }: MobileTransfersOptions) {
  const { t } = useLocale();
  const transfers = ref<TransferTask[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  let pollTimer: ReturnType<typeof window.setInterval> | null = null;

  function applyProgress(message: WsMessage) {
    const data = message.payload as {
      transferId?: string;
      progress?: number;
      transferredBytes?: number;
      speedBytesPerSecond?: number;
      remainingSeconds?: number | null;
    } | undefined;
    const transferId = data?.transferId;
    if (!transferId) return;

    const transfer = transfers.value.find((item) => item.id === transferId);
    if (!transfer) {
      void refresh();
      return;
    }

    if (typeof data.progress === "number") transfer.progress = data.progress / 100;
    if (typeof data.transferredBytes === "number") {
      transfer.transferredBytes = data.transferredBytes;
    }
    if (typeof data.speedBytesPerSecond === "number") {
      transfer.speedBytesPerSecond = data.speedBytesPerSecond;
    }
    if (typeof data.remainingSeconds === "number") {
      transfer.remainingSeconds = data.remainingSeconds;
    } else if (data?.remainingSeconds === null) {
      transfer.remainingSeconds = undefined;
    }
    if (transfer.status !== "paused") transfer.status = "transferring";
  }

  function applyTerminalStatus(status: "completed" | "cancelled" | "failed") {
    return (message: WsMessage) => {
      const transferId = message.payload?.transferId;
      if (typeof transferId !== "string") return;
      const transfer = transfers.value.find((item) => item.id === transferId);
      if (!transfer) {
        void refresh();
        return;
      }
      transfer.status = status;
      transfer.speedBytesPerSecond = 0;
      transfer.remainingSeconds = 0;
      if (status === "completed") {
        transfer.progress = 1;
        transfer.completedAt = new Date().toISOString();
      }
    };
  }

  const onTransferCompleted = applyTerminalStatus("completed");
  const onTransferCancelled = applyTerminalStatus("cancelled");
  const onTransferFailed = applyTerminalStatus("failed");
  const onSocketConnected = () => void refresh();

  function bindSocketListeners() {
    wsClient.on("transfer.progress", applyProgress);
    wsClient.on("transfer.download_progress", applyProgress);
    wsClient.on("transfer.created", onSocketConnected);
    wsClient.on("transfer.completed", onTransferCompleted);
    wsClient.on("transfer.cancelled", onTransferCancelled);
    wsClient.on("transfer.failed", onTransferFailed);
    wsClient.on("connected", onSocketConnected);
  }

  function unbindSocketListeners() {
    wsClient.off("transfer.progress", applyProgress);
    wsClient.off("transfer.download_progress", applyProgress);
    wsClient.off("transfer.created", onSocketConnected);
    wsClient.off("transfer.completed", onTransferCompleted);
    wsClient.off("transfer.cancelled", onTransferCancelled);
    wsClient.off("transfer.failed", onTransferFailed);
    wsClient.off("connected", onSocketConnected);
  }

  const currentTransfer = computed(() =>
    transfers.value.find((transfer) =>
      ["transferring", "verifying", "waiting", "requesting", "awaiting_acceptance", "accepted", "paused"].includes(transfer.status)
    ) ?? transfers.value[0] ?? null
  );

  function stopPolling() {
    if (pollTimer !== null) {
      window.clearInterval(pollTimer);
      pollTimer = null;
    }
  }

  async function refresh() {
    const token = sessionToken.value;
    if (!token || !isApproved.value) {
      transfers.value = [];
      return;
    }

    loading.value = true;
    try {
      const response = await getMyTransfers(token);
      const incoming = Array.isArray(response.transfers)
        ? response.transfers as TransferTask[]
        : [];
      const liveById = new Map(transfers.value.map((transfer) => [transfer.id, transfer]));
      transfers.value = incoming.map((snapshot) => {
        const live = liveById.get(snapshot.id);
        if (!live || ["completed", "cancelled", "failed", "rejected", "expired"].includes(snapshot.status)) {
          return snapshot;
        }
        return {
          ...snapshot,
          progress: Math.max(snapshot.progress, live.progress),
          transferredBytes: Math.max(snapshot.transferredBytes, live.transferredBytes),
          speedBytesPerSecond: live.speedBytesPerSecond || snapshot.speedBytesPerSecond,
          remainingSeconds: live.remainingSeconds ?? snapshot.remainingSeconds,
          status: live.status === "paused" ? "paused" : snapshot.status,
        };
      });
      error.value = null;
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : t("mobile.loadStatusFailed");
    } finally {
      loading.value = false;
    }
  }

  async function runAction(action: (token: string) => Promise<unknown>) {
    const token = sessionToken.value;
    if (!token || !isApproved.value) {
      error.value = t("mobile.notAuthorized");
      return;
    }
    try {
      await action(token);
      await refresh();
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : t("mobile.transferOperationFailed");
    }
  }

  async function pause(transferId: string) {
    await runAction((token) => pauseTransferApi(transferId, token));
  }

  async function resume(transferId: string) {
    await runAction((token) => resumeTransferApi(transferId, token));
  }

  async function cancel(transferId: string) {
    await runAction((token) => cancelTransferApi(transferId, token));
  }

  watch(
    [sessionToken, isApproved],
    ([token, approved]) => {
      stopPolling();
      unbindSocketListeners();
      if (!token || !approved) {
        transfers.value = [];
        return;
      }
      void refresh();
      bindSocketListeners();
      pollTimer = window.setInterval(() => void refresh(), 2500);
    },
    { immediate: true }
  );

  onUnmounted(() => {
    stopPolling();
    unbindSocketListeners();
  });

  return {
    transfers,
    currentTransfer,
    loading,
    error,
    refresh,
    pause,
    resume,
    cancel,
  };
}
