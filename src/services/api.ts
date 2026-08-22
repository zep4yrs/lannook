// CONFIG_REQUIRED: VITE_LANNOOK_API_BASE_URL
// Leave unset for the normal paired-LAN flow, which uses the page's current
// origin. VITE_LYNQO_API_BASE_URL is read only as a legacy fallback.
const configuredApiBase =
  import.meta.env.VITE_LANNOOK_API_BASE_URL?.trim() ||
  import.meta.env.VITE_LYNQO_API_BASE_URL?.trim();
const runtimeOrigin = typeof window === "undefined" ? "http://localhost" : window.location.origin;
const BASE = configuredApiBase?.replace(/\/+$/, "") || runtimeOrigin;

export interface ApiError {
  code: string;
  message: string;
}

interface ApiErrorEnvelope {
  error?: ApiError | string;
}

export function getApiErrorMessage(payload: unknown, status: number): string {
  if (payload && typeof payload === "object" && "error" in payload) {
    const error = (payload as ApiErrorEnvelope).error;
    if (typeof error === "string" && error.trim()) return error;
    if (error && typeof error === "object" && typeof error.message === "string") {
      return error.message;
    }
  }
  return `HTTP ${status}`;
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...options,
    headers: { "Content-Type": "application/json", ...options?.headers },
  });
  if (!res.ok) {
    const payload: unknown = await res.json().catch(() => null);
    throw new Error(getApiErrorMessage(payload, res.status));
  }
  return res.json();
}

export function validateToken(token: string) {
  return request(`/api/connect?token=${encodeURIComponent(token)}`);
}

export interface PairResult {
  valid: boolean;
  token: string;
  deviceName: string;
  networkName: string;
}

/** Exchange a desktop-shown 6-digit PIN for the pairing capability. */
export function pairWithPin(pin: string) {
  return request<PairResult>("/api/pair", {
    method: "POST",
    body: JSON.stringify({ pin }),
  });
}

export interface HostStatus {
  localIp: string;
  name: string;
  networkName: string;
  port: number;
  status: string;
  version: string;
}

export function fetchHostStatus() {
  return request<HostStatus>(`/api/status`);
}

