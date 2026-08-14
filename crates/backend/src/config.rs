use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BackendConfig {
    pub version: u16,
    pub entity_types: BTreeMap<String, EntityPolicy>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityPolicy {
    pub schema: Value,
    pub fields: BTreeMap<String, FieldSelector>,
    pub consistency: ConsistencyPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSelector {
    pub pointer: String,
    #[serde(default)]
    pub required: bool,
}

impl FieldSelector {
    pub fn select<'a>(&self, operation: &'a Value) -> Option<&'a Value> {
        operation.pointer(&self.pointer)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsistencyPolicy {
    #[serde(default)]
    pub rules: Vec<ConsistencyRule>,
    pub fallback: ConsistencyBehavior,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsistencyRule {
    pub name: String,
    #[serde(default)]
    pub when_all_present: Vec<String>,
    #[serde(flatten)]
    pub behavior: ConsistencyBehavior,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConsistencyBehavior {
    pub identity: Vec<String>,
    #[serde(default)]
    pub group_by: Vec<String>,
    pub baseline: BaselinePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<BatchPolicy>,
    pub conflict: ConflictPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselinePolicy {
    pub consistency: ConfiguredConsistency,
    pub source: BaselineSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causal_field: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfiguredConsistency {
    Causal,
    Eventual,
    Linearizable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineSource {
    Any,
    Authority,
    Replica,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchPolicy {
    pub visibility: BatchVisibility,
    pub close_when: BatchCloseCondition,
    pub staging_ttl_ms: u64,
    pub on_expire: BatchExpiryPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchVisibility {
    OnClose,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchExpiryPolicy {
    Discard,
    Publish,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchCloseCondition {
    Marker {
        field: String,
        equals: Value,
    },
    ExpectedCount {
        field: String,
    },
    TimeWindow {
        field: String,
        size_ms: u64,
        #[serde(default)]
        allowed_lateness_ms: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictPolicy {
    pub strategy: ConflictStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_versions_field: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    PreserveHeads,
    Reject,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid backend configuration JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid backend configuration at {path}: {message}")]
    Invalid { path: String, message: String },
}

impl BackendConfig {
    pub fn from_json(input: &str) -> Result<Self, ConfigError> {
        let config: Self = serde_json::from_str(input)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != 1 {
            return Err(invalid(
                "version",
                "only configuration version 1 is supported",
            ));
        }
        if self.entity_types.is_empty() {
            return Err(invalid(
                "entity_types",
                "at least one entity type is required",
            ));
        }

        for (entity_type, policy) in &self.entity_types {
            let root = format!("entity_types.{entity_type}");
            if entity_type.is_empty() {
                return Err(invalid(
                    "entity_types",
                    "entity type names must not be empty",
                ));
            }
            if !policy.schema.is_object() && !policy.schema.is_boolean() {
                return Err(invalid(
                    format!("{root}.schema"),
                    "schema must be a JSON Schema object or boolean",
                ));
            }
            for (name, selector) in &policy.fields {
                if name.is_empty() {
                    return Err(invalid(
                        format!("{root}.fields"),
                        "field names must not be empty",
                    ));
                }
                if !valid_json_pointer(&selector.pointer) {
                    return Err(invalid(
                        format!("{root}.fields.{name}.pointer"),
                        "pointer must be an RFC 6901 JSON Pointer",
                    ));
                }
            }

            let mut rule_names = BTreeSet::new();
            for (index, rule) in policy.consistency.rules.iter().enumerate() {
                let rule_root = format!("{root}.consistency.rules[{index}]");
                if rule.name.is_empty() || !rule_names.insert(&rule.name) {
                    return Err(invalid(
                        format!("{rule_root}.name"),
                        "rule names must be non-empty and unique",
                    ));
                }
                validate_aliases(
                    &rule_root,
                    "when_all_present",
                    &rule.when_all_present,
                    &policy.fields,
                )?;
                validate_behavior(&rule_root, &rule.behavior, &policy.fields)?;
                validate_rule_availability(
                    &rule_root,
                    &rule.behavior,
                    &rule.when_all_present,
                    &policy.fields,
                )?;
            }
            validate_behavior(
                &format!("{root}.consistency.fallback"),
                &policy.consistency.fallback,
                &policy.fields,
            )?;
            validate_rule_availability(
                &format!("{root}.consistency.fallback"),
                &policy.consistency.fallback,
                &[],
                &policy.fields,
            )?;
        }

        Ok(())
    }
}

fn validate_rule_availability(
    root: &str,
    behavior: &ConsistencyBehavior,
    matched_fields: &[String],
    fields: &BTreeMap<String, FieldSelector>,
) -> Result<(), ConfigError> {
    let mut needed = behavior.identity.clone();
    needed.extend(behavior.group_by.iter().cloned());
    if let Some(field) = &behavior.baseline.at_field {
        needed.push(field.clone());
    }

    for alias in needed {
        if !fields[&alias].required && !matched_fields.contains(&alias) {
            return Err(invalid(
                root,
                format!("field alias {alias:?} must be required or listed in when_all_present"),
            ));
        }
    }
    Ok(())
}

fn validate_behavior(
    root: &str,
    behavior: &ConsistencyBehavior,
    fields: &BTreeMap<String, FieldSelector>,
) -> Result<(), ConfigError> {
    if behavior.identity.is_empty() {
        return Err(invalid(
            format!("{root}.identity"),
            "at least one identity field is required",
        ));
    }
    validate_aliases(root, "identity", &behavior.identity, fields)?;
    validate_aliases(root, "group_by", &behavior.group_by, fields)?;
    if let Some(field) = &behavior.baseline.at_field {
        validate_aliases(
            root,
            "baseline.at_field",
            std::slice::from_ref(field),
            fields,
        )?;
    }
    if let Some(field) = &behavior.baseline.causal_field {
        validate_aliases(
            root,
            "baseline.causal_field",
            std::slice::from_ref(field),
            fields,
        )?;
    }
    if let Some(field) = &behavior.conflict.base_versions_field {
        validate_aliases(
            root,
            "conflict.base_versions_field",
            std::slice::from_ref(field),
            fields,
        )?;
    }
    if let Some(batch) = &behavior.batch {
        if batch.staging_ttl_ms == 0 {
            return Err(invalid(
                format!("{root}.batch.staging_ttl_ms"),
                "staging TTL must be greater than zero",
            ));
        }
        if behavior.group_by.is_empty() {
            return Err(invalid(
                format!("{root}.group_by"),
                "batch rules require at least one grouping field",
            ));
        }
        match &batch.close_when {
            BatchCloseCondition::Marker { field, .. }
            | BatchCloseCondition::ExpectedCount { field }
            | BatchCloseCondition::TimeWindow { field, .. } => validate_aliases(
                root,
                "batch.close_when.field",
                std::slice::from_ref(field),
                fields,
            )?,
        }
        if let BatchCloseCondition::TimeWindow { size_ms: 0, .. } = &batch.close_when {
            return Err(invalid(
                format!("{root}.batch.close_when.size_ms"),
                "time window size must be greater than zero",
            ));
        }
    }
    Ok(())
}

fn validate_aliases(
    root: &str,
    property: &str,
    aliases: &[String],
    fields: &BTreeMap<String, FieldSelector>,
) -> Result<(), ConfigError> {
    let mut seen = BTreeSet::new();
    for alias in aliases {
        if !fields.contains_key(alias) {
            return Err(invalid(
                format!("{root}.{property}"),
                format!("unknown field alias {alias:?}"),
            ));
        }
        if !seen.insert(alias) {
            return Err(invalid(
                format!("{root}.{property}"),
                format!("duplicate field alias {alias:?}"),
            ));
        }
    }
    Ok(())
}

fn valid_json_pointer(pointer: &str) -> bool {
    if !pointer.starts_with('/') {
        return false;
    }
    let bytes = pointer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            index += 1;
            if index == bytes.len() || !matches!(bytes[index], b'0' | b'1') {
                return false;
            }
        }
        index += 1;
    }
    true
}

fn invalid(path: impl Into<String>, message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        path: path.into(),
        message: message.into(),
    }
}
