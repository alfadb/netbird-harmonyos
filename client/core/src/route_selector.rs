// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # route_selector — pure NetBird network-route selection state
//!
//! NetBird splits "which of the routes management advertised do we actually
//! install" out of the data plane: [`crate::network_map::ManagedRoute`] rows are
//! reduced to a small selection value. This module is that value for the
//! HarmonyOS client — a plain struct with **no IO**, no locks, no timers, no
//! persistence path and no dependency on the connector. The integration owner
//! keeps the state and calls these functions; nothing here touches a socket, a
//! file or a route table.
//!
//! ## Selection unit: the NetID (HA group), never the physical route
//!
//! A logical network is identified by `ManagedRoute.net_id`. Several routes can
//! carry the same NetID with different `id`/`peer`/`metric` — the high
//! availability candidates of one network (live example: `docs/route-audit.md`
//! records two `net_id=HostInternal` rows for `192.168.50.0/24` with metric 100
//! and 200 on two different peers). Selection is per NetID: selecting or
//! deselecting a network keeps every HA candidate of that network together, and
//! the physical route `id`/`peer` is never part of the selection key.
//!
//! ## Default-on semantics
//!
//! With no persisted preference every known network is selected, and networks
//! that appear in a later map are selected too ("future routes"). Only an
//! explicit per-network deselection — or the global `deselect_all` — turns a
//! network off.
//!
//! ## Upstream parity (pinned evidence)
//!
//! Semantics mirror `netbirdio/netbird` at commit
//! `f5707c348532a4b15ea39fb53264339f6045932c` (2026-10-06):
//! `client/internal/routeselector/routeselector.go` (the selector) and
//! `client/internal/routemanager/selection.go` + `manager.go` (exit-node
//! exclusivity and reconciliation). The points that are easy to get wrong:
//!
//! - `is_selected` consults **only** `deselected_routes` (plus `deselect_all`):
//!   `selected_routes` exists so an explicit user pick is distinguishable from
//!   the default (`has_user_selection`) and so the exit-node pick survives
//!   restarts. "Unknown" NetIDs are therefore selected by default.
//! - **Conflicting state** (an ID in both `selected_routes` and
//!   `deselected_routes`) is only possible from hand-written/stale state —
//!   every mutation deletes the ID from the other set. Upstream resolves it as
//!   *deselected wins* (`isSelectedLocked`: `deselectedAll || deselected[ID]`;
//!   `FilterSelected` drops exactly the deselected IDs). This module does the
//!   same and preserves the conflict verbatim on reload instead of silently
//!   normalizing it.
//! - `select_routes` validates every requested NetID against the known set
//!   **before** mutating, because a non-append request wipes the current
//!   selection first. A request whose IDs are all unknown performs no mutation
//!   (and does not clear `deselect_all`); a mixed request applies the valid part
//!   and reports the unknown IDs.
//! - `deselect_routes` is a no-op under `deselect_all` (upstream returns nil
//!   before validating), otherwise it reports unknown IDs but applies the valid
//!   ones.
//! - `select_all` clears both lists and the flag; `deselect_all` clears both
//!   lists and sets the flag — i.e. it also covers networks that only appear
//!   later. An explicit selection clears the flag and re-enables default-on for
//!   everything that is not explicitly deselected.
//! - Exit nodes (IPv4 `0.0.0.0/0`, [`crate::config::Route::is_default`]) are
//!   special: without an explicit user selection a candidate is only eligible
//!   when `skip_auto_apply == false`; an explicit user pick overrides that flag.
//!   At most **one** exit NetID stays active. An explicit exit selection
//!   deselects every other known exit in the same request, and management
//!   reconciliation ([`RouteSelector::reconcile_exit_nodes`]) picks the
//!   lexicographically smallest preferred NetID for determinism. The flag is
//!   never auto-cleared by reconciliation.
//! - `skip_auto_apply` is consulted **only** for exit nodes (upstream
//!   `applyExitNodeFilter`); it never disables a plain subnet route, and this
//!   module does not invent such a bypass.
//!
//! ## Out of scope (deliberately)
//!
//! No IPv6 and no `-v6` exit pairing: the management parser rejects IPv6
//! prefixes today ([`crate::network_map`]), so upstream `SyncPairedSelection` /
//! `mirrorV6ExitPairSelections` has no input here. No DNS, no persistence, no
//! NAPI/connector wiring, no compatibility layer for the upstream Go document
//! shape (see below).
//!
//! ## Persisted JSON shape (local state, snake_case)
//!
//! [`RouteSelector::to_json`] emits exactly
//! `{"selected_routes":[...],"deselected_routes":[...],"deselect_all":bool}`
//! with both arrays sorted ascending, so the document is deterministic
//! regardless of mutation order. [`RouteSelector::from_json`] accepts that
//! shape: missing keys become empty/false, an empty or `null` document is a
//! fresh (all-default) selector, duplicate known keys and wrong value types are
//! loud errors, and unknown keys are ignored so a newer writer cannot wipe a
//! user's preference. Upstream serializes the two sets as JSON **objects**
//! (Go `map` marshalling); that legacy object form is intentionally not
//! accepted here — this client owns its own state document and writes arrays.
//! NetIDs that no longer exist in the current map are preserved on load (stale
//! preferences survive) but are rejected by the mutation APIs, which validate
//! against the live known set.

use std::collections::BTreeSet;
use std::fmt;

use crate::config;
use crate::config::{parse_document, Json};
use crate::network_map::ManagedRoute;
use crate::util::json_escape;

// ---------------------------------------------------------------------------
// model
// ---------------------------------------------------------------------------

/// Pure per-NetID route selection state.
///
/// Invariant: the mutation methods keep `selected_routes` and
/// `deselected_routes` disjoint; only [`RouteSelector::from_json`] can load a
/// contradictory document, which is preserved as loaded (see module docs for
/// the resolution rule).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteSelector {
    /// NetIDs the user explicitly selected (distinguishes an explicit pick from
    /// the default and keeps the exit-node pick across restarts).
    selected_routes: BTreeSet<String>,
    /// NetIDs the user explicitly deselected.
    deselected_routes: BTreeSet<String>,
    /// Global "everything off" flag; unlike a per-ID deselection it also covers
    /// networks that do not exist yet.
    deselect_all: bool,
}

