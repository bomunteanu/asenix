use crate::domain::atom::Lifecycle;

#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleTransition {
    ProvisionalToReplicated,
    ReplicatedToCore,
    ToContested,
    ContestedToResolved,
    ToRetracted,
}

impl LifecycleTransition {
    pub fn new_lifecycle_str(&self) -> &'static str {
        match self {
            LifecycleTransition::ProvisionalToReplicated => "replicated",
            LifecycleTransition::ReplicatedToCore => "core",
            LifecycleTransition::ToContested => "contested",
            LifecycleTransition::ContestedToResolved => "resolved",
            LifecycleTransition::ToRetracted => "retracted",
        }
    }
}

/// Snapshot of atom fields needed for lifecycle evaluation.
pub struct AtomState {
    pub atom_id: String,
    pub lifecycle: Lifecycle,
    pub repl_exact: i32,
    pub ph_disagreement: f64,
    pub contradicts_edge_count: i64,
    pub replicates_edge_count: i64,
}

pub struct LifecycleEvaluator {
    pub replication_threshold_replicated: i32,
    pub replication_threshold_core: i32,
    pub disagreement_threshold: f64,
}

impl Default for LifecycleEvaluator {
    fn default() -> Self {
        Self {
            replication_threshold_replicated: 1,
            replication_threshold_core: 3,
            disagreement_threshold: 0.2,
        }
    }
}

impl LifecycleEvaluator {
    pub fn new(disagreement_threshold: f64) -> Self {
        Self {
            disagreement_threshold,
            ..Self::default()
        }
    }

    /// Evaluate what transition, if any, an atom should undergo.
    pub fn evaluate(&self, atom: &AtomState) -> Option<LifecycleTransition> {
        match atom.lifecycle {
            Lifecycle::Provisional => {
                if atom.contradicts_edge_count > 0
                    && atom.ph_disagreement > self.disagreement_threshold
                {
                    Some(LifecycleTransition::ToContested)
                } else if atom.repl_exact >= self.replication_threshold_replicated {
                    Some(LifecycleTransition::ProvisionalToReplicated)
                } else {
                    None
                }
            }
            Lifecycle::Replicated => {
                if atom.contradicts_edge_count > 0
                    && atom.ph_disagreement > self.disagreement_threshold
                {
                    Some(LifecycleTransition::ToContested)
                } else if atom.repl_exact >= self.replication_threshold_core {
                    Some(LifecycleTransition::ReplicatedToCore)
                } else {
                    None
                }
            }
            Lifecycle::Contested => {
                if atom.repl_exact >= self.replication_threshold_core
                    && atom.replicates_edge_count > atom.contradicts_edge_count * 2
                {
                    Some(LifecycleTransition::ContestedToResolved)
                } else {
                    None
                }
            }
            // Core, Resolved, Retracted are terminal or handled elsewhere
            _ => None,
        }
    }
}
