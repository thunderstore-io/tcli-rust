mod method;
mod proto;

/// The daemon's entrypoint. This is a psuedo event loop which does the following in step:
/// 1. Read JSON-RPC input(s) from stdin.
/// 2. Route each input.
/// 3. Serialize the output and write to stdout.
pub async fn start() {}
