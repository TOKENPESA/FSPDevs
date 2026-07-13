/** Deterministic DiCoBa member UUID — must match `fiber_agent::resolve_dicoba_member_id`. */
const OID_NAMESPACE_BYTES = Uint8Array.from([
  0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

/** @param {string} name @param {Uint8Array} namespaceBytes @returns {Promise<string>} */
async function uuidV5(name, namespaceBytes) {
  const nameBytes = new TextEncoder().encode(name);
  const data = new Uint8Array(namespaceBytes.length + nameBytes.length);
  data.set(namespaceBytes);
  data.set(nameBytes, namespaceBytes.length);
  const hash = await crypto.subtle.digest("SHA-1", data);
  const bytes = new Uint8Array(hash.slice(0, 16));
  bytes[6] = (bytes[6] & 0x0f) | 0x50;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** @param {number | string} agentId @returns {Promise<string>} */
export async function dicobaMemberIdForAgent(agentId) {
  const id = Number(agentId);
  if (!Number.isInteger(id) || id < 1 || id > 1024) {
    throw new Error(`agent_id must be 1..1024, got ${agentId}`);
  }
  return uuidV5(`fspdevs-dicoba-member-fa-${id}`, OID_NAMESPACE_BYTES);
}
