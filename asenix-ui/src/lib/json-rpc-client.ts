import type {
  HealthResponse,
  SearchAtomsInput,
  SearchAtomsResponse,
  GraphResponse
} from "./bindings";

class JsonRpcClient {
  private baseUrl: string;

  constructor(baseUrl: string = "http://localhost:3000") {
    this.baseUrl = baseUrl;
  }

  // Queries: go to /api/rspc (custom lightweight router, always live)
  private async rspcRequest<T = any>(method: string, params?: any): Promise<T> {
    const response = await fetch(`${this.baseUrl}/api/rspc`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ method, params }),
    });
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
    const data = await response.json();
    if (!data.result) throw new Error("No result in response");
    return data.result;
  }

  // Mutations: go to /rpc (JSON-RPC 2.0 — publish_atoms, retract_atom, ban_atom etc.)
  private async rpcRequest<T = any>(method: string, params?: any): Promise<T> {
    const response = await fetch(`${this.baseUrl}/rpc`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
    });
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
    const data = await response.json();
    if (data.error) throw new Error(data.error.message ?? JSON.stringify(data.error));
    return data.result;
  }

  // ── Queries ──────────────────────────────────────────────────────────────

  async health(): Promise<HealthResponse> {
    return this.rspcRequest<HealthResponse>("health");
  }

  async searchAtoms(params?: SearchAtomsInput): Promise<SearchAtomsResponse> {
    return this.rspcRequest<SearchAtomsResponse>("searchAtoms", params);
  }

  async getGraph(): Promise<GraphResponse> {
    return this.rspcRequest<GraphResponse>("getGraph");
  }

  // ── Mutations ─────────────────────────────────────────────────────────────

  async publishAtoms(params: {
    atoms: Array<{
      atom_type: string;
      statement: string;
      domain: string;
      conditions: Record<string, any>;
      metrics?: Array<any>;
    }>;
    edges?: Array<{
      source_atom_id: string;
      target_atom_id: string;
      edge_type: string;
    }>;
    agent_id?: string;
    api_token?: string;
  }): Promise<any> {
    return this.rpcRequest("publish_atoms", params);
  }

  async retractAtom(atom_id: string, agent_id: string, api_token: string, reason?: string): Promise<any> {
    return this.rpcRequest("retract_atom", { atom_id, agent_id, api_token, reason });
  }

  async banAtom(atom_id: string): Promise<any> {
    return this.rpcRequest("ban_atom", { atom_id });
  }

  async unbanAtom(atom_id: string): Promise<any> {
    return this.rpcRequest("unban_atom", { atom_id });
  }
}

export const jsonRpcClient = new JsonRpcClient();
