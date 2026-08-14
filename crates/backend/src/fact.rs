use std::collections::BTreeMap;

use jsonschema::{Retrieve, Uri};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const KNOWLEDGE_ENTITY_TYPE: &str = "knowledge";
pub const KNOWLEDGE_RELATION_ENTITY_TYPE: &str = "knowledge_relation";
pub const FACT_COMMON_SCHEMA_URI: &str = "urn:patchouli:schema:fact-common:1";
pub const KNOWLEDGE_SCHEMA_URI: &str = "urn:patchouli:schema:knowledge:1";
pub const KNOWLEDGE_RELATION_SCHEMA_URI: &str = "urn:patchouli:schema:knowledge-relation:1";

const FACT_COMMON_SCHEMA: &str =
    include_str!("../../../packages/protocol/schemas/fact-common@1.schema.json");
const KNOWLEDGE_SCHEMA: &str =
    include_str!("../../../packages/protocol/schemas/knowledge@1.schema.json");
const KNOWLEDGE_RELATION_SCHEMA: &str =
    include_str!("../../../packages/protocol/schemas/knowledge-relation@1.schema.json");

#[derive(Clone, Copy, Debug)]
pub(crate) struct BuiltinFactSchemaRetriever;

impl Retrieve for BuiltinFactSchemaRetriever {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let source = match uri.as_str() {
            FACT_COMMON_SCHEMA_URI => FACT_COMMON_SCHEMA,
            KNOWLEDGE_SCHEMA_URI => KNOWLEDGE_SCHEMA,
            KNOWLEDGE_RELATION_SCHEMA_URI => KNOWLEDGE_RELATION_SCHEMA,
            _ => return Err(format!("unknown schema URI {uri}").into()),
        };
        serde_json::from_str(source).map_err(Into::into)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeValue {
    pub content: KnowledgeContent,
    pub metadata: FactMetadata<KnowledgeSchemaVersion>,
    pub artifact: Vec<ArtifactReference>,
    pub profile: KnowledgeProfile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum KnowledgeContent {
    Text { text: String },
    Structured { value: Value },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactMetadata<TSchema> {
    pub core: FactMetadataCore<TSchema>,
    pub extensions: BTreeMap<String, BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactMetadataCore<TSchema> {
    pub schema: TSchema,
    pub scope: FactScope,
    pub origin: FactOrigin,
    pub time: FactTime,
    pub lifecycle: FactLifecycle,
    pub provenance: Vec<Provenance>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnowledgeSchemaVersion {
    #[serde(rename = "patchouli.knowledge@1")]
    V1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnowledgeRelationSchemaVersion {
    #[serde(rename = "patchouli.knowledge-relation@1")]
    V1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactScope {
    pub tenant: Option<String>,
    pub workspace: Option<String>,
    pub user: Option<String>,
    pub session: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactOrigin {
    pub provider: Option<String>,
    pub binding: Option<String>,
    pub native_type: Option<String>,
    pub native_id: Option<String>,
    pub native_revision: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactTime {
    pub event_at: Option<String>,
    pub source_created_at: Option<String>,
    pub source_updated_at: Option<String>,
    pub observed_at: String,
    pub ingested_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactLifecycle {
    pub status: LifecycleStatus,
    pub expires_at: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Active,
    Superseded,
    Retracted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub kind: ProvenanceKind,
    pub actor: String,
    pub source: Option<String>,
    pub recorded_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceKind {
    Observed,
    Asserted,
    Derived,
    Imported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case", deny_unknown_fields)]
pub enum ArtifactReference {
    Source {
        #[serde(rename = "ref")]
        artifact_ref: String,
        media_type: String,
        digest: String,
        metadata: BTreeMap<String, Value>,
    },
    Attachment {
        #[serde(rename = "ref")]
        artifact_ref: String,
        media_type: String,
        digest: String,
        metadata: BTreeMap<String, Value>,
    },
    Embedding {
        #[serde(rename = "ref")]
        artifact_ref: String,
        media_type: EmbeddingMediaType,
        digest: String,
        metadata: EmbeddingArtifactMetadata,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingMediaType {
    #[serde(rename = "application/vnd.patchouli.embedding")]
    PatchouliEmbedding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingArtifactMetadata {
    pub model: String,
    pub dimensions: u32,
    pub metric: EmbeddingMetric,
    pub source_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingMetric {
    Cosine,
    DotProduct,
    Euclidean,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeProfile {
    pub epistemic: EpistemicStatus,
    pub temporal: TemporalGrounding,
    pub ownership: Ownership,
    pub abstraction: AbstractionLevel,
    pub persistence: Persistence,
    pub retrieval: Vec<RetrievalMode>,
    pub actionability: Actionability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpistemicStatus {
    Unknown,
    Observation,
    Hypothesis,
    Belief,
    Knowledge,
    Derived,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TemporalGrounding {
    Unknown,
    Timeless,
    Instant { at: String },
    Interval { start: String, end: String },
    Sequence { items: Vec<KnowledgeRef> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    Unknown,
    World,
    Agent,
    User,
    Shared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbstractionLevel {
    Unknown,
    Instance,
    Pattern,
    Concept,
    Rule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Persistence {
    Unknown,
    Working,
    ShortTerm,
    LongTerm,
    Permanent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMode {
    Unknown,
    Exact,
    Associative,
    Contextual,
    Causal,
    Procedural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actionability {
    Unknown,
    Informational,
    Directive,
    Constraint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeRef {
    #[serde(rename = "type")]
    pub entity_type: KnowledgeEntityType,
    pub id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnowledgeEntityType {
    #[serde(rename = "knowledge")]
    Knowledge,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeRelationValue {
    #[serde(rename = "type")]
    pub relation_type: KnowledgeRelationType,
    pub from: Vec<KnowledgeRef>,
    pub to: Vec<KnowledgeRef>,
    pub metadata: FactMetadata<KnowledgeRelationSchemaVersion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRelationType {
    Supports,
    Contradicts,
    DerivedFrom,
    GeneralizedFrom,
    Causes,
    Supersedes,
}
