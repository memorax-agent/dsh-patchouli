use std::collections::BTreeMap;

use serde_json::Value;
use thiserror::Error;

use crate::{BackendConfig, ConsistencyBehavior};

#[derive(Clone, Debug)]
pub struct PolicyEngine {
    config: BackendConfig,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyDecision {
    pub rule: Option<String>,
    pub fields: BTreeMap<String, Value>,
    pub identity: BTreeMap<String, Value>,
    pub group: BTreeMap<String, Value>,
    pub behavior: ConsistencyBehavior,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("entity type {0:?} is not configured")]
    UnknownEntityType(String),
    #[error("required configured field {field:?} is absent for entity type {entity_type:?}")]
    MissingField { entity_type: String, field: String },
}

impl PolicyEngine {
    pub fn new(config: BackendConfig) -> Self {
        Self { config }
    }

    pub fn decide(
        &self,
        entity_type: &str,
        operation: &Value,
    ) -> Result<PolicyDecision, PolicyError> {
        let policy = self
            .config
            .entity_types
            .get(entity_type)
            .ok_or_else(|| PolicyError::UnknownEntityType(entity_type.to_owned()))?;

        let mut fields = BTreeMap::new();
        for (name, selector) in &policy.fields {
            match selector.select(operation) {
                Some(value) => {
                    fields.insert(name.clone(), value.clone());
                }
                None if selector.required => {
                    return Err(PolicyError::MissingField {
                        entity_type: entity_type.to_owned(),
                        field: name.clone(),
                    });
                }
                None => {}
            }
        }

        let matched_rule = policy.consistency.rules.iter().find(|rule| {
            rule.when_all_present
                .iter()
                .all(|field| fields.contains_key(field))
        });
        let (rule, behavior) = matched_rule.map_or_else(
            || (None, &policy.consistency.fallback),
            |rule| (Some(rule.name.clone()), &rule.behavior),
        );

        let identity = select_fields(entity_type, &fields, &behavior.identity)?;
        let group = select_fields(entity_type, &fields, &behavior.group_by)?;

        Ok(PolicyDecision {
            rule,
            fields,
            identity,
            group,
            behavior: behavior.clone(),
        })
    }
}

fn select_fields(
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
