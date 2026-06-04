//! Pre-computed routing tables and plugin router.
//!
//! Refs: I-Core-StreamNoBranch, I-Core-PluginOrder

use crate::{BriochePlugin, PluginCapabilities};

/// Pre-computed routing table that eliminates runtime capability checks.
///
/// At engine initialization, plugins are sorted by `(priority, name)` and
/// their indices are collected into per-capability vectors. The streaming
/// loop iterates over these vectors directly — no branching on bitmasks.
///
/// Refs: I-Core-StreamNoBranch, I-Core-PluginOrder
pub struct UnifiedRoutingTable {
    pub route_on_input: Vec<usize>,
    pub route_before_prediction: Vec<usize>,
    pub route_on_stream_event: Vec<usize>,
    pub route_after_prediction: Vec<usize>,
    pub route_on_tool_calls: Vec<usize>,
    pub route_on_tool_result: Vec<usize>,
    pub route_on_error: Vec<usize>,
}

impl UnifiedRoutingTable {
    /// Build a routing table from all plugins.
    ///
    /// Convenience wrapper over `from_plugins_filtered` with all indices active.
    pub fn from_plugins(plugins: &[Box<dyn BriochePlugin>]) -> Self {
        let all_indices: Vec<usize> = (0..plugins.len()).collect();
        Self::from_plugins_filtered(plugins, &all_indices)
    }

    /// Build a routing table from a subset of plugins.
    ///
    /// `active_indices` contains indices into `plugins` that should be
    /// included in the routing table. Used by `rebuild_routes` during
    /// quarantine events.
    ///
    /// Complexity: O(p log p) where p = number of active plugins.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    pub fn from_plugins_filtered(
        plugins: &[Box<dyn BriochePlugin>],
        active_indices: &[usize],
    ) -> Self {
        let mut indexed: Vec<(usize, i16, &'static str)> = active_indices
            .iter()
            .map(|&i| (i, plugins[i].priority(), plugins[i].name()))
            .collect();
        // Total order: priority ascending, then name lexicographically.
        indexed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(b.2)));

        Self {
            route_on_input: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_INPUT)
            }),
            route_before_prediction: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::BEFORE_PREDICTION)
            }),
            route_on_stream_event: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_STREAM_EVENT)
            }),
            route_after_prediction: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::AFTER_PREDICTION)
            }),
            route_on_tool_calls: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_TOOL_CALLS)
            }),
            route_on_tool_result: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_TOOL_RESULT)
            }),
            route_on_error: Self::collect_route(&indexed, plugins, |c| {
                c.contains(PluginCapabilities::ON_ERROR)
            }),
        }
    }

    fn collect_route(
        sorted: &[(usize, i16, &'static str)],
        plugins: &[Box<dyn BriochePlugin>],
        has_cap: impl Fn(PluginCapabilities) -> bool,
    ) -> Vec<usize> {
        sorted
            .iter()
            .filter(|(i, _, _)| has_cap(plugins[*i].capabilities()))
            .map(|(i, _, _)| *i)
            .collect()
    }
}

/// Routing component: owns plugins and the pre-computed dispatch table.
///
/// All plugin iteration happens through this component. The engine
/// delegates routing queries but never mutates the plugin vector directly.
///
/// Refs: I-Core-StreamNoBranch, I-Core-PluginOrder
pub struct PluginRouter {
    pub(crate) plugins: Vec<Box<dyn BriochePlugin>>,
    pub(crate) routing_table: UnifiedRoutingTable,
}
