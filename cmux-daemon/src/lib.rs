//! cmux-daemon library — exposes the daemon's internals (session
//! management, interactive server, JSON-RPC server) so integration tests
//! and external callers can reuse them.

pub mod jsonrpc;
pub mod rpc_server;
pub mod server;
pub mod session_manager;
