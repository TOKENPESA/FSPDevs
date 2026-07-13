/**
 * Network helpers — every async HTTP/WS operation must have a timeout.
 */

const DEFAULT_FETCH_TIMEOUT_MS = 10_000;
const DEFAULT_WS_TIMEOUT_MS = 12_000;

/**
 * fetch() with AbortController timeout.
 * @param {string} url
 * @param {RequestInit} [init]
 * @param {number} [timeoutMs]
 */
export async function fetchWithTimeout(url, init = {}, timeoutMs = DEFAULT_FETCH_TIMEOUT_MS) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(url, { ...init, signal: controller.signal });
    return res;
  } catch (error) {
    if (error instanceof Error && error.name === "AbortError") {
      throw new Error(`Request timed out after ${timeoutMs}ms`);
    }
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * WebSocket connect with handshake timeout.
 * @param {string} url
 * @param {number} [timeoutMs]
 * @returns {Promise<WebSocket>}
 */
export function connectWebSocketWithTimeout(url, timeoutMs = DEFAULT_WS_TIMEOUT_MS) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    const timer = setTimeout(() => {
      ws.close();
      reject(new Error(`WebSocket connect timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    ws.addEventListener("open", () => {
      clearTimeout(timer);
      resolve(ws);
    });
    ws.addEventListener("error", () => {
      clearTimeout(timer);
      reject(new Error("WebSocket connection failed"));
    });
    ws.addEventListener("close", (ev) => {
      if (ws.readyState !== WebSocket.OPEN) {
        clearTimeout(timer);
        reject(new Error(`WebSocket closed before open (code ${ev.code})`));
      }
    });
  });
}

export { DEFAULT_FETCH_TIMEOUT_MS, DEFAULT_WS_TIMEOUT_MS };
