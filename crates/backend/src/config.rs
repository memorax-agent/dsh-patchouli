use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::fact::BuiltinFactSchemaRetriever;

const CONFIG_SCHEMA: &str = include_str!("../../../config/patchouli.schema.json");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendConfig {
    #[serde(rename = "$schema")]
    pub schema_uri: String,
    pub version: u16,
    pub meta_fields: BTreeMap<String, MetaField>,
    pub entity_identity: EntityIdentityPolicy,
    pub entity_types: BTreeMap<String, EntityPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityIdentityPolicy {
    pub scope_by: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaField {
    pub pointer: String,
    pub schema: Value,
}

impl MetaField {
    pub fn select<'a>(&self, meta: &'a Value) -> Option<&'a Value> {
        meta.pointer(&self.pointer)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityPolicy {
    pub value_schema: Value,
    pub rules: Vec<PolicyRule>,
    pub fallback: Behavior,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRule {
    pub name: String,
    pub when: RuleMatch,
    pub behavior: Behavior,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleMatch {
    pub all_present: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Behavior {
    pub baseline: BaselinePolicy,
    pub idempotency: IdempotencyPolicy,
    pub conflict: ConflictPolicy,
    pub publication: PublicationPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum BaselinePolicy {
    Request {
        consistency: ConfiguredConsistency,
        source: BaselineSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        causal_token: Option<String>,
    },
    Shared {
        consistency: ConfiguredConsistency,
        source: BaselineSource,
        key_by: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        causal_token: Option<String>,
    },
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum IdempotencyPolicy {
    Disabled,
    Keyed { key_by: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConflictPolicy {
    pub strategy: ConflictStrategy,
    pub base_versions: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    PreserveHeads,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicationPolicy {
    Immediate,
    Batch {
        key_by: Vec<String>,
        close_when: BatchCloseCondition,
        staging_ttl_ms: u64,
        on_expire: BatchExpiryPolicy,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
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
#[serde(rename_all = "snake_case")]
pub enum BatchExpiryPolicy {
    Discard,
    Publish,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid backend configuration JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("backend configuration does not match its schema at {path}: {message}")]
    Schema { path: String, message: String },
    #[error("invalid backend configuration at {path}: {message}")]
    Invalid { path: String, message: String },
}

impl BackendConfig {
    pub fn from_json(input: &str) -> Result<Self, ConfigError> {
        let document: Value = serde_json::from_str(input)?;
        let schema: Value = serde_json::from_str(CONFIG_SCHEMA)
            .expect("the embedded backend configuration schema must be valid JSON");

        if let Err(error) = jsonschema::draft202012::validate(&schema, &document) {
            return Err(ConfigError::Schema {
                path: error.instance_path().to_string(),
                message: error.to_string(),
            });
        }

        let config: Self = serde_json::from_value(document)?;
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
        if self.meta_fields.is_empty() {
            return Err(invalid(
                "meta_fields",
                "at least one metadata field is required",
            ));
        }
        if self.entity_types.is_empty() {
            return Err(invalid(
                "entity_types",
                "at least one entity type is required",
            ));
        }

        for (name, field) in &self.meta_fields {
            if name.is_empty() {
                return Err(invalid("meta_fields", "field aliases must not be empty"));
            }
            if !valid_json_pointer(&field.pointer) {
                return Err(invalid(
                    format!("meta_fields.{name}.pointer"),
                    "pointer must be a non-root RFC 6901 JSON Pointer relative to meta",
                ));
            }
            validate_json_schema(&format!("meta_fields.{name}.schema"), &field.schema)?;
        }

        validate_aliases(
            "entity_identity.scope_by",
            &self.entity_identity.scope_by,
            &self.meta_fields,
        )?;

        for (entity_type, policy) in &self.entity_types {
            let root = format!("entity_types.{entity_type}");
            if entity_type.is_empty() {
                return Err(invalid(
                    "entity_types",
                    "entity type names must not be empty",
                ));
            }
            validate_json_schema(&format!("{root}.value_schema"), &policy.value_schema)?;

            let mut rule_names = BTreeSet::new();
            for (index, rule) in policy.rules.iter().enumerate() {
                let rule_root = format!("{root}.rules[{index}]");
                if !rule_names.insert(&rule.name) {
                    return Err(invalid(
                        format!("{rule_root}.name"),
                        "rule names must be unique within an entity type",
                    ));
                }
                if rule.when.all_present.is_empty() {
                    return Err(invalid(
                        format!("{rule_root}.when.all_present"),
                        "a rule must test at least one metadata field",
                    ));
                }
                validate_aliases(
                    &format!("{rule_root}.when.all_present"),
                    &rule.when.all_present,
                    &self.meta_fields,
                )?;
                validate_behavior(&rule_root, &rule.behavior, &self.meta_fields)?;
            }
            validate_behavior(
                &format!("{root}.fallback"),
                &policy.fallback,
                &self.meta_fields,
            )?;
        }

        Ok(())
    }

    pub fn validate_entity_value(
        &self,
        entity_type: &str,
        value: &Value,
    ) -> Result<(), ConfigError> {
        let policy = self.entity_types.get(entity_type).ok_or_else(|| {
            invalid(
                format!("entity_types.{entity_type}"),
                "entity type is not configured",
            )
        })?;
        let validator = build_json_schema(&policy.value_schema).map_err(|message| {
            invalid(format!("entity_types.{entity_type}.value_schema"), message)
        })?;
        validator.validate(value).map_err(|error| {
            invalid(
                format!("entity_types.{entity_type}.value{}", error.instance_path()),
                error.to_string(),
            )
        })
    }
}

fn validate_behavior(
    root: &str,
    behavior: &Behavior,
    fields: &BTreeMap<String, MetaField>,
) -> Result<(), ConfigError> {
    let (at, causal_token) = match &behavior.baseline {
        BaselinePolicy::Request {
            at, causal_token, ..
        } => (at, causal_token),
        BaselinePolicy::Shared {
            key_by,
            at,
            causal_token,
            ..
        } => {
            if key_by.is_empty() {
                return Err(invalid(
                    format!("{root}.behavior.baseline.key_by"),
                    "shared baseline requires at least one identity field",
                ));
            }
            validate_aliases(&format!("{root}.behavior.baseline.key_by"), key_by, fields)?;
            (at, causal_token)
        }
    };
    validate_optional_alias(&format!("{root}.behavior.baseline.at"), at.as_ref(), fields)?;
    validate_optional_alias(
        &format!("{root}.behavior.baseline.causal_token"),
        causal_token.as_ref(),
        fields,
    )?;

    if let IdempotencyPolicy::Keyed { key_by } = &behavior.idempotency {
        if key_by.is_empty() {
            return Err(invalid(
                format!("{root}.behavior.idempotency.key_by"),
                "keyed idempotency requires at least one identity field",
            ));
        }
        validate_aliases(
            &format!("{root}.behavior.idempotency.key_by"),
            key_by,
            fields,
        )?;
    }
    validate_aliases(
        &format!("{root}.behavior.conflict.base_versions"),
        std::slice::from_ref(&behavior.conflict.base_versions),
        fields,
    )?;

    if let PublicationPolicy::Batch {
        key_by,
        close_when,
        staging_ttl_ms,
        ..
    } = &behavior.publication
    {
        if key_by.is_empty() {
            return Err(invalid(
                format!("{root}.behavior.publication.key_by"),
                "batch publication requires at least one grouping field",
            ));
        }
        validate_aliases(
            &format!("{root}.behavior.publication.key_by"),
            key_by,
            fields,
        )?;
        if *staging_ttl_ms == 0 {
            return Err(invalid(
                format!("{root}.behavior.publication.staging_ttl_ms"),
                "staging TTL must be greater than zero",
            ));
        }
        let (field, size_ms) = match close_when {
            BatchCloseCondition::Marker { field, .. }
            | BatchCloseCondition::ExpectedCount { field } => (field, None),
            BatchCloseCondition::TimeWindow { field, size_ms, .. } => (field, Some(*size_ms)),
        };
        validate_aliases(
            &format!("{root}.behavior.publication.close_when.field"),
            std::slice::from_ref(field),
            fields,
        )?;
        if size_ms == Some(0) {
            return Err(invalid(
                format!("{root}.behavior.publication.close_when.size_ms"),
                "time window size must be greater than zero",
            ));
        }
    }

    Ok(())
}

fn validate_optional_alias(
    path: &str,
    alias: Option<&String>,
    fields: &BTreeMap<String, MetaField>,
) -> Result<(), ConfigError> {
    if let Some(alias) = alias {
        validate_aliases(path, std::slice::from_ref(alias), fields)?;
    }
    Ok(())
}

fn validate_aliases(
    path: &str,
    aliases: &[String],
    fields: &BTreeMap<String, MetaField>,
) -> Result<(), ConfigError> {
    let mut seen = BTreeSet::new();
    for alias in aliases {
        if !fields.contains_key(alias) {
            return Err(invalid(path, format!("unknown metadata field {alias:?}")));
        }
        if !seen.insert(alias) {
            return Err(invalid(path, format!("duplicate metadata field {alias:?}")));
        }
    }
    Ok(())
}

fn validate_json_schema(path: &str, schema: &Value) -> Result<(), ConfigError> {
    jsonschema::meta::validate(schema)
        .map_err(|error| invalid(path, format!("value must be a valid JSON Schema: {error}")))?;
    build_json_schema(schema)
        .map(|_| ())
        .map_err(|error| invalid(path, format!("value schema cannot be resolved: {error}")))
}

fn build_json_schema(schema: &Value) -> Result<jsonschema::Validator, String> {
    jsonschema::draft202012::options()
        .with_retriever(BuiltinFactSchemaRetriever)
        .build(schema)
        .map_err(|error| error.to_string())
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