/// Reason a selection request or a persisted-state load was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteSelectorError {
    /// Requested NetIDs that are not in the known set. The valid part of the
    /// same request was still applied (upstream parity) unless every requested
    /// ID was unknown, in which case the state is untouched.
    UnknownRoutes(Vec<String>),
    /// The persisted state document could not be read (JSON syntax or shape).
    State { offset: usize, msg: String },
}

impl RouteSelectorError {
    fn state(offset: usize, msg: impl Into<String>) -> Self {
        RouteSelectorError::State { offset, msg: msg.into() }
    }

    fn from_config(err: config::ConfigError) -> Self {
        match err {
            config::ConfigError::Json { offset, msg } => RouteSelectorError::State { offset, msg },
            config::ConfigError::Field { field, reason } => {
                RouteSelectorError::State { offset: 0, msg: format!("field '{field}': {reason}") }
            }
        }
    }
}

impl fmt::Display for RouteSelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RouteSelectorError::UnknownRoutes(ids) => {
                write!(f, "route selector: unknown route net id(s): {}", ids.join(", "))
            }
            RouteSelectorError::State { offset, msg } => {
                write!(f, "route selector state: invalid JSON at byte {offset}: {msg}")
            }
        }
    }
}

impl std::error::Error for RouteSelectorError {}

/// Read-only grouped view of one logical network (one NetID) in a route list,
/// for UI listing and integration checks. Plain data; no framework.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogicalNetwork<'a> {
    /// Logical network identity (the selection unit).
    pub net_id: &'a str,
    /// Distinct advertised prefixes of this network, in first-seen map order
    /// (HA candidates of one network share the prefix).
    pub prefixes: Vec<config::Route>,
    /// Number of HA candidate rows carrying this NetID.
    pub candidate_count: usize,
    /// Current selection state ([`RouteSelector::is_selected`]).
    pub enabled: bool,
    /// True when the network is an IPv4 exit node (`0.0.0.0/0`).
    pub is_exit: bool,
}

// ---------------------------------------------------------------------------
// pure helpers
// ---------------------------------------------------------------------------

/// True when the route is an IPv4 exit node (`0.0.0.0/0`).
pub fn is_exit_route(route: &ManagedRoute) -> bool {
    route.network.is_default()
}

/// Every distinct NetID in the map, sorted ascending (the "known set" the
/// mutation APIs validate against).
pub fn known_net_ids(routes: &[ManagedRoute]) -> Vec<String> {
    routes.iter().map(|r| r.net_id.clone()).collect::<BTreeSet<_>>().into_iter().collect()
}

