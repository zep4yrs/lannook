import { defineStore } from "pinia";
import { shallowRef } from "vue";
import {
  acceptTransfer,
  getCurrentDevice,
  downloadFileWithProgress,
  getPendingTransfersApi,
  pairWithPin,
  registerDevice,
  rejectTransfer,
  validateToken,
} from "@/services/api";
import { wsClient } from "@/services/websocket";
import { translate } from "@/i18n";
import { delay } from "@/utils/format";
import { readAndMigrateLocalStorageValue } from "@/utils/storage";

export interface IncomingTransfer {
  id: string;
  sourceDeviceName: string;
  files: { id: string; name: string; size: number }[];
  totalBytes: number;
  expiresAt?: string;
}

/** Per-file live state for the receive flow (audit-30). */
export interface ReceiveDownloadItem {
  fileId: string;
  name: string;
  size: number;
  status: "queued" | "downloading" | "done" | "failed";
  loadedBytes: number;
  error?: string;
}

export type MobileConnectionPhase =
  | "initializing"
  | "pin_entry"
  | "pending_approval"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "rejected"
  | "revoked"
  | "error";

interface RegistrationResponse {
  sessionToken?: string;
  deviceId?: string;
  approved?: boolean;
}

const MOBILE_CLIENT_ID_KEY = "lannook-mobile-client-id";
// The issued session token is persisted so a page refresh can prove it still
// owns its device record. Without it, the server treats the browser as an
// unknown peer and requires re-approval on every reload.
const MOBILE_SESSION_TOKEN_KEY = "lannook-mobile-session-token";
const LEGACY_MOBILE_CLIENT_ID_KEYS = ["lynqo-mobile-client-id"] as const;
let transientClientIdSequence = 0;
let transientClientId: string | null = null;

interface UserAgentDataLike {
  getHighEntropyValues?: (hints: string[]) => Promise<{ model?: string }>;
}

