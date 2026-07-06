//! TreeDecisionAggregator — Book II §5.23.
//!
//! Conditional decision tree in `ExtensionStorage`.
//!
//! Decisions are evaluated against a stored tree of conditions,
//! allowing complex policy rules without hard-coding in the aggregator.
//!
//! Refs: I-Gov-Decision-Required

use brioche_core::{DecisionAggregator, ExtensionStorage, PluginResult, PolicyDecision};

/// Node of a decision tree.
///
/// Refs: I-Gov-Decision-Required
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
pub enum DecisionNode {
    /// Leaf — returns this decision.
    Leaf(PolicyDecision),
    /// Conditional branch — evaluates the condition, then chooses the node.
    Branch {
        /// Condition to evaluate for this branch.
        condition: DecisionCondition,
        /// Subtree to evaluate when the condition is true.
        if_true: Box<DecisionNode>,
        /// Subtree to evaluate when the condition is false.
        if_false: Box<DecisionNode>,
    },
}

impl Default for DecisionNode {
    fn default() -> Self {
        Self::Leaf(PolicyDecision::default())
    }
}

/// Condition evaluated in the decision tree.
///
/// Refs: I-Gov-Decision-Required
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
pub enum DecisionCondition {
    /// True if at least one decision is `Block`.
    #[default]
    AnyBlock,
    /// True if at least one decision is `OverrideTransition`.
    AnyOverride,
    /// True if all decisions are `Allow`.
    AllAllow,
    /// True if the number of decisions exceeds the threshold.
    CountExceeds(u64),
}

/// Decision tree state stored in ExtensionStorage.
///
/// ## Snapshot strategy
/// COW: full clone. Weight depends on tree depth and node count
/// (typically 003c 20 nodes). Recursive `Box` allocations.
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
    /// Default decision tree.
    pub root: Option<DecisionNode>,
    /// Evaluation counters.
    pub evaluation_count: u64,
}

/// Conditional decision tree aggregator.
///
/// Evaluates decisions against a tree stored in `ExtensionStorage`.
///
/// Refs: I-Gov-Decision-Required
pub struct TreeDecisionAggregator;

impl TreeDecisionAggregator {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
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

        let node = match state.root.clone() {
            Some(n) => n,
            None => {
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
            }
        };

        Ok(match evaluate_tree(&node, &decisions) {
            Some(d) => d,
            None => PolicyDecision::Allow,
        })
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
                DecisionCondition::CountExceeds(n) => (decisions.len() as u64) > *n,
            };
            if result {
                evaluate_tree(if_true, decisions)
            } else {
                evaluate_tree(if_false, decisions)
            }
        }
    }
}
