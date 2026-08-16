use patchouli_backend::{
    ARTIFACT_ENTITY_TYPE, ArtifactPlacement, ArtifactValue, BackendConfig, KNOWLEDGE_ENTITY_TYPE,
    KNOWLEDGE_RELATION_ENTITY_TYPE, KnowledgeRelationValue, KnowledgeValue,
};
use serde_json::{Value, json};

const CONFIG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../config/patchouli.example.json"
));
const KNOWLEDGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/knowledge@1.json"
));
const RELATION: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/knowledge-relation@1.json"
));
const MANAGED_ARTIFACT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/artifact-managed@1.json"
));
const INDEXED_ARTIFACT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/protocol/schemas/examples/artifact-indexed@1.json"
));

#[test]
fn fact_fixtures_match_rust_types_and_registered_schemas() {
    let config = BackendConfig::from_json(CONFIG).expect("valid config");
    let knowledge_json: Value = serde_json::from_str(KNOWLEDGE).expect("knowledge fixture JSON");
    let relation_json: Value = serde_json::from_str(RELATION).expect("relation fixture JSON");
    let managed_artifact_json: Value =
        serde_json::from_str(MANAGED_ARTIFACT).expect("managed artifact fixture JSON");
    let indexed_artifact_json: Value =
        serde_json::from_str(INDEXED_ARTIFACT).expect("indexed artifact fixture JSON");

    let knowledge: KnowledgeValue =
        serde_json::from_value(knowledge_json.clone()).expect("typed knowledge");
    let relation: KnowledgeRelationValue =
        serde_json::from_value(relation_json.clone()).expect("typed relation");
    let managed_artifact: ArtifactValue =
        serde_json::from_value(managed_artifact_json.clone()).expect("typed managed artifact");
    let indexed_artifact: ArtifactValue =
        serde_json::from_value(indexed_artifact_json.clone()).expect("typed indexed artifact");

    assert!(matches!(
        managed_artifact.placement,
        ArtifactPlacement::Managed { .. }
    ));
    assert!(matches!(
        indexed_artifact.placement,
        ArtifactPlacement::Indexed { .. }
    ));
    assert_eq!(relation.from.len(), 2);
    assert_eq!(relation.to.len(), 2);
    assert_eq!(serde_json::to_value(knowledge).unwrap(), knowledge_json);
    assert_eq!(serde_json::to_value(relation).unwrap(), relation_json);
    assert_eq!(
        serde_json::to_value(managed_artifact).unwrap(),
        managed_artifact_json
    );
    assert_eq!(
        serde_json::to_value(indexed_artifact).unwrap(),
        indexed_artifact_json
    );
    config
        .validate_entity_value(ARTIFACT_ENTITY_TYPE, &managed_artifact_json)
        .expect("managed artifact schema");
    config
        .validate_entity_value(ARTIFACT_ENTITY_TYPE, &indexed_artifact_json)
        .expect("indexed artifact schema");
    config
        .validate_entity_value(KNOWLEDGE_ENTITY_TYPE, &knowledge_json)
        .expect("knowledge schema");
    config
        .validate_entity_value(KNOWLEDGE_RELATION_ENTITY_TYPE, &relation_json)
        .expect("relation schema");
}

#[test]
fn managed_artifacts_require_content_identity_but_indexed_artifacts_may_defer_it() {
    let config = BackendConfig::from_json(CONFIG).expect("valid config");
    let mut managed: Value =
        serde_json::from_str(MANAGED_ARTIFACT).expect("managed artifact fixture JSON");
    managed["digest"] = Value::Null;
    assert!(
        config
            .validate_entity_value(ARTIFACT_ENTITY_TYPE, &managed)
            .is_err()
    );

    let mut indexed: Value =
        serde_json::from_str(INDEXED_ARTIFACT).expect("indexed artifact fixture JSON");
    indexed["digest"] = Value::Null;
    indexed["byte_length"] = Value::Null;
    config
        .validate_entity_value(ARTIFACT_ENTITY_TYPE, &indexed)
        .expect("indexed artifact may rely on provider revision");
}

#[test]
fn knowledge_profile_rejects_query_implementation_vocabulary() {
    let config = BackendConfig::from_json(CONFIG).expect("valid config");
    let mut knowledge: Value = serde_json::from_str(KNOWLEDGE).expect("knowledge fixture JSON");
    knowledge["profile"]["retrieval"] = json!(["full_text", "semantic"]);
    knowledge["profile"]["actionability"] = json!("advisory");

    assert!(
        config
            .validate_entity_value(KNOWLEDGE_ENTITY_TYPE, &knowledge)
            .is_err()
    );
}

#[test]
fn relation_endpoints_are_knowledge_references() {
    let config = BackendConfig::from_json(CONFIG).expect("valid config");
    let mut relation: Value = serde_json::from_str(RELATION).expect("relation fixture JSON");
    relation["from"][0]["type"] = json!("artifact");

    assert!(
        config
            .validate_entity_value(KNOWLEDGE_RELATION_ENTITY_TYPE, &relation)
            .is_err()
    );
}

#[test]
fn self_relations_are_valid_fact_records() {
    let config = BackendConfig::from_json(CONFIG).expect("valid config");
    let mut relation: Value = serde_json::from_str(RELATION).expect("relation fixture JSON");
    relation["to"] = relation["from"].clone();

    config
        .validate_entity_value(KNOWLEDGE_RELATION_ENTITY_TYPE, &relation)
        .expect("self relation has no topology restriction");
}

#[test]
fn relation_requires_non_empty_endpoint_collections() {
    let config = BackendConfig::from_json(CONFIG).expect("valid config");
    let mut relation: Value = serde_json::from_str(RELATION).expect("relation fixture JSON");
    relation["from"] = json!([]);

    assert!(
        config
            .validate_entity_value(KNOWLEDGE_RELATION_ENTITY_TYPE, &relation)
            .is_err()
    );
}

#[test]
fn entity_identity_is_not_duplicated_inside_fact_metadata() {
    let config = BackendConfig::from_json(CONFIG).expect("valid config");
    let mut knowledge: Value = serde_json::from_str(KNOWLEDGE).expect("knowledge fixture JSON");
    knowledge["metadata"]["core"]["id"] = json!("duplicate-id");
    knowledge["metadata"]["core"]["revision"] = json!(7);

    assert!(
        config
            .validate_entity_value(KNOWLEDGE_ENTITY_TYPE, &knowledge)
            .is_err()
    );
}
