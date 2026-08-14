use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use thiserror::Error;

use crate::{
    AcquireRequirement, BackendConfig, Behavior, CommitOrderingPolicy, ConsistencySource,
    IdempotencyPolicy, PublicationPolicy, SessionGuarantee, SnapshotPolicy,
};

#[derive(Clone, Debug)]
pub struct PolicySelector {
    config: BackendConfig,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicySelection {
    pub rule: Option<String>,
    pub fields: BTreeMap<String, Value>,
    pub scope: BTreeMap<String, Value>,
    pub consistency: ConsistencyPlan,
    pub idempotency_key: Option<ControlKey>,
    pub publication_key: Option<ControlKey>,
    pub behavior: Behavior,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlKey {
    pub scope: BTreeMap<String, Value>,
    pub fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConsistencyPlan {
    pub snapshot_key: Option<ControlKey>,
    pub allowed_sources: BTreeSet<ConsistencySource>,
    pub causal: Vec<CausalConsistencyPlan>,
    pub linearization_key: Option<ControlKey>,
    pub sessions: Vec<SessionConsistencyPlan>,
    pub commit_ordering_key: Option<ControlKey>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CausalConsistencyPlan {
    pub field: String,
    pub minimum: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionConsistencyPlan {
    pub key: ControlKey,
    pub guarantees: BTreeSet<SessionGuarantee>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("entity type {0:?} is not configured")]
    UnknownEntityType(String),
    #[error("required metadata field {field:?} is absent for entity type {entity_type:?}")]
    MissingField { entity_type: String, field: String },
    #[error("metadata field {field:?} is invalid: {message}")]
    InvalidField { field: String, message: String },
}

impl PolicySelector {
    pub fn new(config: BackendConfig) -> Self {
        Self { config }
    }

    pub fn select(&self, entity_type: &str, meta: &Value) -> Result<PolicySelection, PolicyError> {
        let policy = self
            .config
            .entity_types
            .get(entity_type)
            .ok_or_else(|| PolicyError::UnknownEntityType(entity_type.to_owned()))?;

        let fields = self
            .config
            .meta_fields
            .iter()
            .filter_map(|(name, selector)| {
                selector.select(meta).map(|value| (name, selector, value))
            })
            .map(|(name, selector, value)| {
                selector
                    .validate_value(value)
                    .map(|()| (name.clone(), value.clone()))
                    .map_err(|message| PolicyError::InvalidField {
                        field: name.clone(),
                        message,
                    })
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let matched_rule = policy.rules.iter().find(|rule| {
            rule.when
                .all_present
                .iter()
                .all(|field| fields.contains_key(field))
        });
        let (rule, behavior) = matched_rule.map_or_else(
            || (None, &policy.fallback),
            |rule| (Some(rule.name.clone()), &rule.behavior),
        );

        let scope = select_key(entity_type, &fields, &self.config.entity_identity.scope_by)?;
        let consistency = build_consistency_plan(entity_type, &fields, &scope, behavior)?;
        let idempotency_key = match &behavior.idempotency {
            IdempotencyPolicy::Disabled => None,
            IdempotencyPolicy::Keyed { key_by } => {
                Some(control_key(entity_type, &fields, &scope, key_by)?)
            }
        };
        let publication_key = match &behavior.publication {
            PublicationPolicy::Immediate => None,
            PublicationPolicy::Batch { key_by, .. } => {
                Some(control_key(entity_type, &fields, &scope, key_by)?)
            }
        };

        Ok(PolicySelection {
            rule,
            fields,
            scope,
            consistency,
            idempotency_key,
            publication_key,
            behavior: behavior.clone(),
        })
    }
}

fn build_consistency_plan(
    entity_type: &str,
    fields: &BTreeMap<String, Value>,
    scope: &BTreeMap<String, Value>,
    behavior: &Behavior,
) -> Result<ConsistencyPlan, PolicyError> {
    let snapshot_key = match &behavior.consistency.snapshot {
        SnapshotPolicy::Request => None,
        SnapshotPolicy::Shared { key_by } => Some(control_key(entity_type, fields, scope, key_by)?),
    };
    let mut allowed_sources: BTreeSet<ConsistencySource> = behavior
        .consistency
        .acquire
        .allow_sources
        .iter()
        .copied()
        .collect();
    let mut causal = Vec::new();
    let mut linearization_key = None;
    for requirement in &behavior.consistency.acquire.requirements {
        match requirement {
            AcquireRequirement::CausalAfter { token, optional } => {
                let minimum = match fields.get(token) {
                    Some(value) => Some(value.clone()),
                    None if *optional => None,
                    None => return Err(missing_field(entity_type, token)),
                };
                causal.push(CausalConsistencyPlan {
                    field: token.clone(),
                    minimum,
                });
            }
            AcquireRequirement::Linearizable { key_by } => {
                allowed_sources.retain(|source| *source == ConsistencySource::Authority);
                linearization_key = Some(control_key(entity_type, fields, scope, key_by)?);
            }
        }
    }
    let sessions = behavior
        .consistency
        .sessions
        .iter()
        .map(|session| {
            Ok(SessionConsistencyPlan {
                key: control_key(entity_type, fields, scope, &session.key_by)?,
                guarantees: session.guarantees.iter().copied().collect(),
            })
        })
        .collect::<Result<_, PolicyError>>()?;
    let commit_ordering_key = match &behavior.consistency.commit.ordering {
        CommitOrderingPolicy::None => None,
        CommitOrderingPolicy::Serialize { key_by } => {
            Some(control_key(entity_type, fields, scope, key_by)?)
        }
    };

    Ok(ConsistencyPlan {
        snapshot_key,
        allowed_sources,
        causal,
        linearization_key,
        sessions,
        commit_ordering_key,
    })
}

fn control_key(
    entity_type: &str,
    fields: &BTreeMap<String, Value>,
    scope: &BTreeMap<String, Value>,
    names: &[String],
) -> Result<ControlKey, PolicyError> {
    Ok(ControlKey {
        scope: scope.clone(),
        fields: select_key(entity_type, fields, names)?,
    })
}

fn select_key(
    entity_type: &str,
    fields: &BTreeMap<String, Value>,
    names: &[String],
) -> Result<BTreeMap<String, Value>, PolicyError> {
    names
        .iter()
        .map(|name| {
            fields
                .get(name)
                .cloned()
                .map(|value| (name.clone(), value))
                .ok_or_else(|| missing_field(entity_type, name))
        })
        .collect()
}

fn missing_field(entity_type: &str, field: &str) -> PolicyError {
    PolicyError::MissingField {
        entity_type: entity_type.to_owned(),
        field: field.to_owned(),
    }
}
