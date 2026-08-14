use std::collections::BTreeMap;

use automerge::{ActorId, AutoCommit};
use autosurgeon::{
    Hydrate, HydrateError, Reconcile, Reconciler, Text, hydrate, hydrate_prop, reconcile,
    reconcile::{MapReconciler, NoKey},
};
use serde_json::{Map, Number, Value};
use thiserror::Error;

use crate::{ConflictFallback, ConflictMergeRule, ConflictPlan, ConflictStrategy};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrdtDocument {
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrdtChange {
    pub hash: String,
    pub parents: Vec<String>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConflictResolution {
    pub variants: Vec<Value>,
    pub merged_fields: Vec<MergedField>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MergedField {
    pub path: String,
    pub group: Vec<Value>,
    pub document: CrdtDocument,
}

#[derive(Debug, Error)]
pub enum ConflictError {
    #[error("concurrent versions are rejected by the selected conflict strategy")]
    Rejected,
    #[error("at least one Automerge document is required")]
    EmptyMerge,
    #[error("Automerge document error: {0}")]
    Automerge(#[from] automerge::AutomergeError),
    #[error("Automerge reconciliation error: {0}")]
    Reconcile(#[from] autosurgeon::ReconcileError),
    #[error("Automerge hydration error: {0}")]
    Hydrate(#[from] HydrateError),
}

impl CrdtDocument {
    pub fn from_json(value: &Value) -> Result<Self, ConflictError> {
        let mut document = AutoCommit::new();
        reconcile(&mut document, CrdtEnvelope::from_json(value))?;
        Ok(Self {
            bytes: document.save(),
        })
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ConflictError> {
        AutoCommit::load(&bytes)?;
        Ok(Self { bytes })
    }

    pub fn change(&self, value: &Value) -> Result<Self, ConflictError> {
        let mut document = self.load()?;
        let mut envelope: CrdtEnvelope = hydrate(&document)?;
        envelope.value.update(value);
        document.set_actor(ActorId::random());
        reconcile(&mut document, &envelope)?;
        Ok(Self {
            bytes: document.save(),
        })
    }

    pub fn merge(documents: &[Self]) -> Result<Self, ConflictError> {
        let Some(first) = documents.first() else {
            return Err(ConflictError::EmptyMerge);
        };
        let mut merged = first.load()?;
        for document in &documents[1..] {
            let mut other = document.load()?;
            merged.merge(&mut other)?;
        }
        Ok(Self {
            bytes: merged.save(),
        })
    }

    pub fn json(&self) -> Result<Value, ConflictError> {
        let document = self.load()?;
        let envelope: CrdtEnvelope = hydrate(&document)?;
        Ok(envelope.value.into_json())
    }

    pub fn heads(&self) -> Result<Vec<String>, ConflictError> {
        let mut document = self.load()?;
        Ok(document
            .get_heads()
            .into_iter()
            .map(|hash| hash.to_string())
            .collect())
    }

    pub fn changes(&self) -> Result<Vec<CrdtChange>, ConflictError> {
        let mut document = self.load()?;
        Ok(document
            .get_changes(&[])
            .into_iter()
            .map(|change| CrdtChange {
                hash: change.hash().to_string(),
                parents: change.deps().iter().map(ToString::to_string).collect(),
                bytes: change.raw_bytes().to_vec(),
            })
            .collect())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn load(&self) -> Result<AutoCommit, ConflictError> {
        Ok(AutoCommit::load(&self.bytes)?)
    }
}

pub fn resolve_conflict(
    plan: &ConflictPlan,
    base: &Value,
    candidates: &[Value],
) -> Result<ConflictResolution, ConflictError> {
    if candidates.len() <= 1 {
        return Ok(ConflictResolution {
            variants: candidates.to_vec(),
            merged_fields: Vec::new(),
        });
    }

    match plan.strategy {
        ConflictStrategy::Reject => Err(ConflictError::Rejected),
        ConflictStrategy::Mvcc => Ok(ConflictResolution {
            variants: unique_values(candidates),
            merged_fields: Vec::new(),
        }),
        ConflictStrategy::Merge => merge_candidates(plan, base, candidates),
    }
}

fn merge_candidates(
    plan: &ConflictPlan,
    base: &Value,
    candidates: &[Value],
) -> Result<ConflictResolution, ConflictError> {
    let mut variants = candidates.to_vec();
    let mut merged_fields = Vec::new();

    for rule in &plan.merge {
        let Some(base_field) = base.pointer(&rule.path) else {
            reject_if_required(plan.otherwise)?;
            continue;
        };
        let Some(base_group) = group_values(base_field, rule) else {
            reject_if_required(plan.otherwise)?;
            continue;
        };

        let mut groups = BTreeMap::<Vec<String>, (Vec<Value>, Vec<usize>)>::new();
        for (index, candidate) in variants.iter().enumerate() {
            let Some(field) = candidate.pointer(&rule.path) else {
                reject_if_required(plan.otherwise)?;
                continue;
            };
            let Some(group) = group_values(field, rule) else {
                reject_if_required(plan.otherwise)?;
                continue;
            };
            let key = group.iter().map(canonical_json).collect();
            groups
                .entry(key)
                .or_insert_with(|| (group, Vec::new()))
                .1
                .push(index);
        }

        for (_, (group, indexes)) in groups {
            if indexes.len() < 2 || group != base_group {
                reject_if_required(plan.otherwise)?;
                continue;
            }

            let base_document = CrdtDocument::from_json(base_field)?;
            let branches = indexes
                .iter()
                .map(|index| {
                    base_document.change(
                        variants[*index]
                            .pointer(&rule.path)
                            .expect("grouped merge fields must still exist"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let document = CrdtDocument::merge(&branches)?;
            let merged_value = document.json()?;

            for index in indexes {
                *variants[index]
                    .pointer_mut(&rule.path)
                    .expect("grouped merge fields must still exist") = merged_value.clone();
            }
            merged_fields.push(MergedField {
                path: rule.path.clone(),
                group: group.clone(),
                document,
            });
        }
    }

    Ok(ConflictResolution {
        variants: unique_values(&variants),
        merged_fields,
    })
}

fn reject_if_required(fallback: ConflictFallback) -> Result<(), ConflictError> {
    match fallback {
        ConflictFallback::Mvcc => Ok(()),
        ConflictFallback::Reject => Err(ConflictError::Rejected),
    }
}

fn group_values(value: &Value, rule: &ConflictMergeRule) -> Option<Vec<Value>> {
    rule.group_by
        .iter()
        .map(|pointer| value.pointer(pointer).cloned())
        .collect()
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(values) => {
            let values = values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>();
            serde_json::to_string(&values).expect("JSON values are serializable")
        }
        Value::Array(values) => {
            let values = values.iter().map(canonical_json).collect::<Vec<_>>();
            serde_json::to_string(&values).expect("JSON values are serializable")
        }
        _ => serde_json::to_string(value).expect("JSON values are serializable"),
    }
}

fn unique_values(values: &[Value]) -> Vec<Value> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(value) {
            unique.push(value.clone());
        }
    }
    unique
}

#[derive(Clone)]
struct CrdtEnvelope {
    value: CrdtJson,
}

impl CrdtEnvelope {
    fn from_json(value: &Value) -> Self {
        Self {
            value: CrdtJson::from_json(value),
        }
    }
}

impl Reconcile for CrdtEnvelope {
    type Key<'a> = NoKey;

    fn reconcile<R: Reconciler>(&self, mut reconciler: R) -> Result<(), R::Error> {
        reconciler.map()?.put("value", &self.value)
    }
}

impl Hydrate for CrdtEnvelope {
    fn hydrate_map<D: autosurgeon::ReadDoc>(
        document: &D,
        object: &automerge::ObjId,
    ) -> Result<Self, HydrateError> {
        Ok(Self {
            value: hydrate_prop(document, object, "value")?,
        })
    }
}

#[derive(Clone)]
enum CrdtJson {
    Null,
    Bool(bool),
    Number(Number),
    Text(Text),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

impl CrdtJson {
    fn from_json(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(*value),
            Value::Number(value) => Self::Number(value.clone()),
            Value::String(value) => Self::Text(Text::with_value(value)),
            Value::Array(values) => Self::Array(values.iter().map(Self::from_json).collect()),
            Value::Object(values) => Self::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from_json(value)))
                    .collect(),
            ),
        }
    }

    fn update(&mut self, value: &Value) {
        match (self, value) {
            (Self::Text(current), Value::String(next)) => current.update(next),
            (Self::Array(current), Value::Array(next)) => {
                for (current, next) in current.iter_mut().zip(next) {
                    current.update(next);
                }
                current.truncate(next.len());
                current.extend(next[current.len()..].iter().map(Self::from_json));
            }
            (Self::Object(current), Value::Object(next)) => {
                current.retain(|key, _| next.contains_key(key));
                for (key, next) in next {
                    match current.get_mut(key) {
                        Some(current) => current.update(next),
                        None => {
                            current.insert(key.clone(), Self::from_json(next));
                        }
                    }
                }
            }
            (current, next) => *current = Self::from_json(next),
        }
    }

    fn into_json(self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(value),
            Self::Number(value) => Value::Number(value),
            Self::Text(value) => Value::String(value.as_str().to_owned()),
            Self::Array(values) => Value::Array(values.into_iter().map(Self::into_json).collect()),
            Self::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, value.into_json()))
                    .collect::<Map<_, _>>(),
            ),
        }
    }
}

impl Reconcile for CrdtJson {
    type Key<'a> = NoKey;

    fn reconcile<R: Reconciler>(&self, mut reconciler: R) -> Result<(), R::Error> {
        match self {
            Self::Null => reconciler.none(),
            Self::Bool(value) => reconciler.boolean(*value),
            Self::Number(value) => {
                if let Some(value) = value.as_i64() {
                    reconciler.i64(value)
                } else if let Some(value) = value.as_u64() {
                    reconciler.u64(value)
                } else {
                    reconciler.f64(value.as_f64().expect("JSON numbers are finite"))
                }
            }
            Self::Text(value) => value.reconcile(reconciler),
            Self::Array(values) => values.reconcile(reconciler),
            Self::Object(values) => values.reconcile(reconciler),
        }
    }
}

impl Hydrate for CrdtJson {
    fn hydrate_bool(value: bool) -> Result<Self, HydrateError> {
        Ok(Self::Bool(value))
    }

    fn hydrate_f64(value: f64) -> Result<Self, HydrateError> {
        Number::from_f64(value)
            .map(Self::Number)
            .ok_or_else(|| HydrateError::unexpected("a finite JSON number", value.to_string()))
    }

    fn hydrate_int(value: i64) -> Result<Self, HydrateError> {
        Ok(Self::Number(value.into()))
    }

    fn hydrate_uint(value: u64) -> Result<Self, HydrateError> {
        Ok(Self::Number(value.into()))
    }

    fn hydrate_string(value: &str) -> Result<Self, HydrateError> {
        Ok(Self::Text(Text::with_value(value)))
    }

    fn hydrate_map<D: autosurgeon::ReadDoc>(
        document: &D,
        object: &automerge::ObjId,
    ) -> Result<Self, HydrateError> {
        BTreeMap::<String, Self>::hydrate_map(document, object).map(Self::Object)
    }

    fn hydrate_seq<D: autosurgeon::ReadDoc>(
        document: &D,
        object: &automerge::ObjId,
    ) -> Result<Self, HydrateError> {
        Vec::<Self>::hydrate_seq(document, object).map(Self::Array)
    }

    fn hydrate_text<D: autosurgeon::ReadDoc>(
        document: &D,
        object: &automerge::ObjId,
    ) -> Result<Self, HydrateError> {
        Text::hydrate_text(document, object).map(Self::Text)
    }

    fn hydrate_none() -> Result<Self, HydrateError> {
        Ok(Self::Null)
    }
}
