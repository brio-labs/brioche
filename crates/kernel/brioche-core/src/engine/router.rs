//! Pre-computed routing tables and plugin router.
//!
//! Refs: I-Core-StreamNoBranch, I-Core-PluginOrder

use crate::{
    AfterPredictionPlugin, BeforePredictionPlugin, OnErrorPlugin, OnInputPlugin,
    OnStreamEventPlugin, OnToolCallsPlugin, OnToolResultPlugin,
};

/// Pre-computed routing table that eliminates runtime capability checks.
///
/// At engine initialization, each hook vector is sorted by `(priority, name)`
/// and converted into an index route. The streaming loop iterates over these
/// vectors directly — no branching on bitmasks.
///
/// # Data Layout
/// Seven `Vec<usize>` fields (~56 bytes + route contents). Each route stores
/// indices into its matching capability vector.
///
/// # Complexity
/// Construction: O(p log p). Route iteration: O(route length).
///
/// # Panics
/// Never panics. Routes are built from enumerated indices.
///
/// Refs: I-Core-StreamNoBranch, I-Core-PluginOrder
pub struct UnifiedRoutingTable {
    /// Route on input.
    pub route_on_input: Vec<usize>,
    /// Route before prediction.
    pub route_before_prediction: Vec<usize>,
    /// Route on stream event.
    pub route_on_stream_event: Vec<usize>,
    /// Route after prediction.
    pub route_after_prediction: Vec<usize>,
    /// Route on tool calls.
    pub route_on_tool_calls: Vec<usize>,
    /// Route on tool result.
    pub route_on_tool_result: Vec<usize>,
    /// Route on error.
    pub route_on_error: Vec<usize>,
}

impl UnifiedRoutingTable {
    /// Build a routing table from already capability-separated hooks.
    ///
    /// # Complexity
    /// O(p log p) where p = total registered hook implementations.
    ///
    /// # Panics
    /// Never panics. Sorting uses total scalar/string order.
    ///
    /// Refs: I-Core-StreamNoBranch, I-Gov-TraitAtomic
    pub fn from_hooks(
        on_input: &[Box<OnInputPlugin>],
        before_prediction: &[Box<BeforePredictionPlugin>],
        on_stream_event: &[Box<OnStreamEventPlugin>],
        after_prediction: &[Box<AfterPredictionPlugin>],
        on_tool_calls: &[Box<OnToolCallsPlugin>],
        on_tool_result: &[Box<OnToolResultPlugin>],
        on_error: &[Box<OnErrorPlugin>],
    ) -> Self {
        Self {
            route_on_input: Self::collect_route(on_input),
            route_before_prediction: Self::collect_route(before_prediction),
            route_on_stream_event: Self::collect_route(on_stream_event),
            route_after_prediction: Self::collect_route(after_prediction),
            route_on_tool_calls: Self::collect_route(on_tool_calls),
            route_on_tool_result: Self::collect_route(on_tool_result),
            route_on_error: Self::collect_route(on_error),
        }
    }

    fn collect_route<T>(plugins: &[Box<T>]) -> Vec<usize>
    where
        T: OrderedHook + ?Sized,
    {
        let mut indexed: Vec<(usize, i16, &'static str)> = plugins
            .iter()
            .enumerate()
            .map(|(index, plugin)| (index, plugin.priority(), plugin.name()))
            .collect();
        indexed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(b.2)));
        indexed.into_iter().map(|(index, _, _)| index).collect()
    }
}

trait OrderedHook {
    fn name(&self) -> &'static str;
    fn priority(&self) -> i16;
}

impl OrderedHook for OnInputPlugin {
    fn name(&self) -> &'static str {
        OnInputPlugin::name(self)
    }

    fn priority(&self) -> i16 {
        OnInputPlugin::priority(self)
    }
}

impl OrderedHook for BeforePredictionPlugin {
    fn name(&self) -> &'static str {
        BeforePredictionPlugin::name(self)
    }

    fn priority(&self) -> i16 {
        BeforePredictionPlugin::priority(self)
    }
}

impl OrderedHook for OnStreamEventPlugin {
    fn name(&self) -> &'static str {
        OnStreamEventPlugin::name(self)
    }

    fn priority(&self) -> i16 {
        OnStreamEventPlugin::priority(self)
    }
}

