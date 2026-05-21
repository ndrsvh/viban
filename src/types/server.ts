/** Result of the `server.health` JSON-RPC method, proxied via `server_health`. */
export interface ServerHealth {
  status: string;
  version: string;
  workspace: string;
}
