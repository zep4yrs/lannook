import { defineStore } from "pinia";
import { ref, computed } from "vue";
import type { Device } from "../types";
import { isTauri, getDevices, approveDevice as tauriApproveDevice, rejectDevice as tauriRejectDevice, forgetDevice as tauriForgetDevice } from "@/services/tauri";
import { wsClient } from "@/services/websocket";
import { useLocale } from "@/i18n";

export const useDevicesStore = defineStore("devices", () => {
  const { t } = useLocale();
  // Device records must come from the connected host. Never render example
  // devices in the phone UI because they look like real LAN targets.
  const devices = ref<Device[]>([]);
  const selectedDeviceId = ref<string | null>(null);
  const pendingApprovals = ref<Device[]>([]);
  const dismissedApprovalIds = new Set<string>();
  let latestFetchRequest = 0;
  let webSocketListenersBound = false;

  const selectedDevice = computed(() =>
    devices.value.find((d) => d.id === selectedDeviceId.value) ?? null
  );

  const onlineDevices = computed(() =>
    devices.value.filter((d) => d.online)
  );

  const currentPendingApproval = computed(() => pendingApprovals.value[0] ?? null);

  type DeviceEventPayload = Partial<Device> & {
    deviceId?: string;
    device_id?: string;
    device_type?: Device["deviceType"];
  };

  function normalizeDevice(payload: DeviceEventPayload, defaults: Partial<Device> = {}): Device | null {
    const id = payload.id ?? payload.deviceId ?? payload.device_id;
    if (!id) return null;

    return {
      id,
      name: payload.name ?? defaults.name ?? t("device.newDevice"),
      platform: payload.platform ?? defaults.platform ?? "web",
      deviceType: payload.deviceType ?? payload.device_type ?? defaults.deviceType ?? "phone",
      ip: payload.ip ?? defaults.ip ?? "",
      online: payload.online ?? defaults.online ?? true,
      approved: payload.approved ?? defaults.approved ?? false,
      trusted: payload.trusted ?? defaults.trusted ?? false,
      lastSeenAt: payload.lastSeenAt ?? defaults.lastSeenAt ?? new Date().toISOString(),
    };
  }

  function upsertDevice(next: Device): Device {
    const existing = devices.value.find((device) => device.id === next.id);
    if (existing) {
      Object.assign(existing, next);
      return existing;
    }
    devices.value.push(next);
    return next;
  }

  function clearInvalidSelection() {
    const selected = devices.value.find((device) => device.id === selectedDeviceId.value);
    if (selectedDeviceId.value && (!selected || !selected.online || !selected.approved)) {
      selectedDeviceId.value = null;
    }
  }

  /**
   * Reconcile the persisted device list with live access requests.
   *
   * The list endpoint can briefly report a newly registered phone as offline
   * while its browser is still opening its WebSocket. That snapshot must not
   * dismiss an access prompt which was already delivered over the desktop
   * control channel. An explicit disconnect, approval, rejection, or removal
   * remains the authority that closes a queued prompt.
   */
  function reconcilePendingApprovals(nextDevices: Device[]) {
    const devicesById = new Map(nextDevices.map((device) => [device.id, device]));
    const retained = pendingApprovals.value.flatMap((pendingDevice) => {
      const latest = devicesById.get(pendingDevice.id);
      if (!latest || latest.approved || dismissedApprovalIds.has(pendingDevice.id)) {
        return [];
      }
      return [{ ...pendingDevice, ...latest }];
    });
    const retainedIds = new Set(retained.map((device) => device.id));
    const pendingFromSnapshot = nextDevices.filter(
      (device) =>
        device.online &&
        !device.approved &&
        !dismissedApprovalIds.has(device.id) &&
        !retainedIds.has(device.id)
    );

    pendingApprovals.value = [...retained, ...pendingFromSnapshot];
  }

  /**
   * Fetch devices only from the desktop shell. A browser is a regular paired
   * client and must not enumerate or manage every device on the host.
   */
  async function fetchDevices() {
    if (isTauri()) {
      const requestId = ++latestFetchRequest;
      try {
        const result = (await getDevices()) as Device[];
        // An older request may finish after a newer WebSocket-triggered
        // reconciliation. Only apply the latest snapshot.
        if (requestId === latestFetchRequest) {
          setDevices(result);
        }
      } catch (err) {
        console.error("[devices] Failed to fetch devices:", err);
      }
      return;
    }

    setDevices([]);
  }

  /**
   * Approve a device via the Tauri backend.
   */
  async function approveDevice(
    id: string,
    trusted = false,
    expiryHours?: number
  ): Promise<boolean> {
    if (isTauri()) {
      try {
        const result = await tauriApproveDevice(id, trusted, expiryHours);
        if (!result.success) {
          console.error("[devices] Failed to approve device:", result.error);
          return false;
        }
      } catch (err) {
        console.error("[devices] Failed to approve device:", err);
        return false;
      }
    } else {
      return false;
    }
    // Update local state regardless (optimistic in browser, confirm in Tauri)
    const device = devices.value.find((d) => d.id === id);
    if (device) {
      device.approved = true;
      device.trusted = trusted;
    }
    dismissedApprovalIds.delete(id);
    pendingApprovals.value = pendingApprovals.value.filter((d) => d.id !== id);
    return true;
  }

  /**
   * Reject a device via the Tauri backend.
   */
  async function rejectDevice(id: string): Promise<boolean> {
    if (isTauri()) {
      try {
        const result = await tauriRejectDevice(id);
        if (!result.success) {
          console.error("[devices] Failed to reject device:", result.error);
          return false;
        }
      } catch (err) {
        console.error("[devices] Failed to reject device:", err);
        return false;
      }
    } else {
      return false;
    }
    // Remove from pending approvals and mark as not approved
    const device = devices.value.find((d) => d.id === id);
    if (device) {
      device.approved = false;
      device.trusted = false;
    }
    dismissedApprovalIds.add(id);
    pendingApprovals.value = pendingApprovals.value.filter((d) => d.id !== id);
    clearInvalidSelection();
    return true;
  }

  async function forgetDevice(id: string): Promise<boolean> {
    if (!isTauri()) return false;
    try {
      const result = await tauriForgetDevice(id);
      if (!result.success) return false;
      removeDevice(id);
      return true;
    } catch (err) {
      console.error("[devices] Failed to forget device:", err);
      return false;
    }
  }

  function selectDevice(id: string | null) {
    selectedDeviceId.value = id;
  }

  function removeDevice(id: string) {
    devices.value = devices.value.filter((d) => d.id !== id);
    pendingApprovals.value = pendingApprovals.value.filter((d) => d.id !== id);
    dismissedApprovalIds.delete(id);
    if (selectedDeviceId.value === id) {
      selectedDeviceId.value = null;
    }
  }

  function setDevices(nextDevices: Device[]) {
    devices.value = nextDevices;
    reconcilePendingApprovals(nextDevices);
    clearInvalidSelection();
  }

  /**
   * Register WebSocket event listeners for real-time device updates.
   */
  function setupWebSocketListeners() {
    if (webSocketListenersBound) return;
    webSocketListenersBound = true;

    wsClient.on("device.connected", (msg) => {
      const device = normalizeDevice(msg.payload as DeviceEventPayload, { online: true });
      if (!device) return;
      upsertDevice(device);
      // Events are advisory. Reconcile with the host so an event that arrived
      // during startup cannot leave the desktop with stale device state.
      void fetchDevices();
    });

    wsClient.on("device.disconnected", (msg) => {
      // Backend payload: { device_id } (accept camelCase/legacy shapes too)
      const data = msg.payload as {
        device_id?: string;
        deviceId?: string;
        id?: string;
      };
      const id = data.device_id ?? data.deviceId ?? data.id;
      if (!id) return;
      const device = devices.value.find((d) => d.id === id);
      if (device) {
        device.online = false;
      }
      dismissedApprovalIds.delete(id);
      // The device explicitly disconnected, so do not leave a stale global
      // access dialog open. A later reconnect produces a fresh request.
      pendingApprovals.value = pendingApprovals.value.filter((device) => device.id !== id);
      // An offline device cannot be a send target
      if (selectedDeviceId.value === id) {
        selectedDeviceId.value = null;
      }
      void fetchDevices();
    });

    wsClient.on("device.approval_required", (msg) => {
      const device = normalizeDevice(msg.payload as DeviceEventPayload, {
        approved: false,
        trusted: false,
        online: true,
      });
      if (!device) return;
      dismissedApprovalIds.delete(device.id);
      const storedDevice = upsertDevice(device);
      if (!pendingApprovals.value.find((d) => d.id === device.id)) {
        pendingApprovals.value.push(storedDevice);
      }
      clearInvalidSelection();
      void fetchDevices();
    });

    wsClient.on("device.approved", (msg) => {
      const data = msg.payload as { id?: string; deviceId?: string; device_id?: string };
      const id = data.deviceId ?? data.device_id ?? data.id;
      if (!id) return;
      const device = devices.value.find((d) => d.id === id);
      if (device) {
        device.approved = true;
      }
      dismissedApprovalIds.delete(id);
      pendingApprovals.value = pendingApprovals.value.filter((d) => d.id !== id);
      void fetchDevices();
    });

    wsClient.on("device.rejected", (msg) => {
      const data = msg.payload as { id?: string; deviceId?: string; device_id?: string };
      const id = data.deviceId ?? data.device_id ?? data.id;
      if (!id) return;
      const device = devices.value.find((d) => d.id === id);
      if (device) {
        device.approved = false;
        device.trusted = false;
      }
      dismissedApprovalIds.add(id);
      pendingApprovals.value = pendingApprovals.value.filter((d) => d.id !== id);
      clearInvalidSelection();
      void fetchDevices();
    });
  }

  return {
    // State
    devices,
    selectedDeviceId,
    pendingApprovals,
    // Computed
    selectedDevice,
    onlineDevices,
    currentPendingApproval,
    // Actions
    fetchDevices,
    approveDevice,
    rejectDevice,
    forgetDevice,
    selectDevice,
    removeDevice,
    setDevices,
    setupWebSocketListeners,
  };
});