impl OrderedHook for AfterPredictionPlugin {
    fn name(&self) -> &'static str {
        AfterPredictionPlugin::name(self)
    }

    fn priority(&self) -> i16 {
        AfterPredictionPlugin::priority(self)
    }
}

impl OrderedHook for OnToolCallsPlugin {
    fn name(&self) -> &'static str {
        OnToolCallsPlugin::name(self)
    }

    fn priority(&self) -> i16 {
        OnToolCallsPlugin::priority(self)
    }
}

impl OrderedHook for OnToolResultPlugin {
    fn name(&self) -> &'static str {
        OnToolResultPlugin::name(self)
    }

    fn priority(&self) -> i16 {
        OnToolResultPlugin::priority(self)
    }
}

impl OrderedHook for OnErrorPlugin {
    fn name(&self) -> &'static str {
        OnErrorPlugin::name(self)
    }

    fn priority(&self) -> i16 {
        OnErrorPlugin::priority(self)
    }
}

/// Routing component: owns capability-separated hooks and pre-computed routes.
///
/// # Complexity
/// Plugin lookup by pre-routed index: O(1). Route rebuild: O(p log p).
///
/// # Panics
/// Never panics. All vectors own valid trait objects.
///
/// Refs: I-Core-StreamNoBranch, I-Core-PluginOrder
pub struct PluginRouter {
    pub(crate) on_input_plugins: Vec<Box<OnInputPlugin>>,
    pub(crate) before_prediction_plugins: Vec<Box<BeforePredictionPlugin>>,
    pub(crate) on_stream_event_plugins: Vec<Box<OnStreamEventPlugin>>,
    pub(crate) after_prediction_plugins: Vec<Box<AfterPredictionPlugin>>,
    pub(crate) on_tool_calls_plugins: Vec<Box<OnToolCallsPlugin>>,
    pub(crate) on_tool_result_plugins: Vec<Box<OnToolResultPlugin>>,
    pub(crate) on_error_plugins: Vec<Box<OnErrorPlugin>>,
    pub(crate) routing_table: UnifiedRoutingTable,
}

impl PluginRouter {
    /// Rebuild routes from a flattened active mask.
    ///
    /// The mask is consumed in hook-vector order. Missing mask entries default
    /// to active so stale shell commands cannot accidentally disable new hooks.
    ///
    /// # Complexity
    /// O(p log p) where p = total active hooks.
    ///
    /// # Panics
    /// Never panics. Missing mask entries default to active.
    ///
    /// Refs: I-Gov-Rebuild-Barrier
    pub(crate) fn rebuild_routes_by_mask(&mut self, active_mask: &[bool]) {
        let mut offset = 0;
        self.routing_table = UnifiedRoutingTable {
            route_on_input: collect_active_mask(&self.on_input_plugins, active_mask, &mut offset),
            route_before_prediction: collect_active_mask(
                &self.before_prediction_plugins,
                active_mask,
                &mut offset,
            ),
            route_on_stream_event: collect_active_mask(
                &self.on_stream_event_plugins,
                active_mask,
                &mut offset,
            ),
            route_after_prediction: collect_active_mask(
                &self.after_prediction_plugins,
                active_mask,
                &mut offset,
            ),
            route_on_tool_calls: collect_active_mask(
                &self.on_tool_calls_plugins,
                active_mask,
                &mut offset,
            ),
            route_on_tool_result: collect_active_mask(
                &self.on_tool_result_plugins,
                active_mask,
                &mut offset,
            ),
            route_on_error: collect_active_mask(&self.on_error_plugins, active_mask, &mut offset),
        };
    }
}

fn collect_active_mask<T>(
    plugins: &[Box<T>],
    active_mask: &[bool],
    offset: &mut usize,
) -> Vec<usize>
where
    T: OrderedHook + ?Sized,
{
    let start = *offset;
    *offset += plugins.len();
    let mut indexed: Vec<(usize, i16, &'static str)> = plugins
        .iter()
        .enumerate()
        .filter(|(index, _)| match active_mask.get(start + *index) {
            Some(is_active) => *is_active,
            None => true,
        })
        .map(|(index, plugin)| (index, plugin.priority(), plugin.name()))
        .collect();
    indexed.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(b.2)));
    indexed.into_iter().map(|(index, _, _)| index).collect()
}