/// Every distinct exit-node NetID in the map, sorted ascending.
pub fn exit_net_ids(routes: &[ManagedRoute]) -> Vec<String> {
    routes
        .iter()
        .filter(|r| is_exit_route(r))
        .map(|r| r.net_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn min_net_id(ids: &[String]) -> Option<String> {
    ids.iter().min().cloned()
}

// ---------------------------------------------------------------------------
// selector
// ---------------------------------------------------------------------------

impl RouteSelector {
    /// A fresh selector: nothing explicit, everything default-on.
    pub fn new() -> Self {
        RouteSelector::default()
    }

    /// The global deselect-all flag (callers enforcing per-NetID invariants must
    /// leave the selection alone while it is set).
    pub fn is_deselect_all(&self) -> bool {
        self.deselect_all
    }

    /// Explicitly selected NetIDs, sorted (immutable view for persistence/UI).
    pub fn selected_routes(&self) -> &BTreeSet<String> {
        &self.selected_routes
    }

    /// Explicitly deselected NetIDs, sorted (immutable view for persistence/UI).
    pub fn deselected_routes(&self) -> &BTreeSet<String> {
        &self.deselected_routes
    }

    /// Whether a NetID is currently selected. Default-on: a NetID nobody
    /// mentioned — including one that does not exist yet — is selected unless
    /// the global flag is set.
    pub fn is_selected(&self, net_id: &str) -> bool {
        if self.deselect_all {
            return false;
        }
        !self.deselected_routes.contains(net_id)
    }

    /// Whether the user (or a previous reconciliation) explicitly touched this
    /// NetID, in either direction. An explicit selection outranks management's
    /// `skip_auto_apply` flag.
    pub fn has_user_selection(&self, net_id: &str) -> bool {
        self.selected_routes.contains(net_id) || self.deselected_routes.contains(net_id)
    }

    /// Reset to all-selected: clears both lists and the global flag, so networks
    /// appearing later are selected too.
    pub fn select_all(&mut self) {
        self.deselect_all = false;
        self.selected_routes.clear();
        self.deselected_routes.clear();
    }

    /// Turn everything off, including networks that appear later: clears both
    /// lists and sets the global flag.
    pub fn deselect_all(&mut self) {
        self.deselect_all = true;
        self.selected_routes.clear();
        self.deselected_routes.clear();
    }

    /// Select the given NetIDs (upstream `RouteSelector.SelectRoutes`).
    ///
    /// `known` is the live NetID set ([`known_net_ids`]). Every requested ID is
    /// validated first: unknown IDs are collected and returned, and a request
    /// whose IDs are *all* unknown performs no mutation at all (a non-append
    /// request would otherwise wipe the selection and put nothing back). A mixed
    /// request still applies its valid part. `append == false` replaces the
    /// whole selection: every known NetID is marked deselected first, then the
    /// requested ones are selected. An empty `append == false` request is the
    /// upstream "deselect everything known" boundary, not an error. Selecting
    /// clears `deselect_all`.
    pub fn select_routes(
        &mut self,
        net_ids: &[String],
        append: bool,
        known: &[String],
    ) -> Result<(), RouteSelectorError> {
        let known_set: BTreeSet<&str> = known.iter().map(String::as_str).collect();
        let mut available: Vec<&str> = Vec::new();
        let mut unknown: Vec<String> = Vec::new();
        for id in net_ids {
            if known_set.contains(id.as_str()) {
                if !available.contains(&id.as_str()) {
                    available.push(id.as_str());
                }
            } else if !unknown.contains(id) {
                unknown.push(id.clone());
            }
        }

        if available.is_empty() && !unknown.is_empty() {
            return Err(RouteSelectorError::UnknownRoutes(unknown));
        }

        if !append || self.deselect_all {
            self.deselected_routes.clear();
            self.selected_routes.clear();
            for id in known {
                self.deselected_routes.insert(id.clone());
            }
        }
        for id in available {
            self.deselected_routes.remove(id);
            self.selected_routes.insert(id.to_string());
        }
        self.deselect_all = false;

        if unknown.is_empty() {
            Ok(())
        } else {
            Err(RouteSelectorError::UnknownRoutes(unknown))
        }
    }

    /// Deselect the given NetIDs (upstream `RouteSelector.DeselectRoutes`).
    ///
    /// Under `deselect_all` this is a no-op and reports nothing (upstream
    /// returns before validating). Otherwise unknown IDs are reported while the
    /// valid IDs are still deselected.
    pub fn deselect_routes(
        &mut self,
        net_ids: &[String],
        known: &[String],
    ) -> Result<(), RouteSelectorError> {
        if self.deselect_all {
            return Ok(());
        }
        let known_set: BTreeSet<&str> = known.iter().map(String::as_str).collect();
        let mut unknown: Vec<String> = Vec::new();
        for id in net_ids {
            if !known_set.contains(id.as_str()) {
                if !unknown.contains(id) {
                    unknown.push(id.clone());
                }
                continue;
            }
            self.deselected_routes.insert(id.clone());
            self.selected_routes.remove(id);
        }
        if unknown.is_empty() {
            Ok(())
        } else {
            Err(RouteSelectorError::UnknownRoutes(unknown))
        }
    }

    /// Make `preferred` the only selected exit NetID among `exit_net_ids`
    /// (upstream `RouteSelector.SetExclusiveExitNode`): every other exit ID in
    /// the list is deselected, `preferred` (when non-empty) is selected, all in
    /// one step. Non-exit selections are never referenced. A global
    /// deselect-all is left untouched. `preferred == None` turns every listed
    /// exit off without touching anything else.
    pub fn set_exclusive_exit_node(&mut self, preferred: Option<&str>, exit_net_ids: &[String]) {
        if self.deselect_all {
            return;
        }
        for id in exit_net_ids {
            if Some(id.as_str()) == preferred {
                continue;
            }
            self.deselected_routes.insert(id.clone());
            self.selected_routes.remove(id);
        }
        if let Some(preferred) = preferred {
            self.deselected_routes.remove(preferred);
            self.selected_routes.insert(preferred.to_string());
        }
    }

    /// Reconcile the single active exit node against the current map (upstream
    /// `DefaultManager.updateRouteSelectorFromManagement`): the user's persisted
    /// pick wins (lexicographically smallest if legacy state holds several),
    /// otherwise the map's auto-applicable exit (`skip_auto_apply == false`,
    /// lexicographically smallest if several), otherwise no exit is activated at
    /// all. Does nothing while `deselect_all` is set — an explicit "all off"
    /// must not be undone by management auto-apply. IPv4 only; the upstream
    /// `-v6` pair mirroring has no input because the parser rejects IPv6.
    pub fn reconcile_exit_nodes(&mut self, routes: &[ManagedRoute]) {
        if self.deselect_all {
            return;
        }
        let exits = exit_net_ids(routes);
        if exits.is_empty() {
            return;
        }
        let mut user_selected: Vec<String> = Vec::new();
        let mut management_selected: Vec<String> = Vec::new();
        for id in &exits {
            if self.has_user_selection(id) {
                if self.is_selected(id) {
                    user_selected.push(id.clone());
                }
            } else if routes.iter().any(|r| r.net_id == *id && !r.skip_auto_apply) {
                management_selected.push(id.clone());
            }
        }
        let preferred = min_net_id(&user_selected).or_else(|| min_net_id(&management_selected));
        self.set_exclusive_exit_node(preferred.as_deref(), &exits);
    }

    /// Select one logical network by NetID, validating against the live map and
    /// enforcing exit-node exclusivity (upstream `DefaultManager.SelectRoutes`
    /// with `appendRoute = true`: the UI checkbox behaviour — other networks
    /// keep their state). Unknown NetID: error, state untouched.
    pub fn select(&mut self, net_id: &str, routes: &[ManagedRoute]) -> Result<(), RouteSelectorError> {
        self.select_routes_exclusive(&[net_id.to_string()], true, routes)
    }

    /// Deselect one logical network by NetID, validating against the live map.
    /// Unknown NetID: error, state untouched. (Deselecting an exit simply turns
    /// it off; exclusivity only constrains activation.)
    pub fn deselect(
        &mut self,
        net_id: &str,
        routes: &[ManagedRoute],
    ) -> Result<(), RouteSelectorError> {
        self.deselect_routes(&[net_id.to_string()], &known_net_ids(routes))
    }

    /// Batch selection against the live map, with exit-node exclusivity
    /// (upstream `DefaultManager.selectRoutes`): after applying the selection,
    /// if the request activates an exit NetID, every other known exit NetID is
    /// deselected. Exclusivity runs even when the selection reports a partial
    /// (unknown-ID) failure, because the valid part of the request was applied.
    pub fn select_routes_exclusive(
        &mut self,
        net_ids: &[String],
        append: bool,
        routes: &[ManagedRoute],
    ) -> Result<(), RouteSelectorError> {
        let known = known_net_ids(routes);
        let request: BTreeSet<&str> = net_ids.iter().map(String::as_str).collect();
        let result = self.select_routes(net_ids, append, &known);

        let exits = exit_net_ids(routes);
        let activates = exits.iter().any(|id| request.contains(id.as_str()));
        if activates {
            let others: Vec<String> =
                exits.iter().filter(|id| !request.contains(id.as_str())).cloned().collect();
            if !others.is_empty() {
                // Every `others` entry is in `known`, so this can only report
                // nothing; it also cannot silently fail to deselect a sibling.
                let _ = self.deselect_routes(&others, &known);
            }
        }
        result
    }

    /// Select-all followed by the immediate exit-node reconciliation (upstream
    /// `DefaultManager.selectAllRoutes`), so wiping the explicit state cannot
    /// leave two auto-applicable exit nodes active at once.
    pub fn select_all_reconciled(&mut self, routes: &[ManagedRoute]) {
        self.select_all();
        self.reconcile_exit_nodes(routes);
    }

    /// Routes to apply, cloned, in map order: everything selected by default,
    /// minus explicitly deselected NetIDs (all HA candidates of a NetID share
    /// the verdict). `skip_auto_apply` is not consulted here — see
    /// [`RouteSelector::filter_selected_exit_nodes`].
    pub fn filter_selected(&self, routes: &[ManagedRoute]) -> Vec<ManagedRoute> {
        self.filter_selected_refs(routes).into_iter().cloned().collect()
    }

    /// Borrowing variant of [`RouteSelector::filter_selected`] (no clone).
    pub fn filter_selected_refs<'a>(&self, routes: &'a [ManagedRoute]) -> Vec<&'a ManagedRoute> {
        if self.deselect_all {
            return Vec::new();
        }
        routes.iter().filter(|r| !self.deselected_routes.contains(&r.net_id)).collect()
    }

    /// Routes to apply with the exit-node rule applied (upstream
    /// `FilterSelectedExitNodes`), cloned, in map order. Non-exit networks are
    /// included unless their NetID is deselected. An exit network without an
    /// explicit user selection contributes only its auto-applicable candidates
    /// (`skip_auto_apply == false`); when it has an explicit selection the whole
    /// candidate group is kept. A deselected NetID is always excluded, as is
    /// everything under `deselect_all`.
    pub fn filter_selected_exit_nodes(&self, routes: &[ManagedRoute]) -> Vec<ManagedRoute> {
        self.filter_selected_exit_nodes_refs(routes).into_iter().cloned().collect()
    }

    /// Borrowing variant of [`RouteSelector::filter_selected_exit_nodes`].
    pub fn filter_selected_exit_nodes_refs<'a>(
        &self,
        routes: &'a [ManagedRoute],
    ) -> Vec<&'a ManagedRoute> {
        if self.deselect_all {
            return Vec::new();
        }
        let exit_nets: BTreeSet<&str> =
            routes.iter().filter(|r| is_exit_route(r)).map(|r| r.net_id.as_str()).collect();
        routes
            .iter()
            .filter(|r| {
                let net_id = r.net_id.as_str();
                if self.deselected_routes.contains(net_id) {
                    return false;
                }
                if !exit_nets.contains(net_id) {
                    return true;
                }
                // An explicit user selection overrides management's flag;
                // otherwise only auto-applicable candidates stay on.
                self.has_user_selection(net_id) || !r.skip_auto_apply
            })
            .collect()
    }

    /// Group the map into one [`LogicalNetwork`] per NetID, in first-seen order.
    pub fn logical_networks<'a>(&self, routes: &'a [ManagedRoute]) -> Vec<LogicalNetwork<'a>> {
        let mut out: Vec<LogicalNetwork<'a>> = Vec::new();
        for route in routes {
            if let Some(net) = out.iter_mut().find(|n| n.net_id == route.net_id.as_str()) {
                net.candidate_count += 1;
                if !net.prefixes.contains(&route.network) {
                    net.prefixes.push(route.network);
                }
                net.is_exit |= is_exit_route(route);
            } else {
                out.push(LogicalNetwork {
                    net_id: route.net_id.as_str(),
                    prefixes: vec![route.network],
                    candidate_count: 1,
                    enabled: self.is_selected(&route.net_id),
                    is_exit: is_exit_route(route),
                });
            }
        }
        out
    }

    /// Serialize to the local-state document (see module docs): fixed key order,
    /// arrays sorted ascending, snake_case.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"selected_routes\":{},\"deselected_routes\":{},\"deselect_all\":{}}}",
            net_id_array(&self.selected_routes),
            net_id_array(&self.deselected_routes),
            self.deselect_all,
        )
    }

    /// Parse a persisted local-state document. An empty or `null` document is a
    /// fresh selector (upstream `UnmarshalJSON`); missing keys become
    /// empty/false; duplicate known keys and wrong value types are errors;
    /// unknown keys are ignored. NetIDs are not validated here — stale
    /// preferences are preserved and only rejected by the mutation APIs.
    pub fn from_json(text: &str) -> Result<Self, RouteSelectorError> {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(RouteSelector::new());
        }
        let doc = parse_document(text).map_err(RouteSelectorError::from_config)?;
        Self::from_value(&doc)
    }

    /// Parse an ALREADY PARSED selector document — the adapter the start
    /// configuration uses for its `route_selection` object
    /// ([`crate::connector::ConnectorConfig`]). The start reader has already
    /// run the one strict JSON parser over the whole document, so this entry
    /// point runs the SAME shape/value rules as [`RouteSelector::from_json`]
    /// on that parsed value instead of rendering it back to text and parsing
    /// it a second time. `Json::Null` (like the empty/`null` text form) is a
    /// fresh all-default selector; any other non-object is a loud error.
    ///
    /// `pub(crate)`: the parsed-JSON value type is crate-private, so this
    /// adapter is too (the public entry point remains `from_json`).
    pub(crate) fn from_value(value: &Json) -> Result<Self, RouteSelectorError> {
        let entries = match value {
            Json::Null => return Ok(RouteSelector::new()),
            Json::Obj(entries) => entries,
            _ => return Err(RouteSelectorError::state(0, "expected a JSON object")),
        };

        let mut selected: Option<BTreeSet<String>> = None;
        let mut deselected: Option<BTreeSet<String>> = None;
        let mut deselect_all: Option<bool> = None;
        for (key, value) in entries {
            match key.as_str() {
                "selected_routes" => {
                    if selected.is_some() {
                        return Err(RouteSelectorError::state(0, "duplicate field 'selected_routes'"));
                    }
                    selected = Some(net_id_set(value, "selected_routes")?);
                }
                "deselected_routes" => {
                    if deselected.is_some() {
                        return Err(RouteSelectorError::state(
                            0,
                            "duplicate field 'deselected_routes'",
                        ));
                    }
                    deselected = Some(net_id_set(value, "deselected_routes")?);
                }
                "deselect_all" => {
                    if deselect_all.is_some() {
                        return Err(RouteSelectorError::state(0, "duplicate field 'deselect_all'"));
                    }
                    match value {
                        Json::Bool(flag) => deselect_all = Some(*flag),
                        _ => {
                            return Err(RouteSelectorError::state(
                                0,
                                "field 'deselect_all' must be a boolean",
                            ))
                        }
                    }
                }
                // Unknown keys are ignored: a newer writer must not wipe the
                // user's preference (upstream json.Unmarshal behaves the same).
                _ => {}
            }
        }

        Ok(RouteSelector {
            selected_routes: selected.unwrap_or_default(),
            deselected_routes: deselected.unwrap_or_default(),
            deselect_all: deselect_all.unwrap_or(false),
        })
    }
}

