use std::collections::BTreeMap;

use serde_json::Value;
use thiserror::Error;

use crate::{BackendConfig, BaselinePolicy, Behavior, IdempotencyPolicy, PublicationPolicy};

#[derive(Clone, Debug)]
pub struct PolicySelector {
    config: BackendConfig,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicySelection {
    pub rule: Option<String>,
    pub fields: BTreeMap<String, Value>,
    pub scope: BTreeMap<String, Value>,
    pub baseline_key: Option<BTreeMap<String, Value>>,
    pub idempotency_key: Option<BTreeMap<String, Value>>,
    pub publication_key: Option<BTreeMap<String, Value>>,
    pub behavior: Behavior,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("entity type {0:?} is not configured")]
    UnknownEntityType(String),
    #[error("required metadata field {field:?} is absent for entity type {entity_type:?}")]
    MissingField { entity_type: String, field: String },
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
                selector
                    .select(meta)
                    .cloned()
                    .map(|value| (name.clone(), value))
            })
            .collect::<BTreeMap<_, _>>();

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
        let baseline_key = match &behavior.baseline {
            BaselinePolicy::Request { .. } => None,
            BaselinePolicy::Shared { key_by, .. } => {
                Some(select_key(entity_type, &fields, key_by)?)
            }
        };
        let idempotency_key = match &behavior.idempotency {
            IdempotencyPolicy::Disabled => None,
            IdempotencyPolicy::Keyed { key_by } => Some(select_key(entity_type, &fields, key_by)?),
        };
        let publication_key = match &behavior.publication {
            PublicationPolicy::Immediate => None,
            PublicationPolicy::Batch { key_by, .. } => {
                Some(select_key(entity_type, &fields, key_by)?)
            }
        };

        Ok(PolicySelection {
            rule,
            fields,
            scope,
            baseline_key,
            idempotency_key,
            publication_key,
            behavior: behavior.clone(),
        })
    }
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
                .ok_or_else(|| PolicyError::MissingField {
                    entity_type: entity_type.to_owned(),
                    field: name.clone(),
                })
        })
        .collect()
}