export const useMobileSessionStore = defineStore("mobileSession", () => {
  const sessionToken = shallowRef<string | null>(null);
  const deviceId = shallowRef<string | null>(null);
  const isApproved = shallowRef(false);
  const isReady = shallowRef(false);
  const connectionPhase = shallowRef<MobileConnectionPhase>("initializing");
  const connectionError = shallowRef<string | null>(null);
  const receiveError = shallowRef<string | null>(null);
  const pendingReceiveTransfer = shallowRef<IncomingTransfer | null>(null);
  const showReceiveDialog = shallowRef(false);
  // Live per-file download state consumed by ReceiveRequestDialog (audit-30).
  const receiveDownloads = shallowRef<ReceiveDownloadItem[]>([]);
  const isReceiving = shallowRef(false);
  let activeReceiveTransfer: IncomingTransfer | null = null;
  /** One-time download credentials issued by accept_transfer (fileId → token). */
  let activeDownloadTokens = new Map<string, string>();

  let pairingToken = "";
  let socketToken = "";
  let socketConnected = false;
  let explicitRejection = false;
  let approvalSyncInFlight = false;
  let approvalPollTimer: ReturnType<typeof window.setInterval> | null = null;
  let initializationVersion = 0;
  let listenersBound = false;
  let visibilityListenerBound = false;

  function getAndroidModelFromUserAgent(userAgent: string): string | null {
    const match = userAgent.match(
      /Android\s+[^;]+;\s*(?:[a-z]{2}-[A-Z]{2};\s*)?([^;()]+?)(?:\s+Build\/|;|\))/i
    );
    const model = match?.[1]?.trim();
    return model && model.length > 1 && model.toLowerCase() !== "wv" ? model : null;
  }

  async function getBrowserDeviceName(clientId: string): Promise<string> {
    const ua = navigator.userAgent;
    if (/Android/.test(ua)) {
      const userAgentData = (navigator as Navigator & { userAgentData?: UserAgentDataLike })
        .userAgentData;
      try {
        const model = (await userAgentData?.getHighEntropyValues?.(["model"]))?.model?.trim();
        if (model) return `Android · ${model}`;
      } catch {
        // Some browsers withhold high-entropy hints. The regular UA and stable
        // suffix still provide a non-fabricated, distinguishable name.
      }
      const model = getAndroidModelFromUserAgent(ua);
      return model ? `Android · ${model}` : `Android · ${clientId.slice(-4).toUpperCase()}`;
    }

    const suffix = clientId.slice(-4).toUpperCase();
    if (/iPhone/.test(ua)) return `iPhone · ${suffix}`;
    if (/iPad/.test(ua)) return `iPad · ${suffix}`;
    return `Web · ${suffix}`;
  }

  function detectPlatform(): string {
    const ua = navigator.userAgent;
    if (/iPhone|iPad/.test(ua)) return "ios";
    if (/Android/.test(ua)) return "android";
    return "web";
  }

  function createClientId(): string {
    const uuid = window.crypto?.randomUUID?.();
    if (uuid) return uuid;

    const bytes = new Uint8Array(16);
    window.crypto?.getRandomValues?.(bytes);
    transientClientIdSequence += 1;
    const entropy = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    return `mobile-${Date.now().toString(36)}-${transientClientIdSequence}-${entropy}`;
  }

  function getStableClientId(): string {
    try {
      const existing = readAndMigrateLocalStorageValue(
        MOBILE_CLIENT_ID_KEY,
        LEGACY_MOBILE_CLIENT_ID_KEYS
      );
      if (existing && existing.length >= 16) return existing;

      const generated = createClientId();
      window.localStorage.setItem(MOBILE_CLIENT_ID_KEY, generated);
      return generated;
    } catch {
      // Private browsing may disable storage. Keep one identity for the tab so
      // repeated registration calls still do not create duplicate devices.
      transientClientId ??= createClientId();
      return transientClientId;
    }
  }

  function stopApprovalPolling() {
    if (approvalPollTimer !== null) {
      window.clearInterval(approvalPollTimer);
      approvalPollTimer = null;
    }
  }

  function clearIncomingRequest() {
    pendingReceiveTransfer.value = null;
    showReceiveDialog.value = false;
    receiveError.value = null;
    receiveDownloads.value = [];
    isReceiving.value = false;
    activeReceiveTransfer = null;
    activeDownloadTokens = new Map();
  }

  function patchDownloadItem(fileId: string, patch: Partial<ReceiveDownloadItem>) {
    receiveDownloads.value = receiveDownloads.value.map((item) =>
      item.fileId === fileId ? { ...item, ...patch } : item
    );
  }

  /** Trigger one browser download and release the object URL afterwards. */
  function saveBlobToDevice(blob: Blob, name: string) {
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = name;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
  }

  async function downloadSingleFile(
    transferId: string,
    token: string,
    device: string,
    file: { id: string; name: string; size: number }
  ): Promise<void> {
    let lastReportedPercent = -1;
    const blob = await downloadFileWithProgress(
      transferId,
      file.id,
      token,
      device,
      ({ loadedBytes }) => {
        // Throttle reactive updates to whole-percent steps.
        const percent = file.size > 0 ? Math.floor((loadedBytes / file.size) * 100) : 100;
        if (percent !== lastReportedPercent) {
          lastReportedPercent = percent;
          patchDownloadItem(file.id, { loadedBytes });
        }
      }
    );
    saveBlobToDevice(blob, file.name);
  }

  /**
   * Retry a single failed file without re-accepting the whole transfer.
   */
  async function retryDownloadFile(fileId: string): Promise<void> {
    const token = sessionToken.value;
    const transfer = activeReceiveTransfer;
    if (!token || !deviceId.value || !transfer || isReceiving.value) return;
    const file = transfer.files.find((entry) => entry.id === fileId);
    if (!file) return;
    // A consumed one-time token falls back to the session credential; the
    // backend decides whether it still authorizes this device.
    const authToken = activeDownloadTokens.get(fileId) ?? token;

    isReceiving.value = true;
    patchDownloadItem(fileId, { status: "downloading", error: undefined, loadedBytes: 0 });
    try {
      await downloadSingleFile(transfer.id, authToken, deviceId.value, file);
      patchDownloadItem(fileId, { status: "done" });
      const items = receiveDownloads.value;
      if (items.every((item) => item.status === "done")) {
        void refreshPendingReceiveTransfers();
      }
    } catch (error) {
      console.error("[mobile-session] Retry download failed:", error);
      patchDownloadItem(fileId, {
        status: "failed",
        error: error instanceof Error ? error.message : translate("mobile.receiveFailed"),
      });
    } finally {
      isReceiving.value = false;
    }
  }

  async function acceptIncomingTransfer(transferId: string) {
    const token = sessionToken.value;
    const transfer = pendingReceiveTransfer.value;
    if (!token || !deviceId.value || !transfer || transfer.id !== transferId) return;
    // Guard against double-taps while a download batch is in flight.
    if (isReceiving.value) return;

    receiveError.value = null;
    isReceiving.value = true;
    activeReceiveTransfer = transfer;
    receiveDownloads.value = transfer.files.map((file) => ({
      fileId: file.id,
      name: file.name,
      size: file.size,
      status: "queued",
      loadedBytes: 0,
    }));

    let accepted: { downloadTokens?: Array<{ fileId: string; downloadToken: string }> };
    try {
      accepted = (await acceptTransfer(transferId, token)) as {
        downloadTokens?: Array<{ fileId: string; downloadToken: string }>;
      };
    } catch (error) {
      isReceiving.value = false;
      receiveError.value = error instanceof Error ? error.message : translate("mobile.receiveFailed");
      console.error("[mobile-session] Failed to accept transfer:", error);
      return;
    }

    // The one-time download tokens travel in the Authorization header so they
    // never land in browser history or server access logs.
    activeDownloadTokens = new Map(
      (accepted.downloadTokens ?? []).map((item) => [item.fileId, item.downloadToken])
    );

    let failedCount = 0;
    for (const [index, file] of transfer.files.entries()) {
      patchDownloadItem(file.id, { status: "downloading", loadedBytes: 0 });
      try {
        const authToken = activeDownloadTokens.get(file.id);
        if (!authToken) {
          throw new Error(translate("mobile.noDownloadCredential", { name: file.name }));
        }
        await downloadSingleFile(transferId, authToken, deviceId.value, file);
        patchDownloadItem(file.id, { status: "done" });
      } catch (error) {
        failedCount += 1;
        console.error("[mobile-session] Download failed for file:", file.name, error);
        patchDownloadItem(file.id, {
          status: "failed",
          error: error instanceof Error ? error.message : translate("mobile.receiveFailed"),
        });
      }
      // Mobile browsers throttle rapid programmatic downloads; a small gap
      // plus per-file visible state keeps the queue honest instead of
      // silently dropping every file after the first (audit-30).
      if (index < transfer.files.length - 1) {
        await delay(400);
      }
    }

    isReceiving.value = false;
    if (failedCount === 0) {
      clearIncomingRequest();
      void refreshPendingReceiveTransfers();
    } else {
      receiveError.value = translate("mobile.partialReceiveFailed", { count: failedCount });
    }
  }

  async function refreshPendingReceiveTransfers() {
    const token = sessionToken.value;
    if (!token || !isApproved.value) return;

    try {
      const response = (await getPendingTransfersApi(token)) as {
        transfers?: Array<{
          id: string;
          sourceDeviceName?: string;
          totalBytes?: number;
          files?: { id: string; name: string; size: number }[];
          expiresAt?: string;
        }>;
      };
      const pending = response.transfers?.[0];
      if (!pending || pendingReceiveTransfer.value?.id === pending.id) return;

      pendingReceiveTransfer.value = {
        id: pending.id,
        sourceDeviceName: pending.sourceDeviceName || translate("mobile.unknownDevice"),
        files: pending.files ?? [],
        totalBytes: pending.totalBytes ?? 0,
        expiresAt: pending.expiresAt,
      };
      showReceiveDialog.value = true;
    } catch (error) {
      console.warn("[mobile-session] Failed to load pending transfers:", error);
    }
  }

  function markDeviceApproved() {
    if (!sessionToken.value) return;
    explicitRejection = false;
    const changed = !isApproved.value;
    isApproved.value = true;
    connectionPhase.value = socketConnected ? "connected" : "connecting";
    if (changed) void refreshPendingReceiveTransfers();
  }

  async function syncApprovalState(token = sessionToken.value): Promise<void> {
    if (!token || !deviceId.value || approvalSyncInFlight) return;
    approvalSyncInFlight = true;
    const expectedDeviceId = deviceId.value;
    try {
      const state = await getCurrentDevice(token);
      if (state.deviceId !== expectedDeviceId || expectedDeviceId !== deviceId.value) return;

      if (state.approved) {
        markDeviceApproved();
      } else {
        const wasApproved = isApproved.value;
        isApproved.value = false;
        clearIncomingRequest();
        if (wasApproved) {
          connectionPhase.value = "revoked";
        } else if (!explicitRejection) {
          connectionPhase.value = "pending_approval";
        }
      }
    } catch (error) {
      console.warn("[mobile-session] Failed to refresh authorization state:", error);
    } finally {
      approvalSyncInFlight = false;
    }
  }

  function startApprovalPolling() {
    stopApprovalPolling();
    void syncApprovalState();
    approvalPollTimer = window.setInterval(() => void syncApprovalState(), 3000);
  }

  function handleTransferRequested(msg: { payload?: Record<string, unknown> }) {
    if (!isApproved.value) return;
    const data = msg.payload as
      | {
          transferId?: string;
          id?: string;
          sourceDeviceName?: string;
          files?: { id: string; name: string; size: number }[];
          totalBytes?: number;
          expiresAt?: string;
        }
      | undefined;
    const transferId = data?.transferId ?? data?.id;
    if (!data || !transferId || pendingReceiveTransfer.value?.id === transferId) return;

    pendingReceiveTransfer.value = {
      id: transferId,
      sourceDeviceName: data.sourceDeviceName || translate("mobile.unknownDevice"),
      files: data.files ?? [],
      totalBytes: data.totalBytes ?? 0,
      expiresAt: data.expiresAt,
    };
    showReceiveDialog.value = true;
  }

  function handleDeviceApproved(msg: { payload?: Record<string, unknown> }) {
    if (msg.payload?.deviceId === deviceId.value) markDeviceApproved();
  }

  function handleDeviceRejected(msg: { payload?: Record<string, unknown> }) {
    if (msg.payload?.deviceId !== deviceId.value) return;
    const wasApproved = isApproved.value;
    explicitRejection = true;
    isApproved.value = false;
    connectionPhase.value = wasApproved ? "revoked" : "rejected";
    clearIncomingRequest();
  }

  function handleConnectionState(msg: { payload?: Record<string, unknown> }) {
    const state = msg.payload?.state;
    if (state === "open") {
      socketConnected = true;
      connectionPhase.value = isApproved.value ? "connected" : "pending_approval";
      void syncApprovalState();
      return;
    }
    if (state === "connecting") {
      socketConnected = false;
      if (isApproved.value) connectionPhase.value = "connecting";
      return;
    }
    if (state === "reconnecting" || state === "disconnected") {
      socketConnected = false;
      if (isReady.value && !explicitRejection) connectionPhase.value = "reconnecting";
    }
  }

  function handleReconnectFailed() {
    socketConnected = false;
    connectionPhase.value = "error";
    connectionError.value = translate("mobile.realtimeReconnectFailed");
  }

  function bindSocketListeners() {
    if (!listenersBound) {
      listenersBound = true;
      wsClient.on("transfer.requested", handleTransferRequested);
      wsClient.on("device.approved", handleDeviceApproved);
      wsClient.on("device.rejected", handleDeviceRejected);
      wsClient.on("connection.state", handleConnectionState);
      wsClient.on("reconnect_failed", handleReconnectFailed);
    }
    if (!visibilityListenerBound) {
      visibilityListenerBound = true;
      document.addEventListener("visibilitychange", () => {
        if (document.visibilityState !== "visible" || !isReady.value) return;
        void syncApprovalState();
        if (wsClient.getState() === "disconnected") wsClient.reconnect();
      });
    }
  }

  function connectSocket(token: string) {
    bindSocketListeners();
    socketToken = token;
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    wsClient.connect(`${protocol}//${window.location.host}/ws?token=${encodeURIComponent(token)}`);
  }

  function setSession(token: string, id: string, approved: boolean) {
    try {
      window.localStorage.setItem(MOBILE_SESSION_TOKEN_KEY, token);
    } catch {
      // Private browsing may disable storage; the in-memory token still works
      // for this tab.
    }
    const tokenChanged = socketToken !== token;
    sessionToken.value = token;
    deviceId.value = id;
    isApproved.value = approved;
    isReady.value = true;
    explicitRejection = false;
    connectionError.value = null;
    connectionPhase.value = approved ? "connecting" : "pending_approval";
    if (tokenChanged || wsClient.getState() === "idle" || wsClient.getState() === "disconnected") {
      connectSocket(token);
    }
    startApprovalPolling();
    if (approved) void refreshPendingReceiveTransfers();
  }

  async function registerCurrentBrowser(token: string): Promise<RegistrationResponse> {
    const clientId = getStableClientId();
    let storedSessionToken: string | undefined;
    try {
      storedSessionToken = window.localStorage.getItem(MOBILE_SESSION_TOKEN_KEY) ?? undefined;
    } catch {
      // Storage may be unavailable; registration without a proof still works,
      // it just requires a fresh approval.
    }
    return registerDevice({
      name: await getBrowserDeviceName(clientId),
      platform: detectPlatform(),
      deviceType: "phone",
      userAgent: navigator.userAgent,
      clientId,
      token,
      sessionToken: storedSessionToken,
    }) as Promise<RegistrationResponse>;
  }

  function reset() {
    stopApprovalPolling();
    wsClient.disconnect();
    socketToken = "";
    socketConnected = false;
    explicitRejection = false;
    sessionToken.value = null;
    deviceId.value = null;
    isApproved.value = false;
    isReady.value = false;
    connectionError.value = null;
    connectionPhase.value = "initializing";
    clearIncomingRequest();
  }

  async function initialize(token: string | null | undefined) {
    const nextPairingToken = token?.trim() ?? "";
    if (!nextPairingToken) {
      reset();
      // No token in the URL: offer the desktop PIN entry flow instead of an
      // error, so a browser that cannot scan the QR code can still connect.
      connectionPhase.value = "pin_entry";
      return;
    }
    if (nextPairingToken === pairingToken && isReady.value) {
      void syncApprovalState();
      return;
    }

    const requestVersion = ++initializationVersion;
    reset();
    pairingToken = nextPairingToken;
    connectionPhase.value = "initializing";
    try {
      await validateToken(nextPairingToken);
      const registration = await registerCurrentBrowser(nextPairingToken);
      if (requestVersion !== initializationVersion) return;
      if (!registration.sessionToken || !registration.deviceId) {
        throw new Error(translate("mobile.invalidDeviceSession"));
      }
      // Always accept the authoritative registration response. This recovers
      // automatically when the desktop database was cleared and issued a new
      // device ID instead of remaining stuck on a stale cached session.
      setSession(registration.sessionToken, registration.deviceId, registration.approved === true);
    } catch (error) {
      if (requestVersion !== initializationVersion) return;
      const message = error instanceof Error ? error.message : translate("mobile.validationFailed");
      connectionPhase.value = "error";
      connectionError.value = `${message} ${translate("mobile.scanAgain")}`;
      console.error("[mobile-session] Token validation failed:", error);
    }
  }

  async function requestAccess() {
    if (!pairingToken) return;
    explicitRejection = false;
    connectionError.value = null;
    connectionPhase.value = "initializing";
    try {
      const registration = await registerCurrentBrowser(pairingToken);
      if (!registration.sessionToken || !registration.deviceId) {
        throw new Error(translate("mobile.invalidDeviceSession"));
      }
      setSession(registration.sessionToken, registration.deviceId, registration.approved === true);
    } catch (error) {
      connectionPhase.value = "error";
      connectionError.value = error instanceof Error ? error.message : translate("mobile.requestAccessFailed");
    }
  }

  /**
   * Exchange the 6-digit code shown on the desktop for the pairing
   * capability, then continue with the exact same flow as a scanned token.
   */
  /**
   * Map raw pairing failures to human-phrased, localized messages
   * (audit-21): users saw English backend strings like "invalid pin".
   */
  function describePairingError(error: unknown): string {
    const raw = error instanceof Error ? error.message.toLowerCase() : "";
    if (raw.includes("expire")) return translate("mobile.pinExpired");
    if (raw.includes("network") || error instanceof TypeError) {
      return translate("mobile.networkError");
    }
    return translate("mobile.pinInvalid");
  }

  async function submitPin(pin: string): Promise<boolean> {
    const clean = pin.replace(/\D/g, "").slice(0, 6);
    if (clean.length !== 6) return false;
    connectionError.value = null;
    connectionPhase.value = "initializing";
    try {
      const paired = await pairWithPin(clean);
      if (!paired.token) throw new Error(translate("mobile.pinInvalid"));
      await initialize(paired.token);
      return true;
    } catch (error) {
      console.warn("[mobile-session] PIN pairing failed:", error);
      connectionPhase.value = "pin_entry";
      connectionError.value = describePairingError(error);
      return false;
    }
  }

  async function rejectIncomingTransfer(transferId: string) {
    const token = sessionToken.value;
    if (!token) return;

    receiveError.value = null;
    try {
      await rejectTransfer(transferId, token);
      clearIncomingRequest();
      void refreshPendingReceiveTransfers();
    } catch (error) {
      receiveError.value = error instanceof Error ? error.message : translate("mobile.rejectTransferFailed");
      console.error("[mobile-session] Failed to reject transfer:", error);
    }
  }

  return {
    sessionToken,
    deviceId,
    isApproved,
    isReady,
    connectionPhase,
    connectionError,
    receiveError,
    pendingReceiveTransfer,
    showReceiveDialog,
    receiveDownloads,
    isReceiving,
    initialize,
    requestAccess,
    submitPin,
    syncApprovalState,
    refreshPendingReceiveTransfers,
    acceptIncomingTransfer,
    rejectIncomingTransfer,
    retryDownloadFile,
  };
});
