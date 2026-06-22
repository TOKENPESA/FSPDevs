/// Maximum FA nodes in the lattice mesh simulation.
pub const RING_SIZE: u16 = 1024;

/// Default on-chain channel capacity (shannons) for graph edges and open_channel.
pub const DEFAULT_OPEN_CHANNEL_SHANNONS: u64 = 50_000_000_000;

/// Graph edge capacity used by MFA routing (shannons).
pub const CHANNEL_LIQUIDITY: u64 = 10_000_000_000_000;

/// Dev-only secret key marker byte for deterministic FA pubkeys.
pub const DEV_KEY_MARKER_BYTE: u8 = 0xA5;

/// JSON-RPC port base for per-FA FNN nodes in fleet layout.
pub const FNN_RPC_BASE: u16 = 18_000;

/// Fiber P2P port base for per-FA FNN nodes in fleet layout.
pub const FNN_P2P_BASE: u16 = 28_000;
