// TypeScript bindings generated from the Asenix rspc API
// TODO: These will be replaced by specta-generated bindings

export interface Atom {
  atom_id: string;
  atom_type: string;
  domain: string;
  project_id?: string;
  statement: string;
  conditions: any;
  metrics?: any;
  lifecycle: string;
  ph_attraction: number;
  ph_repulsion: number;
  ph_novelty: number;
  ph_disagreement: number;
  ban_flag: boolean;
  retracted: boolean;
  created_at: string;
}

export interface Project {
  project_id: string;
  name: string;
  slug: string;
  description?: string;
  created_at: string;
}

export interface ListProjectsResponse {
  projects: Project[];
}

export interface CreateProjectInput {
  name: string;
  slug: string;
  description?: string;
}

export interface UpdateProjectInput {
  project_id: string;
  name: string;
  slug: string;
  description?: string;
}

export interface ProjectFile {
  filename: string;
  size_bytes: number;
  content_type?: string;
  uploaded_at: string;
}

export interface SearchAtomsResponse {
  atoms: Atom[];
  total: number;
}

export interface HealthResponse {
  status: string;
  timestamp: string;
}

export interface SearchAtomsInput {
  domain?: string;
  type?: string;
  lifecycle?: string;
  query?: string;
  project_id?: string;
  limit?: number;
  offset?: number;
}

export interface Edge {
  source_id: string;
  target_id: string;
  edge_type: string;
  repl_type?: string;
  created_at: string;
}

export interface GraphResponse {
  atoms: Atom[];
  edges: Edge[];
}

export interface GraphWithEmbeddingsResponse {
  atoms: Atom[];
  edges: Edge[];
  embeddings: Record<string, number[]>;
}

export interface GraphInput {
  project_id?: string;
}

// rspc procedure types
export type Procedures = {
  queries: {
    health: {
      key: "health";
      input: void;
      result: HealthResponse;
    };
    searchAtoms: {
      key: "searchAtoms";
      input: SearchAtomsInput;
      result: SearchAtomsResponse;
    };
    getGraph: {
      key: "getGraph";
      input: GraphInput;
      result: GraphResponse;
    };
    listProjects: {
      key: "listProjects";
      input: void;
      result: ListProjectsResponse;
    };
    getProject: {
      key: "getProject";
      input: { project_id: string };
      result: Project;
    };
    createProject: {
      key: "createProject";
      input: CreateProjectInput;
      result: Project;
    };
  };
  mutations: never;
  subscriptions: never;
};
