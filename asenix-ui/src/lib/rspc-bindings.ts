// Manual TypeScript bindings for the Asenix rspc API
// TODO: These will be replaced by specta-generated bindings

export interface Atom {
  atom_id: string;
  atom_type: string;
  domain: string;
  statement: string;
  conditions: any;
  metrics?: any;
  lifecycle: string;
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
  };
  mutations: never;
  subscriptions: never;
};
