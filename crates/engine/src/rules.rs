//! Composable spatial rules: each rule reads the spatial index and updates entity state + events.

use spatial::SpatialIndex;
use state::{
    assignment_transition, circle_membership_transitions, zone_membership_with_dwell, EntityState,
    Event, ZoneDwell,
};
use std::collections::{BTreeSet, HashMap};

/// Per-update inputs shared across [`SpatialRule::apply`].
pub struct RuleContext<'a> {
    pub entity_id: &'a str,
    pub position: (f64, f64),
    pub at_ms: u64,
    pub zone_dwell: &'a HashMap<String, ZoneDwell>,
}

/// One step in the engine pipeline: query spatial data, emit transitions, mutate the entity slice of state.
pub trait SpatialRule: Send + Sync {
    fn apply(
        &self,
        spatial: &dyn SpatialIndex,
        ctx: &RuleContext<'_>,
        state: &mut EntityState,
        scratch: &mut BTreeSet<String>,
        out: &mut Vec<Event>,
    );
}

/// Zone enter/exit from polygon membership.
#[derive(Debug, Copy, Clone, Default)]
pub struct ZoneRule;

impl SpatialRule for ZoneRule {
    fn apply(
        &self,
        spatial: &dyn SpatialIndex,
        ctx: &RuleContext<'_>,
        state: &mut EntityState,
        scratch: &mut BTreeSet<String>,
        out: &mut Vec<Event>,
    ) {
        scratch.clear();
        spatial.zone_membership_at(ctx.position, scratch);
        zone_membership_with_dwell(
            ctx.entity_id,
            ctx.at_ms,
            scratch,
            &mut state.inside,
            &mut state.zone_enter_pending,
            &mut state.zone_exit_pending,
            ctx.zone_dwell,
            out,
        );
    }
}

/// Circle approach/recede from disk membership.
#[derive(Debug, Copy, Clone, Default)]
pub struct RadiusRule;

impl SpatialRule for RadiusRule {
    fn apply(
        &self,
        spatial: &dyn SpatialIndex,
        ctx: &RuleContext<'_>,
        state: &mut EntityState,
        scratch: &mut BTreeSet<String>,
        out: &mut Vec<Event>,
    ) {
        scratch.clear();
        spatial.circle_membership_at(ctx.position, scratch);
        out.extend(circle_membership_transitions(
            ctx.entity_id,
            &state.inside_circle,
            scratch,
            ctx.at_ms,
        ));
        std::mem::swap(&mut state.inside_circle, scratch);
    }
}

/// Primary catalog region assignment (tie-break: lexicographically smallest id).
#[derive(Debug, Copy, Clone, Default)]
pub struct CatalogRule;

impl SpatialRule for CatalogRule {
    fn apply(
        &self,
        spatial: &dyn SpatialIndex,
        ctx: &RuleContext<'_>,
        state: &mut EntityState,
        _scratch: &mut BTreeSet<String>,
        out: &mut Vec<Event>,
    ) {
        let new_catalog = spatial.primary_catalog_at(ctx.position);
        out.extend(assignment_transition(
            ctx.entity_id,
            &state.catalog_region,
            &new_catalog,
            ctx.at_ms,
        ));
        state.catalog_region = new_catalog;
    }
}

/// Default pipeline: zone, radius, catalog.
pub fn default_rules() -> Vec<Box<dyn SpatialRule>> {
    vec![
        Box::new(ZoneRule),
        Box::new(RadiusRule),
        Box::new(CatalogRule),
    ]
}
