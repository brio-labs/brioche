//! TreeDecisionAggregator — Book II §5.23.
//!
//! Conditional decision tree in `ExtensionStorage`.
//!
//! Decisions are evaluated against a stored tree of conditions,
//! allowing complex policy rules without hard-coding in the aggregator.
//!
//! Refs: I-Gov-Decision-Required

use brioche_core::{DecisionAggregator, ExtensionStorage, PluginResult, PolicyDecision};

/// Nœud d'un arbre de décision.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecisionNode {
    /// Feuille — retourne cette décision.
    Leaf(PolicyDecision),
    /// Branche conditionnelle — évalue la condition, puis choisit le nœud.
    Branch {
        condition: DecisionCondition,
        if_true: Box<DecisionNode>,
        if_false: Box<DecisionNode>,
    },
}

/// Condition évaluée dans l'arbre de décision.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecisionCondition {
    /// Vrai si au moins une décision est `Block`.
    AnyBlock,
    /// Vrai si au moins une décision est `OverrideTransition`.
    AnyOverride,
    /// Vrai si toutes les décisions sont `Allow`.
    AllAllow,
    /// Vrai si le nombre de décisions dépasse le seuil.
    CountExceeds(usize),
}

/// État de l'arbre de décision stocké dans ExtensionStorage.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
#[brioche(critical_state)]
pub struct DecisionTreeState {
    /// Arbre de décision par défaut.
    pub root: Option<DecisionNode>,
    /// Compteurs d'évaluation.
    pub evaluation_count: u64,
}

/// Agrégateur d'arbre de décision conditionnel.
///
/// Évalue les décisions contre un arbre stocké dans `ExtensionStorage`.
pub struct TreeDecisionAggregator;

impl TreeDecisionAggregator {
    /// Crée une nouvelle instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for TreeDecisionAggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl DecisionAggregator for TreeDecisionAggregator {
    fn aggregate_decisions(
        &self,
        decisions: Vec<PolicyDecision>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let state = ext.get_or_insert_default::<DecisionTreeState>();
        state.evaluation_count += 1;

        let node = state.root.clone().unwrap_or_else(|| {
            // Default tree: Block > Override > Mutate > Request > Allow
            DecisionNode::Branch {
                condition: DecisionCondition::AnyBlock,
                if_true: Box::new(DecisionNode::Leaf(PolicyDecision::Block {
                    reason: "tree: blocked by policy".into(),
                })),
                if_false: Box::new(DecisionNode::Branch {
                    condition: DecisionCondition::AnyOverride,
                    if_true: Box::new(DecisionNode::Leaf(PolicyDecision::OverrideTransition(
                        vec![],
                    ))),
                    if_false: Box::new(DecisionNode::Branch {
                        condition: DecisionCondition::AllAllow,
                        if_true: Box::new(DecisionNode::Leaf(PolicyDecision::Allow)),
                        if_false: Box::new(DecisionNode::Leaf(PolicyDecision::MutateHistory(
                            vec![],
                        ))),
                    }),
                }),
            }
        });

        Ok(evaluate_tree(&node, &decisions).unwrap_or(PolicyDecision::Allow))
    }
}

fn evaluate_tree(node: &DecisionNode, decisions: &[PolicyDecision]) -> Option<PolicyDecision> {
    match node {
        DecisionNode::Leaf(decision) => Some(decision.clone()),
        DecisionNode::Branch {
            condition,
            if_true,
            if_false,
        } => {
            let result = match condition {
                DecisionCondition::AnyBlock => decisions
                    .iter()
                    .any(|d| matches!(d, PolicyDecision::Block { .. })),
                DecisionCondition::AnyOverride => decisions
                    .iter()
                    .any(|d| matches!(d, PolicyDecision::OverrideTransition(_))),
                DecisionCondition::AllAllow => {
                    decisions.iter().all(|d| matches!(d, PolicyDecision::Allow))
                }
                DecisionCondition::CountExceeds(n) => decisions.len() > *n,
            };
            if result {
                evaluate_tree(if_true, decisions)
            } else {
                evaluate_tree(if_false, decisions)
            }
        }
    }
}
