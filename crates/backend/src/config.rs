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
    pub retention: RetentionPolicy,
    pub meta_fields: BTreeMap<String, MetaField>,
    pub entity_identity: EntityIdentityPolicy,
    pub entity_types: BTreeMap<String, EntityPolicy>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionPolicy {
    pub idempotency_seconds: u64,
    pub changes_seconds: u64,
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

    pub fn validate_value(&self, value: &Value) -> Result<(), String> {
        let validator = build_json_schema(&self.schema)?;
        validator.validate(value).map_err(|error| error.to_string())
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
    pub consistency: ConsistencyPolicy,
    pub idempotency: IdempotencyPolicy,
    pub conflict: ConflictPolicy,
    pub publication: PublicationPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsistencyPolicy {
    pub snapshot: SnapshotPolicy,
    pub acquire: AcquirePolicy,
    pub sessions: Vec<SessionPolicy>,
    pub commit: CommitConsistencyPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SnapshotPolicy {
    Request,
    Shared { key_by: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquirePolicy {
    pub allow_sources: Vec<ConsistencySource>,
    pub requirements: Vec<AcquireRequirement>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencySource {
    Authority,
    Replica,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AcquireRequirement {
    CausalAfter {
        token: String,
        #[serde(default)]
        optional: bool,
    },
    Linearizable {
        key_by: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPolicy {
    pub key_by: Vec<String>,
    pub guarantees: Vec<SessionGuarantee>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionGuarantee {
    MonotonicReads,
    ReadYourWrites,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitConsistencyPolicy {
    pub ordering: CommitOrderingPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommitOrderingPolicy {
    None,
    Serialize { key_by: Vec<String> },
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
    pub default_strategy: ConflictStrategy,
    pub strategy_from: String,
    pub base_versions: String,
    pub merge: Vec<ConflictMergeRule>,
    pub otherwise: ConflictFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    Merge,
    Mvcc,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConflictMergeRule {
    pub path: String,
    pub strategy: ConflictMergeStrategy,
    pub group_by: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictMergeStrategy {
    Automerge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictFallback {
    Mvcc,
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
    Marker { field: String, equals: Value },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchExpiryPolicy {
    Discard,
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
        if self.retention.idempotency_seconds == 0 {
            return Err(invalid(
                "retention.idempotency_seconds",
                "idempotency retention must be greater than zero",
            ));
        }
        if self.retention.changes_seconds == 0 {
            return Err(invalid(
                "retention.changes_seconds",
                "change retention must be greater than zero",
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
            if field.pointer == format!("/{}", crate::DEADLINE_META_FIELD) {
                return Err(invalid(
                    format!("meta_fields.{name}.pointer"),
                    "the protocol deadline field is reserved and cannot be configured as identity or policy metadata",
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
                validate_behavior(
                    &format!("{rule_root}.behavior"),
                    &rule.behavior,
                    &self.meta_fields,
                    &self.entity_identity.scope_by,
                )?;
            }
            validate_behavior(
                &format!("{root}.fallback"),
                &policy.fallback,
                &self.meta_fields,
                &self.entity_identity.scope_by,
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
    scope_by: &[String],
) -> Result<(), ConfigError> {
    validate_consistency(
        &format!("{root}.consistency"),
        &behavior.consistency,
        &behavior.publication,
        fields,
        scope_by,
    )?;

    if let IdempotencyPolicy::Keyed { key_by } = &behavior.idempotency {
        if key_by.is_empty() {
            return Err(invalid(
                format!("{root}.idempotency.key_by"),
                "keyed idempotency requires at least one identity field",
            ));
        }
        validate_control_key_aliases(
            &format!("{root}.idempotency.key_by"),
            key_by,
            fields,
            scope_by,
        )?;
    }
    validate_conflict(&format!("{root}.conflict"), &behavior.conflict, fields)?;

    if let PublicationPolicy::Batch {
        key_by,
        close_when,
        staging_ttl_ms,
        ..
    } = &behavior.publication
    {
        if key_by.is_empty() {
            return Err(invalid(
                format!("{root}.publication.key_by"),
                "batch publication requires at least one grouping field",
            ));
        }
        validate_control_key_aliases(
            &format!("{root}.publication.key_by"),
            key_by,
            fields,
            scope_by,
        )?;
        if *staging_ttl_ms == 0 {
            return Err(invalid(
                format!("{root}.publication.staging_ttl_ms"),
                "staging TTL must be greater than zero",
            ));
        }
        let BatchCloseCondition::Marker { field, .. } = close_when;
        validate_aliases(
            &format!("{root}.publication.close_when.field"),
            std::slice::from_ref(field),
            fields,
        )?;
    }

    Ok(())
}

fn validate_conflict(
    root: &str,
    conflict: &ConflictPolicy,
    fields: &BTreeMap<String, MetaField>,
) -> Result<(), ConfigError> {
    validate_aliases(
        &format!("{root}.strategy_from"),
        std::slice::from_ref(&conflict.strategy_from),
        fields,
    )?;
    validate_aliases(
        &format!("{root}.base_versions"),
        std::slice::from_ref(&conflict.base_versions),
        fields,
    )?;

    let mut paths = BTreeSet::new();
    for (index, rule) in conflict.merge.iter().enumerate() {
        let rule_root = format!("{root}.merge[{index}]");
        if !valid_json_pointer(&rule.path) {
            return Err(invalid(
                format!("{rule_root}.path"),
                "path must be a non-root RFC 6901 JSON Pointer relative to the entity value",
            ));
        }
        if paths.iter().any(|existing: &String| {
            nested_json_pointer(existing, &rule.path) || nested_json_pointer(&rule.path, existing)
        }) {
            return Err(invalid(
                format!("{rule_root}.path"),
                "merge paths must not duplicate or contain one another",
            ));
        }
        paths.insert(rule.path.clone());

        let mut group_paths = BTreeSet::new();
        for pointer in &rule.group_by {
            if !valid_json_pointer(pointer) {
                return Err(invalid(
                    format!("{rule_root}.group_by"),
                    "group paths must be non-root RFC 6901 JSON Pointers relative to the merged value",
                ));
            }
            if !group_paths.insert(pointer) {
                return Err(invalid(
                    format!("{rule_root}.group_by"),
                    format!("duplicate group path {pointer:?}"),
                ));
            }
        }
    }

    Ok(())
}

fn nested_json_pointer(parent: &str, child: &str) -> bool {
    parent == child
        || child
            .strip_prefix(parent)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn validate_consistency(
    root: &str,
    consistency: &ConsistencyPolicy,
    publication: &PublicationPolicy,
    fields: &BTreeMap<String, MetaField>,
    scope_by: &[String],
) -> Result<(), ConfigError> {
    match (&consistency.snapshot, publication) {
        (SnapshotPolicy::Request, PublicationPolicy::Immediate) => {}
        (
            SnapshotPolicy::Shared {
                key_by: snapshot_key,
            },
            PublicationPolicy::Batch {
                key_by: publication_key,
                ..
            },
        ) => {
            if snapshot_key.is_empty() {
                return Err(invalid(
                    format!("{root}.snapshot.key_by"),
                    "a shared snapshot requires at least one identity field",
                ));
            }
            validate_control_key_aliases(
                &format!("{root}.snapshot.key_by"),
                snapshot_key,
                fields,
                scope_by,
            )?;
            if snapshot_key.iter().collect::<BTreeSet<_>>()
                != publication_key.iter().collect::<BTreeSet<_>>()
            {
                return Err(invalid(
                    format!("{root}.snapshot.key_by"),
                    "shared snapshot and batch publication must use the same key fields",
                ));
            }
        }
        (SnapshotPolicy::Shared { .. }, PublicationPolicy::Immediate) => {
            return Err(invalid(
                format!("{root}.snapshot.mode"),
                "a shared snapshot requires batch publication",
            ));
        }
        (SnapshotPolicy::Request, PublicationPolicy::Batch { .. }) => {
            return Err(invalid(
                format!("{root}.snapshot.mode"),
                "batch publication requires a shared snapshot",
            ));
        }
    }

    if consistency.acquire.allow_sources.is_empty() {
        return Err(invalid(
            format!("{root}.acquire.allow_sources"),
            "at least one snapshot source is required",
        ));
    }
    let allowed_sources = consistency
        .acquire
        .allow_sources
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if allowed_sources.len() != consistency.acquire.allow_sources.len() {
        return Err(invalid(
            format!("{root}.acquire.allow_sources"),
            "snapshot sources must be unique",
        ));
    }

    let mut linearizable_requirements = 0;
    for (index, requirement) in consistency.acquire.requirements.iter().enumerate() {
        let requirement_root = format!("{root}.acquire.requirements[{index}]");
        match requirement {
            AcquireRequirement::CausalAfter { token, .. } => validate_aliases(
                &format!("{requirement_root}.token"),
                std::slice::from_ref(token),
                fields,
            )?,
            AcquireRequirement::Linearizable { key_by } => {
                linearizable_requirements += 1;
                validate_control_key_aliases(
                    &format!("{requirement_root}.key_by"),
                    key_by,
                    fields,
                    scope_by,
                )?;
            }
        }
    }
    if linearizable_requirements > 1 {
        return Err(invalid(
            format!("{root}.acquire.requirements"),
            "only one linearization domain may be configured",
        ));
    }
    if linearizable_requirements == 1 && !allowed_sources.contains(&ConsistencySource::Authority) {
        return Err(invalid(
            format!("{root}.acquire.allow_sources"),
            "linearizable acquisition requires authority to be an allowed source",
        ));
    }

    let mut session_keys = BTreeSet::new();
    for (index, session) in consistency.sessions.iter().enumerate() {
        let session_root = format!("{root}.sessions[{index}]");
        if session.key_by.is_empty() {
            return Err(invalid(
                format!("{session_root}.key_by"),
                "a session requires at least one identity field",
            ));
        }
        validate_control_key_aliases(
            &format!("{session_root}.key_by"),
            &session.key_by,
            fields,
            scope_by,
        )?;
        let mut normalized_key = session.key_by.clone();
        normalized_key.sort();
        if !session_keys.insert(normalized_key) {
            return Err(invalid(
                format!("{session_root}.key_by"),
                "a session identity may be declared only once; combine its guarantees",
            ));
        }
        if session.guarantees.is_empty() {
            return Err(invalid(
                format!("{session_root}.guarantees"),
                "a session requires at least one guarantee",
            ));
        }
        if session.guarantees.iter().collect::<BTreeSet<_>>().len() != session.guarantees.len() {
            return Err(invalid(
                format!("{session_root}.guarantees"),
                "session guarantees must be unique",
            ));
        }
    }

    if matches!(publication, PublicationPolicy::Batch { .. })
        && (!consistency.sessions.is_empty()
            || consistency
                .acquire
                .requirements
                .iter()
                .any(|requirement| matches!(requirement, AcquireRequirement::CausalAfter { .. })))
    {
        return Err(invalid(
            root,
            "causal/session consistency currently requires immediate publication",
        ));
    }

    if let CommitOrderingPolicy::Serialize { key_by } = &consistency.commit.ordering {
        validate_control_key_aliases(
            &format!("{root}.commit.ordering.key_by"),
            key_by,
            fields,
            scope_by,
        )?;
    }

    Ok(())
}

fn validate_control_key_aliases(
    path: &str,
    aliases: &[String],
    fields: &BTreeMap<String, MetaField>,
    scope_by: &[String],
) -> Result<(), ConfigError> {
    validate_aliases(path, aliases, fields)?;
    let scope_fields = scope_by.iter().collect::<BTreeSet<_>>();
    if let Some(alias) = aliases.iter().find(|alias| scope_fields.contains(alias)) {
        return Err(invalid(
            path,
            format!(
                "metadata field {alias:?} is already an implicit scope prefix and must not be repeated"
            ),
        ));
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