export function registerDevice(data: {
  name: string;
  platform: string;
  deviceType: string;
  userAgent: string;
  clientId: string;
  token: string;
  sessionToken?: string;
}) {
  return request("/api/devices/register", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export function getCurrentDevice(sessionToken: string) {
  return request<{ deviceId: string; approved: boolean }>("/api/devices/me", {
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function getAvailableDevices(sessionToken: string) {
  return request<{ devices: unknown[] }>("/api/devices", {
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function createTransfer(
  files: { name: string; size: number; mimeType?: string }[],
  sessionToken: string
) {
  return request("/api/transfers", {
    method: "POST",
    body: JSON.stringify({ files, sessionToken }),
  });
}

export function getCompletedChunks(
  transferId: string,
  sessionToken: string,
  fileId?: string
): Promise<number[]> {
  const query = fileId ? `?fileId=${encodeURIComponent(fileId)}` : "";
  return request<{ transferId: string; completedChunks: number[] }>(
    `/api/transfers/${transferId}/chunks${query}`,
    {
      headers: { Authorization: `Bearer ${sessionToken}` },
    }
  ).then((res) => res.completedChunks ?? []);
}

export function completeTransfer(transferId: string, sessionToken: string) {
  return request(`/api/transfers/${transferId}/complete`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function cancelTransferApi(transferId: string, sessionToken: string) {
  return request(`/api/transfers/${transferId}/cancel`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function getMyTransfers(sessionToken: string) {
  return request<{ transfers: unknown[] }>("/api/transfers", {
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

// Upload a single chunk as raw bytes
export async function uploadChunk(
  transferId: string,
  chunkIndex: number,
  data: ArrayBuffer,
  sessionToken: string,
  fileId?: string
): Promise<void> {
  const query = fileId ? `?fileId=${encodeURIComponent(fileId)}` : "";
  const res = await fetch(
    `${BASE}/api/transfers/${transferId}/chunks/${chunkIndex}${query}`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${sessionToken}`,
        "Content-Type": "application/octet-stream",
      },
      body: data,
    }
  );
  if (!res.ok) {
    const payload: unknown = await res.json().catch(() => null);
    throw new Error(getApiErrorMessage(payload, res.status));
  }
}

/**
 * Upload a chunk while exposing browser-native byte progress. `fetch` does
 * not provide request-body progress events, so mobile UIs would otherwise
 * update only after an entire chunk completed.
 */
export function uploadChunkWithProgress(
  transferId: string,
  chunkIndex: number,
  data: ArrayBuffer,
  sessionToken: string,
  onUploadProgress: (loadedBytes: number, totalBytes: number) => void,
  fileId?: string
): Promise<void> {
  const query = fileId ? `?fileId=${encodeURIComponent(fileId)}` : "";
  const url = `${BASE}/api/transfers/${transferId}/chunks/${chunkIndex}${query}`;

  return new Promise((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open("POST", url);
    request.setRequestHeader("Authorization", `Bearer ${sessionToken}`);
    request.setRequestHeader("Content-Type", "application/octet-stream");

    request.upload.onprogress = (event) => {
      if (event.lengthComputable) {
        onUploadProgress(event.loaded, event.total);
      }
    };
    request.onerror = () => reject(new Error("Network error while uploading chunk"));
    request.onabort = () => reject(new Error("Upload cancelled"));
    request.onload = () => {
      if (request.status >= 200 && request.status < 300) {
        resolve();
        return;
      }

      let message = `HTTP ${request.status}`;
      try {
        message = getApiErrorMessage(JSON.parse(request.responseText), request.status);
      } catch {
        // A non-JSON error response still has a useful HTTP status.
      }
      reject(new Error(message));
    };
    request.send(data);
  });
}

// Phase 3: Bidirectional transfers, relay, and pause/resume

export function acceptTransfer(transferId: string, sessionToken: string) {
  return request(`/api/transfers/${transferId}/accept`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function rejectTransfer(transferId: string, sessionToken: string) {
  return request(`/api/transfers/${transferId}/reject`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function pauseTransferApi(transferId: string, sessionToken: string) {
  return request(`/api/transfers/${transferId}/pause`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function resumeTransferApi(transferId: string, sessionToken: string) {
  return request(`/api/transfers/${transferId}/resume`, {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function getResumeInfo(transferId: string, sessionToken: string) {
  return request<{ completedChunks: number[]; totalChunks: number }>(
    `/api/transfers/${transferId}/resume-info`,
    {
      headers: { Authorization: `Bearer ${sessionToken}` },
    }
  );
}

export function getPendingTransfersApi(sessionToken: string) {
  return request<{ transfers: unknown[] }>(`/api/devices/me/transfers/pending`, {
    headers: { Authorization: `Bearer ${sessionToken}` },
  });
}

export function createRelay(
  files: { name: string; size: number; mimeType?: string }[],
  sourceDeviceId: string,
  targetDeviceId: string,
  sessionToken: string
) {
  return request("/api/transfers/relay", {
    method: "POST",
    headers: { Authorization: `Bearer ${sessionToken}` },
    body: JSON.stringify({ files, sourceDeviceId, targetDeviceId, sessionToken }),
  });
}

/**
 * Download a received file. The one-time download token travels in the
 * Authorization header instead of the URL so it never lands in browser
 * history or server access logs. The returned Blob should be revoked by
 * the caller after saving.
 */
export async function downloadFile(
  transferId: string,
  fileId: string,
  token: string,
  deviceId: string
): Promise<Blob> {
  const url = `${BASE}/api/transfers/${transferId}/files/${fileId}/download?deviceId=${encodeURIComponent(deviceId)}`;
  const res = await fetch(url, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!res.ok) {
    const payload: unknown = await res.json().catch(() => null);
    throw new Error(getApiErrorMessage(payload, res.status));
  }
  return res.blob();
}

export interface DownloadProgressInfo {
  loadedBytes: number;
  totalBytes: number;
}

/**
 * Stream a received file to a Blob while reporting byte progress
 * (audit-30). The legacy downloadFile buffered silently, so multi-GB
 * transfers left the phone UI frozen with no feedback. Falls back to the
 * plain blob path when the response has no readable stream.
 */
export async function downloadFileWithProgress(
  transferId: string,
  fileId: string,
  token: string,
  deviceId: string,
  onProgress: (info: DownloadProgressInfo) => void,
  signal?: AbortSignal
): Promise<Blob> {
  const url = `${BASE}/api/transfers/${transferId}/files/${fileId}/download?deviceId=${encodeURIComponent(deviceId)}`;
  const res = await fetch(url, {
    headers: { Authorization: `Bearer ${token}` },
    signal,
  });
  if (!res.ok) {
    const payload: unknown = await res.json().catch(() => null);
    throw new Error(getApiErrorMessage(payload, res.status));
  }

  const totalBytes = Number(res.headers.get("Content-Length") ?? 0);
  const body = res.body;
  if (!body || typeof body.getReader !== "function") {
    const blob = await res.blob();
    onProgress({ loadedBytes: blob.size, totalBytes: blob.size });
    return blob;
  }

  const reader = body.getReader();
  const chunks: BlobPart[] = [];
  let loadedBytes = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (value) {
      chunks.push(value as unknown as BlobPart);
      loadedBytes += value.byteLength;
      onProgress({
        loadedBytes,
        totalBytes: totalBytes > 0 ? totalBytes : loadedBytes,
      });
    }
  }
  return new Blob(chunks);
}
