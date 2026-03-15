function adminAuthHeader(): Record<string, string> {
  try {
    const stored = localStorage.getItem('asenix-admin-auth')
    const token = stored ? JSON.parse(stored)?.state?.token : null
    return token ? { Authorization: `Bearer ${token}` } : {}
  } catch {
    return {}
  }
}

import type {
  HealthResponse,
  SearchAtomsInput,
  SearchAtomsResponse,
  GraphResponse,
  GraphWithEmbeddingsResponse,
  GraphInput,
  Project,
  ProjectFile,
  ListProjectsResponse,
  CreateProjectInput,
} from "./bindings";

class JsonRpcClient {
  private baseUrl: string;

  constructor(baseUrl: string = "") {
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

  async getGraph(params?: GraphInput): Promise<GraphResponse> {
    return this.rspcRequest<GraphResponse>("getGraph", params);
  }

  async getGraphWithEmbeddings(params?: GraphInput): Promise<GraphWithEmbeddingsResponse> {
    return this.rspcRequest<GraphWithEmbeddingsResponse>("getGraphWithEmbeddings", params);
  }

  // ── Projects ─────────────────────────────────────────────────────────────

  async listProjects(): Promise<ListProjectsResponse> {
    return this.rspcRequest<ListProjectsResponse>("listProjects");
  }

  async getProject(project_id: string): Promise<Project> {
    return this.rspcRequest<Project>("getProject", { project_id });
  }

  async createProject(params: CreateProjectInput): Promise<Project> {
    return this.rspcRequest<Project>("createProject", params);
  }

  async updateProject(params: { project_id: string } & CreateProjectInput): Promise<Project> {
    return this.rspcRequest<Project>("updateProject", params);
  }

  async deleteProject(project_id: string): Promise<void> {
    return this.rspcRequest("deleteProject", { project_id });
  }

  // ── Project layer REST endpoints ─────────────────────────────────────────

  async createProjectRest(params: CreateProjectInput): Promise<Project> {
    const res = await fetch(`${this.baseUrl}/projects`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...adminAuthHeader() },
      body: JSON.stringify(params),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
    return res.json();
  }

  async deleteProjectRest(project_id: string): Promise<void> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}`, {
      method: 'DELETE',
      headers: adminAuthHeader(),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
  }

  async getProtocol(project_id: string): Promise<string | null> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/protocol`);
    if (res.status === 204 || res.status === 404) return null;
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
    return res.text();
  }

  async setProtocol(project_id: string, protocol: string): Promise<void> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/protocol`, {
      method: 'PUT',
      headers: { 'Content-Type': 'text/plain', ...adminAuthHeader() },
      body: protocol,
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
  }

  async getRequirements(project_id: string): Promise<any[]> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/requirements`);
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
    // Backend returns { project_id, requirements: [...] }
    const data = await res.json();
    return data.requirements ?? data;
  }

  async setRequirements(project_id: string, requirements: any[]): Promise<any[]> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/requirements`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...adminAuthHeader() },
      // Backend expects { requirements: [...] }
      body: JSON.stringify({ requirements }),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
    const data = await res.json();
    return data.requirements ?? data;
  }

  async getSeedBounty(project_id: string): Promise<any | null> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/seed-bounty`);
    if (res.status === 204 || res.status === 404) return null;
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
    // Backend returns { project_id, seed_bounty: value }
    const data = await res.json();
    return data.seed_bounty ?? data;
  }

  async setSeedBounty(project_id: string, seedBounty: any): Promise<void> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/seed-bounty`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...adminAuthHeader() },
      // Backend expects { seed_bounty: value }
      body: JSON.stringify({ seed_bounty: seedBounty }),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
  }

  async listProjectFiles(project_id: string): Promise<ProjectFile[]> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/files`);
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
    // Backend returns { project_id, files: [...] }
    const data = await res.json();
    return data.files ?? data;
  }

  async uploadProjectFile(project_id: string, filename: string, data: Blob, contentType?: string): Promise<void> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/files/${encodeURIComponent(filename)}`, {
      method: 'PUT',
      headers: { 'Content-Type': contentType ?? data.type ?? 'application/octet-stream', ...adminAuthHeader() },
      body: data,
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
  }

  async deleteProjectFile(project_id: string, filename: string): Promise<void> {
    const res = await fetch(`${this.baseUrl}/projects/${project_id}/files/${encodeURIComponent(filename)}`, {
      method: 'DELETE',
      headers: adminAuthHeader(),
    });
    if (!res.ok) {
      const body = await res.json().catch(() => ({}));
      throw new Error((body as { error?: string }).error ?? `HTTP ${res.status}`);
    }
  }

  projectFileUrl(project_id: string, filename: string): string {
    return `${this.baseUrl}/projects/${project_id}/files/${encodeURIComponent(filename)}`;
  }

  // ── Mutations ─────────────────────────────────────────────────────────────

  async publishAtoms(params: {
    atoms: Array<{
      atom_type: string;
      statement: string;
      domain: string;
      project_id?: string;
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
    const response = await fetch(`${this.baseUrl}/api/rspc`, {
      method: "POST",
      headers: { "Content-Type": "application/json", ...adminAuthHeader() },
      body: JSON.stringify({ method: "ban_atom", params: { atom_id } }),
    });
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
    const data = await response.json();
    if (!data.result) throw new Error("No result in response");
    return data.result;
  }

  async unbanAtom(atom_id: string): Promise<any> {
    const response = await fetch(`${this.baseUrl}/api/rspc`, {
      method: "POST",
      headers: { "Content-Type": "application/json", ...adminAuthHeader() },
      body: JSON.stringify({ method: "unban_atom", params: { atom_id } }),
    });
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
    const data = await response.json();
    if (!data.result) throw new Error("No result in response");
    return data.result;
  }

  // ── Review ────────────────────────────────────────────────────────────────

  async getReviewQueue(params?: { domain?: string; project_id?: string; limit?: number; offset?: number }): Promise<{ items: any[]; total: number }> {
    const qs = new URLSearchParams()
    if (params?.domain) qs.set('domain', params.domain)
    if (params?.project_id) qs.set('project_id', params.project_id)
    if (params?.limit !== undefined) qs.set('limit', String(params.limit))
    if (params?.offset !== undefined) qs.set('offset', String(params.offset))
    const response = await fetch(`${this.baseUrl}/review?${qs}`, {
      headers: { 'Content-Type': 'application/json', ...adminAuthHeader() },
    })
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`)
    return response.json()
  }

  async reviewAtom(atomId: string, action: 'approve' | 'reject', reason?: string): Promise<any> {
    const response = await fetch(`${this.baseUrl}/review/${atomId}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...adminAuthHeader() },
      body: JSON.stringify({ action, reason }),
    })
    if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`)
    return response.json()
  }
}

export const jsonRpcClient = new JsonRpcClient();
