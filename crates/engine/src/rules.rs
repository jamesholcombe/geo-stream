//! Composable spatial rules: each rule reads the spatial index and updates entity state + events.

use spatial::NaiveSpatialIndex;
use state::{
    assignment_transition, corridor_membership_transitions, membership_transitions,
    radius_membership_transitions, EntityState, Event,
};
use std::collections::BTreeSet;

/// Per-update inputs shared across [`SpatialRule::apply`].
pub struct RuleContext<'a> {
    pub entity_id: &'a str,
    pub position: (f64, f64),
    pub at_ms: u64,
}

/// One step in the engine pipeline: query spatial data, emit transitions, mutate the entity slice of state.
pub trait SpatialRule: Send + Sync {
    fn apply(
        &self,
        spatial: &NaiveSpatialIndex,
        ctx: &RuleContext<'_>,
        state: &mut EntityState,
        scratch: &mut BTreeSet<String>,
        out: &mut Vec<Event>,
    );
}

/// Geofence enter/exit from polygon membership.
#[derive(Debug, Copy, Clone, Default)]
pub struct GeofenceRule;

impl SpatialRule for GeofenceRule {
    fn apply(
        &self,
        spatial: &NaiveSpatialIndex,
        ctx: &RuleContext<'_>,
        state: &mut EntityState,
        scratch: &mut BTreeSet<String>,
        out: &mut Vec<Event>,
    ) {
        scratch.clear();
        spatial.geofence_membership_at(ctx.position, scratch);
        out.extend(membership_transitions(
            ctx.entity_id,
            &state.inside,
            scratch,
            ctx.at_ms,
        ));
        std::mem::swap(&mut state.inside, scratch);
    }
}

/// Corridor enter/exit (separate polygon layer).
#[derive(Debug, Copy, Clone, Default)]
pub struct CorridorRule;

impl SpatialRule for CorridorRule {
    fn apply(
        &self,
        spatial: &NaiveSpatialIndex,
        ctx: &RuleContext<'_>,
        state: &mut EntityState,
        scratch: &mut BTreeSet<String>,
        out: &mut Vec<Event>,
    ) {
        scratch.clear();
        spatial.corridor_membership_at(ctx.position, scratch);
        out.extend(corridor_membership_transitions(
            ctx.entity_id,
            &state.inside_corridor,
            scratch,
            ctx.at_ms,
        ));
        std::mem::swap(&mut state.inside_corridor, scratch);
    }
}

/// Radius approach/recede from disk membership.
#[derive(Debug, Copy, Clone, Default)]
pub struct RadiusRule;

impl SpatialRule for RadiusRule {
    fn apply(
        &self,
        spatial: &NaiveSpatialIndex,
        ctx: &RuleContext<'_>,
        state: &mut EntityState,
        scratch: &mut BTreeSet<String>,
        out: &mut Vec<Event>,
    ) {
        scratch.clear();
        spatial.radius_membership_at(ctx.position, scratch);
        out.extend(radius_membership_transitions(
            ctx.entity_id,
            &state.inside_radius,
            scratch,
            ctx.at_ms,
        ));
        std::mem::swap(&mut state.inside_radius, scratch);
    }
}

/// Primary catalog region assignment (tie-break: lexicographically smallest id).
#[derive(Debug, Copy, Clone, Default)]
pub struct CatalogRule;

impl SpatialRule for CatalogRule {
    fn apply(
        &self,
        spatial: &NaiveSpatialIndex,
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

/// Default pipeline: geofence, corridor, radius, catalog.
pub fn default_rules() -> Vec<Box<dyn SpatialRule>> {
    vec![
        Box::new(GeofenceRule),
        Box::new(CorridorRule),
        Box::new(RadiusRule),
        Box::new(CatalogRule),
    ]
}
