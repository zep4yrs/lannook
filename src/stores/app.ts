import { defineStore } from "pinia";
import { ref } from "vue";
import type { AppState, ToastKind } from "../types";
import {
  isTauri,
  startLocalService,
  stopLocalService,
  getLocalServiceStatus,
  getConnectionInfo,
  getConnectionQrCode,
  getAppVersion,
  regenerateConnectionToken,
  refreshPairingPin,
} from "@/services/tauri";
import type { ServiceInfo, ConnectionInfo, QrCodeData } from "@/services/tauri";
import { wsClient } from "@/services/websocket";
import { fetchHostStatus } from "@/services/api";
import { useLocale } from "@/i18n";
import { APP_NAME } from "@/config/brand";
import { needsConnectionQrRefresh } from "@/utils/connectionQr";

interface RefreshConnectionDataOptions {
  /** Keep existing connection details visible during background reconciliation. */
  silent?: boolean;
}

export const useAppStore = defineStore("app", () => {
  const { t } = useLocale();
  const serverRunning = ref<AppState["serverRunning"]>(false);
  const trayReady = ref(isTauri());
  const networkName = ref<AppState["networkName"]>("");
  const localIp = ref<AppState["localIp"]>("");
  const deviceName = ref<AppState["deviceName"]>(APP_NAME);
  const connectionToken = ref<AppState["connectionToken"]>("");
  const appVersion = ref("—");

  // New Tauri-backed state
  const serviceStatus = ref<ServiceInfo | null>(null);
  const connectionInfo = ref<ConnectionInfo | null>(null);
  const qrCode = ref<QrCodeData | null>(null);
  const selectedConnectionIp = ref("");
  const connectionInfoLoading = ref(false);
  const connectionInfoError = ref<string | null>(null);
  let connectionRefresh: Promise<void> | null = null;

  // True when the WebSocket gave up reconnecting; the UI shows a reconnect prompt.
  const connectionLost = ref(false);

  // Toast notifications (rendered by ToastHost)
  interface Toast {
    id: number;
    kind: ToastKind;
    title: string;
    description?: string;
  }
  const toasts = ref<Toast[]>([]);
  let toastId = 0;

  // Mobile device-selector sheet visibility
  const deviceSheetOpen = ref(false);

  /**
   * Initialize the app store. In Tauri mode, fetches real service status
   * and connection info from the backend. A hosted browser only receives the
   * public service status; device management remains in the desktop app.
   */
  async function initialize() {
    if (isTauri()) {
      try {
        const status = await getLocalServiceStatus();
        serviceStatus.value = status;
        serverRunning.value = status.status === "running";
      } catch (err) {
        console.error("[app] Failed to get service status:", err);
      }

      await refreshConnectionData();
      try {
        appVersion.value = (await getAppVersion()).version;
      } catch (err) {
        console.error("[app] Failed to get application version:", err);
      }
      return;
    }

    try {
      const status = await fetchHostStatus();
      serverRunning.value = status.status === "running";
      localIp.value = status.localIp ?? window.location.hostname;
      networkName.value = status.networkName ?? networkName.value;
      deviceName.value = status.name ?? deviceName.value;
      appVersion.value = status.version ?? appVersion.value;
    } catch (err) {
      console.error("[app] Failed to load hosted service status:", err);
      return;
    }

  }

  /**
   * Start the local file-sharing service via Tauri backend.
   */
  async function startServer() {
    if (isTauri()) {
      try {
        const result = await startLocalService();
        if (!result.success) throw new Error(result.error ?? t("app.startServiceFailed"));
        const status = await getLocalServiceStatus();
        serviceStatus.value = status;
        serverRunning.value = status.status === "running";

        await refreshConnectionData();
      } catch (err) {
        console.error("[app] Failed to start server:", err);
        try {
          const status = await getLocalServiceStatus();
          serviceStatus.value = status;
          serverRunning.value = status.status === "running";
        } catch {
          serverRunning.value = false;
        }
        pushToast("error", t("app.startFailed"), err instanceof Error ? err.message : t("app.startServiceFailed"));
      }
    }
  }

  /**
   * Stop the local file-sharing service via Tauri backend.
   */
  async function stopServer() {
    if (isTauri()) {
      try {
        const result = await stopLocalService();
        if (!result.success) throw new Error(result.error ?? t("app.stopServiceFailed"));
        const status = await getLocalServiceStatus();
        serviceStatus.value = status;
        serverRunning.value = status.status === "running";
        qrCode.value = null;
      } catch (err) {
        console.error("[app] Failed to stop server:", err);
        pushToast("error", t("app.stopFailed"), err instanceof Error ? err.message : t("app.stopServiceFailed"));
      }
    }
  }

  /**
   * Toggle server on/off (used by UI toggle).
   */
  async function toggleServer() {
    if (serverRunning.value) {
      await stopServer();
    } else {
      await startServer();
    }
  }

  /**
   * Refresh the QR code data from the backend.
   */
  async function refreshQrCode(ip = selectedConnectionIp.value || undefined) {
    if (isTauri()) {
      try {
        const data = await getConnectionQrCode(ip);
        if (!qrCode.value || qrCode.value.url !== data.url) qrCode.value = data;
        if (ip) selectedConnectionIp.value = ip;
      } catch (err) {
        console.error("[app] Failed to refresh QR code:", err);
      }
    }
  }

  /** Refresh connection metadata and QR as one snapshot for the panel. */
  async function refreshConnectionData(options: RefreshConnectionDataOptions = {}) {
    if (!isTauri()) return;
    if (connectionRefresh) return connectionRefresh;

    // Do not hide an already-scanned QR code while the panel performs its
    // periodic adapter check. The code is rebuilt only when its address or
    // pairing token actually changes.
    const showLoading = !options.silent && !qrCode.value;
    if (showLoading) connectionInfoLoading.value = true;
    if (!options.silent) connectionInfoError.value = null;

    connectionRefresh = (async () => {
      try {
        const info = await getConnectionInfo();
        const nextIp = info.addresses.some((entry) => entry.ip === selectedConnectionIp.value)
          ? selectedConnectionIp.value
          : info.ip;
        const qr = needsConnectionQrRefresh(qrCode.value, nextIp, info.port, info.token)
          ? await getConnectionQrCode(nextIp)
          : qrCode.value;
        if (!qr) throw new Error("Unable to generate connection QR code.");

        connectionInfo.value = info;
        if (!qrCode.value || qrCode.value.url !== qr.url) qrCode.value = qr;
        selectedConnectionIp.value = nextIp;
        localIp.value = info.ip;
        connectionToken.value = info.token;
        networkName.value = info.networkName;
        if (info.deviceName) deviceName.value = info.deviceName;
      } catch (error) {
        if (!options.silent) {
          connectionInfoError.value =
            error instanceof Error ? error.message : "Unable to load connection information.";
        }
        console.error("[app] Failed to refresh connection information:", error);
      } finally {
        if (showLoading) connectionInfoLoading.value = false;
      }
    })().finally(() => {
      connectionRefresh = null;
    });

    return connectionRefresh;
  }

  /** Refresh the 6-digit pairing PIN shown in the connect panel. */
  async function refreshPairingPinCode(): Promise<string | null> {
    try {
      const pin = await refreshPairingPin();
      if (connectionInfo.value) {
        connectionInfo.value = { ...connectionInfo.value, pin };
      }
      return pin;
    } catch (error) {
      console.error("[app] Failed to refresh pairing PIN:", error);
      return null;
    }
  }

  /** Switch the QR code to another currently active LAN path (for example a
   * Windows mobile-hotspot adapter) without changing the listening socket. */
  async function selectConnectionAddress(ip: string) {
    const address = connectionInfo.value?.addresses.find((entry) => entry.ip === ip);
    if (!address) throw new Error("selected_address_inactive");
    connectionInfoLoading.value = true;
    try {
      const qr = await getConnectionQrCode(ip);
      if (!qrCode.value || qrCode.value.url !== qr.url) qrCode.value = qr;
      selectedConnectionIp.value = ip;
    } catch (error) {
      throw error;
    } finally {
      connectionInfoLoading.value = false;
    }
  }

  /**
   * Connect the WebSocket client to the local service.
   * Uses connection info (IP, port, token) to build the WS URL.
   */
  function connectWebSocket() {
    if (isTauri() && connectionInfo.value) {
      const { port, controlToken } = connectionInfo.value;
      // Desktop control traffic stays on loopback. LAN adapter selection is
      // only for phones and must not break the desktop's own live updates.
      const wsUrl = `ws://127.0.0.1:${port}/ws?token=${controlToken}`;
      wsClient.connect(wsUrl);
    }
  }

  /**
   * Surface WebSocket connection loss to the UI. When reconnect attempts are
   * exhausted we flag connectionLost and notify the user; a successful
   * reconnect clears the flag. Idempotent — safe to call once at startup.
   */
  let connectionMonitorSetup = false;
  function setupConnectionMonitor() {
    if (connectionMonitorSetup) return;
    connectionMonitorSetup = true;

    wsClient.on("reconnect_failed", () => {
      connectionLost.value = true;
      pushToast("error", t("app.connectLost"), t("app.connectLostDescription"));
    });
    wsClient.on("connected", () => {
      if (connectionLost.value) {
        connectionLost.value = false;
        pushToast("success", t("app.reconnected"), t("app.reconnectedDescription"));
      }
    });
  }

  /** Manually retry the WebSocket connection (used by the reconnect prompt). */
  function manualReconnect() {
    wsClient.reconnect();
  }

  function setDeviceName(name: string) {
    deviceName.value = name;
  }

  /**
   * Regenerate the connection token. In Tauri mode the backend generates
   * the new token (so connecting devices use the real one) and the QR code
   * is refreshed to match. Browser mode falls back to a local random token.
   */
  async function regenerateToken() {
    if (isTauri()) {
      try {
        const newToken = await regenerateConnectionToken();
        connectionToken.value = newToken;
        if (connectionInfo.value) {
          connectionInfo.value = { ...connectionInfo.value, token: newToken };
        }
        await refreshConnectionData();
        return;
      } catch (err) {
        console.error("[app] Failed to regenerate token via backend:", err);
      }
    }
    pushToast("info", t("app.useDesktopApp"), t("app.useDesktopAppDescription"));
  }

  /**
   * Show a toast notification. Auto-dismisses after 3 seconds.
   */
  function pushToast(kind: ToastKind, title: string, description?: string) {
    const id = ++toastId;
    toasts.value.push({ id, kind, title, description });
    setTimeout(() => dismissToast(id), 3000);
  }

  /**
   * Remove a toast by id (click-to-dismiss or auto timeout).
   */
  function dismissToast(id: number) {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }

  function openDeviceSheet() {
    deviceSheetOpen.value = true;
  }

  function closeDeviceSheet() {
    deviceSheetOpen.value = false;
  }

  return {
    // State
    serverRunning,
    trayReady,
    networkName,
    localIp,
    deviceName,
    connectionToken,
    appVersion,
    serviceStatus,
    connectionInfo,
    qrCode,
    selectedConnectionIp,
    connectionInfoLoading,
    connectionInfoError,
    toasts,
    deviceSheetOpen,
    connectionLost,
    // Actions
    initialize,
    startServer,
    stopServer,
    toggleServer,
    refreshQrCode,
    refreshConnectionData,
    selectConnectionAddress,
    connectWebSocket,
    setupConnectionMonitor,
    manualReconnect,
    setDeviceName,
    regenerateToken,
    refreshPairingPinCode,
    pushToast,
    dismissToast,
    openDeviceSheet,
    closeDeviceSheet,
  };
});
