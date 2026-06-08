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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecisionNode {
    /// Leaf — returns this decision.
    Leaf(PolicyDecision),
    /// Conditional branch — evaluates the condition, then chooses the node.
    Branch {
        condition: DecisionCondition,
        if_true: Box<DecisionNode>,
        if_false: Box<DecisionNode>,
    },
}

/// Condition evaluated in the decision tree.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecisionCondition {
    /// True if at least one decision is `Block`.
    AnyBlock,
    /// True if at least one decision is `OverrideTransition`.
    AnyOverride,
    /// True if all decisions are `Allow`.
    AllAllow,
    /// True if the number of decisions exceeds the threshold.
    CountExceeds(usize),
}

/// Decision tree state stored in ExtensionStorage.
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
pub struct TreeDecisionAggregator;

impl TreeDecisionAggregator {
    /// Creates a new instance.
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
        let node = ext.with_or_insert_default::<DecisionTreeState, _>(|state| {
            state.evaluation_count += 1;
            state.root.clone().unwrap_or_else(|| {
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
            })
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
