// TypeScript bindings generated from the Asenix rspc API
// TODO: These will be replaced by specta-generated bindings

export interface Atom {
  atom_id: string;
  atom_type: string;
  domain: string;
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
}

export interface SearchAtomsResponse {
  atoms: Atom[];
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
      input: void;
      result: GraphResponse;
    };
  };
  mutations: never;
  subscriptions: never;
};