fn net_id_set(value: &Json, field: &str) -> Result<BTreeSet<String>, RouteSelectorError> {
    match value {
        Json::Arr(items) => {
            let mut set = BTreeSet::new();
            for item in items {
                match item {
                    Json::Str(id) => {
                        set.insert(id.clone());
                    }
                    _ => {
                        return Err(RouteSelectorError::state(
                            0,
                            format!("field '{field}' must be an array of strings"),
                        ))
                    }
                }
            }
            Ok(set)
        }
        _ => Err(RouteSelectorError::state(0, format!("field '{field}' must be an array of strings"))),
    }
}

fn net_id_array(ids: &BTreeSet<String>) -> String {
    let mut out = String::from("[");
    for (index, id) in ids.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&json_escape(id));
        out.push('"');
    }
    out.push(']');
    out
}

// ---------------------------------------------------------------------------
// unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Route;

    fn route(
        net_id: &str,
        id: &str,
        peer: &str,
        metric: i64,
        network: [u8; 4],
        prefix_len: u8,
    ) -> ManagedRoute {
        ManagedRoute {
            id: id.into(),
            network: Route { addr: network, prefix_len },
            domains: vec![],
            net_id: net_id.into(),
            network_type: 1,
            peer: peer.into(),
            metric,
            masquerade: true,
            keep_route: true,
            skip_auto_apply: false,
        }
    }

    fn subnet(net_id: &str, id: &str) -> ManagedRoute {
        route(net_id, id, "router-key", 10, [192, 168, 50, 0], 24)
    }

    fn exit(net_id: &str, id: &str, skip_auto_apply: bool) -> ManagedRoute {
        let mut route = route(net_id, id, "exit-router-key", 10, [0, 0, 0, 0], 0);
        route.skip_auto_apply = skip_auto_apply;
        route
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn default_state_selects_all_routes_and_networks_added_later() {
        let selector = RouteSelector::new();
        let routes = vec![subnet("lan", "r-lan"), subnet("corp", "r-corp")];
        assert!(!selector.is_deselect_all());
        assert_eq!(selector.filter_selected(&routes).len(), 2);
        assert_eq!(selector.filter_selected_exit_nodes(&routes).len(), 2);

        // A network nobody mentioned — including one that does not exist yet —
        // is selected by default.
        assert!(selector.is_selected("appears-later"));
        let with_future = vec![subnet("lan", "r-lan"), subnet("corp", "r-corp"), subnet("future", "r-future")];
        assert_eq!(selector.filter_selected(&with_future).len(), 3);
        assert!(selector.selected_routes().is_empty());
        assert!(selector.deselected_routes().is_empty());
    }

    /// The two `net_id=HostInternal` rows recorded in docs/route-audit.md
    /// (same prefix, distinct route IDs/peers, metric 100 vs 200) are one HA
    /// group and must toggle together.
    #[test]
    fn ha_group_candidates_toggle_together_and_reenable() {
        let routes = vec![
            route("HostInternal", "d29f0ml27eas73a9rqig", "F0GVt4oE", 200, [192, 168, 50, 0], 24),
            route("HostInternal", "d29f0ml27eas73a9rqig:r2", "t7tldQA3", 100, [192, 168, 50, 0], 24),
        ];
        let mut selector = RouteSelector::new();
        assert_eq!(known_net_ids(&routes), ids(&["HostInternal"]));

        selector.deselect("HostInternal", &routes).expect("known NetID");
        assert!(!selector.is_selected("HostInternal"));
        assert!(selector.filter_selected(&routes).is_empty());
        assert!(selector.filter_selected_exit_nodes(&routes).is_empty());

        selector.select("HostInternal", &routes).expect("known NetID");
        let back = selector.filter_selected(&routes);
        assert_eq!(back.len(), 2, "both HA candidates come back together");
        assert_eq!(
            back.iter().map(|r| r.metric).collect::<Vec<_>>(),
            vec![200, 100],
            "map order is preserved"
        );
    }

    #[test]
    fn independent_net_ids_toggle_without_cross_effect() {
        let routes = vec![subnet("lan", "r-lan"), subnet("corp", "r-corp")];
        let mut selector = RouteSelector::new();

        selector.deselect("lan", &routes).expect("known");
        let remaining = selector.filter_selected(&routes);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].net_id, "corp");

        selector.select("lan", &routes).expect("known");
        assert_eq!(selector.filter_selected(&routes).len(), 2);
    }

    #[test]
    fn unknown_id_mutation_is_atomic() {
        let routes = vec![subnet("a", "r-a"), subnet("b", "r-b")];
        let mut selector = RouteSelector::new();
        selector.deselect("b", &routes).expect("known");
        let before = selector.clone();

        let err = selector.select("missing", &routes).unwrap_err();
        assert_eq!(err, RouteSelectorError::UnknownRoutes(ids(&["missing"])));
        assert_eq!(selector, before, "a rejected select leaves the state untouched");

        let err = selector.deselect("missing", &routes).unwrap_err();
        assert_eq!(err, RouteSelectorError::UnknownRoutes(ids(&["missing"])));
        assert_eq!(selector, before, "a rejected deselect leaves the state untouched");
    }

    #[test]
    fn all_unknown_request_keeps_selection_and_deselect_all_policy() {
        let known = ids(&["a", "b"]);
        let mut selector = RouteSelector::new();
        selector.select_routes(&ids(&["a"]), false, &known).expect("known");
        assert!(!selector.is_selected("b"));

        let err = selector.select_routes(&ids(&["Missing"]), false, &known).unwrap_err();
        assert!(matches!(err, RouteSelectorError::UnknownRoutes(_)), "{err:?}");
        assert!(selector.is_selected("a"), "the previous selection survives a fully invalid request");
        assert!(!selector.is_selected("b"), "nothing else may become selected");

        // The early return must not clear the global "all off" policy, or a
        // typo'd NetID would silently re-enable every route.
        let mut off = RouteSelector::new();
        off.deselect_all();
        let err = off.select_routes(&ids(&["Missing"]), false, &known).unwrap_err();
        assert!(matches!(err, RouteSelectorError::UnknownRoutes(_)));
        assert!(off.is_deselect_all());
        assert!(!off.is_selected("not-in-the-map-yet"));
    }

    #[test]
    fn partial_unknown_request_applies_valid_ids_and_reports_error() {
        let known = ids(&["a", "b"]);

        // Unknown first, valid last (and the reverse) — both must select `a`,
        // deselect `b`, and report the unknown ID.
        for request in [ids(&["missing", "a"]), ids(&["a", "missing"])] {
            let mut selector = RouteSelector::new();
            let err = selector.select_routes(&request, false, &known).unwrap_err();
            assert_eq!(err, RouteSelectorError::UnknownRoutes(ids(&["missing"])));
            assert!(selector.is_selected("a"), "the valid part of the request is applied");
            assert!(!selector.is_selected("b"), "a replace request wipes the rest");
        }
    }

    #[test]
    fn empty_non_append_request_deselects_every_known_net_id() {
        let known = ids(&["a", "b"]);
        let mut selector = RouteSelector::new();
        selector.select_routes(&ids(&["a"]), false, &known).expect("known");

        selector.select_routes(&[], false, &known).expect("empty replace is not an error");
        assert!(!selector.is_selected("a"));
        assert!(!selector.is_selected("b"));
        assert!(!selector.is_deselect_all(), "an empty replace is not the global flag");
        assert!(selector.is_selected("c-appears-later"), "later networks are default-on again");
    }

    #[test]
    fn json_roundtrip_is_exact_and_order_independent() {
        let known = ids(&["corp", "lan"]);
        let mut selector = RouteSelector::new();
        selector.select_routes(&ids(&["lan"]), false, &known).expect("known");
        selector.deselect_routes(&ids(&["corp"]), &known).expect("known");

        let json = selector.to_json();
        assert_eq!(
            json,
            "{\"selected_routes\":[\"lan\"],\"deselected_routes\":[\"corp\"],\"deselect_all\":false}"
        );
        let reloaded = RouteSelector::from_json(&json).expect("own document");
        assert_eq!(reloaded, selector);
        assert_eq!(reloaded.to_json(), json, "reload is byte-stable");

        // Mutation order must not leak into the persisted document.
        let mut first = RouteSelector::new();
        first.deselect_routes(&ids(&["b", "a"]), &ids(&["a", "b", "c"])).expect("known");
        let mut second = RouteSelector::new();
        second.deselect_routes(&ids(&["a", "b"]), &ids(&["b", "c", "a"])).expect("known");
        assert_eq!(first.to_json(), second.to_json());
        assert_eq!(
            first.to_json(),
            "{\"selected_routes\":[],\"deselected_routes\":[\"a\",\"b\"],\"deselect_all\":false}"
        );

        let mut off = RouteSelector::new();
        off.deselect_all();
        assert_eq!(
            off.to_json(),
            "{\"selected_routes\":[],\"deselected_routes\":[],\"deselect_all\":true}"
        );
    }

    #[test]
    fn json_load_preserves_stale_net_ids_but_mutation_rejects_them() {
        let mut selector = RouteSelector::from_json(
            "{\"selected_routes\":[\"stale-net\",\"a\"],\"deselected_routes\":[\"gone\"],\"deselect_all\":false}",
        )
        .expect("loads");
        assert!(selector.is_selected("stale-net"), "a stale preference is preserved on load");
        assert!(!selector.is_selected("gone"));
        assert_eq!(
            selector.to_json(),
            "{\"selected_routes\":[\"a\",\"stale-net\"],\"deselected_routes\":[\"gone\"],\"deselect_all\":false}"
        );

        let routes = vec![subnet("a", "r-a")];
        let before = selector.clone();
        let err = selector.select("stale-net", &routes).unwrap_err();
        assert_eq!(err, RouteSelectorError::UnknownRoutes(ids(&["stale-net"])));
        assert_eq!(selector, before, "mutation validates against the live map");
    }

    #[test]
    fn json_accepts_null_and_empty_and_ignores_unknown_keys() {
        let fresh = RouteSelector::new();
        assert_eq!(RouteSelector::from_json("").expect("empty"), fresh);
        assert_eq!(RouteSelector::from_json("null").expect("null"), fresh);
        assert_eq!(RouteSelector::from_json("  null  ").expect("blank null"), fresh);

        // Unknown keys are ignored so a newer writer cannot wipe the preference.
        let loaded = RouteSelector::from_json(
            "{\"selected_routes\":[\"a\"],\"future_field\":7,\"nested\":{\"x\":1}}",
        )
        .expect("unknown keys");
        assert!(loaded.is_selected("a"));
        assert_eq!(
            loaded.to_json(),
            "{\"selected_routes\":[\"a\"],\"deselected_routes\":[],\"deselect_all\":false}"
        );

        // Duplicate known keys and wrong types are loud errors.
        assert!(RouteSelector::from_json("{\"deselect_all\":true,\"deselect_all\":false}").is_err());
        assert!(RouteSelector::from_json("{\"selected_routes\":{}}").is_err());
        assert!(RouteSelector::from_json("{\"deselected_routes\":[1]}").is_err());
        assert!(RouteSelector::from_json("{\"deselect_all\":1}").is_err());
        assert!(RouteSelector::from_json("[]").is_err());
        assert!(RouteSelector::from_json("{").is_err());
    }

    #[test]
    fn deselect_all_covers_future_networks_with_explicit_selection_as_exception() {
        let routes = vec![subnet("a", "r-a"), subnet("b", "r-b")];
        let mut selector = RouteSelector::new();
        selector.deselect_all();
        assert!(selector.is_deselect_all());
        assert!(!selector.is_selected("a"));
        assert!(!selector.is_selected("future"), "the flag covers networks not in the map yet");

        // An explicit pick is the exception: it clears the flag and marks the
        // remaining known networks deselected (upstream replace branch).
        selector.select("a", &routes).expect("known");
        assert!(!selector.is_deselect_all());
        assert!(selector.is_selected("a"));
        assert!(!selector.is_selected("b"));
        assert!(selector.is_selected("future"));
    }

    #[test]
    fn select_all_resets_lists_and_flag() {
        let routes = vec![subnet("a", "r-a")];
        let mut selector = RouteSelector::new();
        selector.deselect_all();
        selector.deselect_routes(&ids(&["a"]), &known_net_ids(&routes)).expect("no-op under flag");

        selector.select_all();
        assert!(!selector.is_deselect_all());
        assert!(selector.selected_routes().is_empty());
        assert!(selector.deselected_routes().is_empty());
        assert!(selector.is_selected("a"));
        assert!(selector.is_selected("future"));

        // Also resets a per-ID selection back to default-on.
        let mut picked = RouteSelector::new();
        picked.deselect("a", &routes).expect("known");
        picked.select_all();
        assert!(picked.is_selected("a"));
        assert_eq!(
            picked.to_json(),
            "{\"selected_routes\":[],\"deselected_routes\":[],\"deselect_all\":false}"
        );
    }

    /// Hand-written/stale state only (every mutation deletes the ID from the
    /// other set). Official resolution: the **deselected** map wins
    /// (upstream `isSelectedLocked` consults only `deselectedRoutes`).
    #[test]
    fn conflicting_selected_and_deselected_state_resolves_deselected_wins() {
        let mut selector = RouteSelector::from_json(
            "{\"selected_routes\":[\"x\"],\"deselected_routes\":[\"x\"],\"deselect_all\":false}",
        )
        .expect("loads");
        assert!(selector.has_user_selection("x"));
        assert!(!selector.is_selected("x"), "the explicit deselection wins");
        let routes = vec![subnet("x", "r-x"), exit("x-exit", "r-exit", false)];
        assert!(selector.filter_selected(&routes).iter().all(|r| r.net_id == "x-exit"));
        assert!(selector
            .filter_selected_exit_nodes_refs(&routes)
            .iter()
            .all(|r| r.net_id != "x"));

        // The conflict survives a reload verbatim instead of being normalized.
        assert_eq!(
            selector.to_json(),
            "{\"selected_routes\":[\"x\"],\"deselected_routes\":[\"x\"],\"deselect_all\":false}"
        );

        // A mutation resolves it deterministically.
        selector.select("x", &vec![subnet("x", "r-x")]).expect("known");
        assert!(selector.is_selected("x"));
        assert!(selector.deselected_routes().is_empty());
    }

    #[test]
    fn unknown_deselect_under_deselect_all_is_a_noop() {
        // Upstream `DeselectRoutes` returns before validating while the global
        // flag is set: the state is already all-off.
        let mut selector = RouteSelector::new();
        selector.deselect_all();
        let before = selector.clone();
        selector.deselect_routes(&ids(&["missing"]), &ids(&["a"])).expect("no-op");
        assert_eq!(selector, before);
    }

    #[test]
    fn exit_reconcile_picks_single_lexicographically_first_eligible_exit() {
        let routes = vec![
            exit("exit-b", "r-b", false),
            exit("exit-a", "r-a", false),
            subnet("lan", "r-lan"),
        ];
        let mut selector = RouteSelector::new();
        selector.reconcile_exit_nodes(&routes);
        assert!(selector.is_selected("exit-a"), "deterministic lexical pick");
        assert!(!selector.is_selected("exit-b"), "at most one exit NetID stays on");
        assert!(selector.is_selected("lan"), "non-exit selection is untouched");

        // The pick is stable regardless of map order.
        let mut reordered = routes.clone();
        reordered.reverse();
        let mut other = RouteSelector::new();
        other.reconcile_exit_nodes(&reordered);
        assert_eq!(other.to_json(), selector.to_json());
    }

    #[test]
    fn explicit_exit_selection_is_exclusive_and_overrides_skip_auto_apply() {
        let routes = vec![
            exit("exit-b", "r-b", false),
            exit("exit-a", "r-a", true),
            subnet("lan", "r-lan"),
        ];
        let mut selector = RouteSelector::new();
        selector.reconcile_exit_nodes(&routes);
        assert!(selector.is_selected("exit-b"), "only the auto-applicable exit is eligible");
        assert!(!selector.is_selected("exit-a"), "skip_auto_apply keeps it off without a user pick");

        // The explicit pick overrides the flag and turns the sibling off in the
        // same request.
        selector.select("exit-a", &routes).expect("known");
        assert!(selector.is_selected("exit-a"));
        assert!(!selector.is_selected("exit-b"), "the other known exit is deselected");
        assert!(selector.is_selected("lan"));
        assert_eq!(selector.filter_selected_exit_nodes(&routes).len(), 2);

        // The pick survives a reload and the next map reconciliation.
        let json = selector.to_json();
        let mut reloaded = RouteSelector::from_json(&json).expect("own document");
        reloaded.reconcile_exit_nodes(&routes);
        assert!(reloaded.is_selected("exit-a"));
        assert!(!reloaded.is_selected("exit-b"));
        assert_eq!(reloaded.to_json(), json);
    }

    #[test]
    fn select_all_reconciled_keeps_a_single_exit_node() {
        let routes = vec![
            exit("exit-b", "r-b", false),
            exit("exit-a", "r-a", false),
            subnet("lan", "r-lan"),
        ];
        let mut selector = RouteSelector::new();
        selector.select("exit-b", &routes).expect("known");

        selector.select_all_reconciled(&routes);
        assert!(selector.is_selected("lan"), "non-exit routes are all selected");
        assert!(selector.is_selected("exit-a"), "the deterministic management pick is active");
        assert!(!selector.is_selected("exit-b"), "select-all wiped the old pick, not turned on two");
    }

    #[test]
    fn disabled_exit_stays_off_while_subnet_keeps_default_selection() {
        // "Corp" is management-disabled (skip_auto_apply) but is not an exit:
        // SkipAutoApply is only consulted for exit nodes, so the subnet keeps the
        // default-on selection and no bypass is invented for it.
        let mut corp = subnet("corp", "r-corp");
        corp.skip_auto_apply = true;
        let routes = vec![corp, exit("exit-x", "r-exit", true)];
        let mut selector = RouteSelector::new();
        selector.reconcile_exit_nodes(&routes);

        assert!(!selector.is_selected("exit-x"), "a disabled exit is not auto-applied");
        let applied: Vec<&str> = selector
            .filter_selected_exit_nodes_refs(&routes)
            .iter()
            .map(|r| r.net_id.as_str())
            .collect();
        assert_eq!(applied, vec!["corp"], "the subnet is still returned by default");
        assert!(selector.filter_selected(&routes).iter().all(|r| r.net_id == "corp"));

        // Only an explicit selection turns the disabled exit on.
        selector.select("exit-x", &routes).expect("known");
        assert!(selector.is_selected("exit-x"));
        assert_eq!(selector.filter_selected_exit_nodes(&routes).len(), 2);
    }

    #[test]
    fn deselect_all_keeps_exits_off_across_reconcile() {
        let routes = vec![exit("exit-a", "r-a", false), subnet("lan", "r-lan")];
        let mut selector = RouteSelector::new();
        selector.deselect_all();
        let before = selector.clone();

        selector.reconcile_exit_nodes(&routes);
        assert_eq!(selector, before, "management auto-apply must not undo an explicit all-off");
        assert!(selector.filter_selected(&routes).is_empty());
        assert!(selector.filter_selected_exit_nodes(&routes).is_empty());
    }

    #[test]
    fn set_exclusive_exit_node_ignores_deselect_all_and_leaves_non_exits_alone() {
        let known = ids(&["lan", "exit-a", "exit-b"]);
        let exits = ids(&["exit-a", "exit-b"]);
        let mut selector = RouteSelector::new();
        selector.deselect_routes(&ids(&["lan"]), &known).expect("known");

        selector.set_exclusive_exit_node(Some("exit-b"), &exits);
        assert!(selector.is_selected("exit-b"));
        assert!(!selector.is_selected("exit-a"));
        assert!(!selector.is_selected("lan"), "non-exit state untouched");

        selector.set_exclusive_exit_node(None, &exits);
        assert!(!selector.is_selected("exit-a") && !selector.is_selected("exit-b"));
        assert!(!selector.is_selected("lan"), "turning exits off touches nothing else");

        let mut off = RouteSelector::new();
        off.deselect_all();
        let before = off.clone();
        off.set_exclusive_exit_node(Some("exit-a"), &exits);
        assert_eq!(off, before, "the global all-off policy wins");
    }

    #[test]
    fn logical_networks_group_ha_candidates_with_state() {
        let routes = vec![
            route("HostInternal", "id-200", "peer-a", 200, [192, 168, 50, 0], 24),
            route("HostInternal", "id-100", "peer-b", 100, [192, 168, 50, 0], 24),
            exit("exit-a", "id-exit", false),
        ];
        let mut selector = RouteSelector::new();
        selector.deselect("HostInternal", &routes).expect("known");

        let view = selector.logical_networks(&routes);
        assert_eq!(view.len(), 2);
        assert_eq!(view[0].net_id, "HostInternal");
        assert_eq!(view[0].candidate_count, 2);
        assert_eq!(view[0].prefixes, vec![Route { addr: [192, 168, 50, 0], prefix_len: 24 }]);
        assert!(!view[0].enabled);
        assert!(!view[0].is_exit);
        assert_eq!(view[1].net_id, "exit-a");
        assert_eq!(view[1].candidate_count, 1);
        assert!(view[1].enabled);
        assert!(view[1].is_exit);
        assert_eq!(view[1].prefixes, vec![Route { addr: [0, 0, 0, 0], prefix_len: 0 }]);
    }

    #[test]
    fn base_managed_routes_are_not_mutated_by_selection_or_filtering() {
        let routes = vec![
            route("HostInternal", "id-200", "peer-a", 200, [192, 168, 50, 0], 24),
            route("HostInternal", "id-100", "peer-b", 100, [192, 168, 50, 0], 24),
            exit("exit-a", "id-exit", false),
            subnet("lan", "id-lan"),
        ];
        let snapshot = routes.clone();

        let mut selector = RouteSelector::new();
        selector.reconcile_exit_nodes(&routes);
        selector.select("HostInternal", &routes).expect("known");
        selector.deselect("exit-a", &routes).expect("known");
        let _ = selector.filter_selected(&routes);
        let _ = selector.filter_selected_exit_nodes(&routes);
        let _ = selector.logical_networks(&routes);
        let _ = known_net_ids(&routes);
        let _ = exit_net_ids(&routes);
        assert_eq!(routes, snapshot, "the input map is read-only");

        // Filtering returns clones: mutating one cannot reach the base map.
        let mut cloned = selector.filter_selected(&routes);
        if let Some(first) = cloned.first_mut() {
            first.peer = "rewritten".into();
        }
        assert_eq!(routes, snapshot);
    }

    #[test]
    fn known_and_exit_net_id_helpers_are_sorted_and_deduplicated() {
        let routes = vec![subnet("b", "r-1"), exit("a", "r-2", false), subnet("b", "r-3")];
        assert_eq!(known_net_ids(&routes), ids(&["a", "b"]));
        assert_eq!(exit_net_ids(&routes), ids(&["a"]));
        assert!(is_exit_route(&routes[1]));
        assert!(!is_exit_route(&routes[0]));
        assert!(known_net_ids(&[]).is_empty());
        assert!(exit_net_ids(&[]).is_empty());
    }

    /// The start-config adapter (`from_value`) must accept/reject exactly what
    /// the text entry point does — it is the SAME validator, only skipping the
    /// redundant second parse of bytes the strict reader already parsed.
    #[test]
    fn from_value_matches_from_json_on_the_parsed_document() {
        let text = "{\"selected_routes\":[\"a\"],\"deselected_routes\":[\"b\"],\"deselect_all\":false}";
        let parsed = parse_document(text).expect("strict reader");
        assert_eq!(RouteSelector::from_value(&parsed).expect("parsed value"), RouteSelector::from_json(text).expect("text"));

        // `null` is the fresh selector in both forms; a non-object is loud.
        assert_eq!(RouteSelector::from_value(&Json::Null).expect("null"), RouteSelector::new());
        assert!(RouteSelector::from_value(&Json::Arr(vec![])).is_err());
        assert!(RouteSelector::from_value(&Json::Bool(true)).is_err());
        assert!(RouteSelector::from_value(&Json::Obj(vec![(
            "deselect_all".to_string(),
            Json::Num(1)
        )]))
        .is_err());
    }
}
