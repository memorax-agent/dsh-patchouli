use patchouli_backend::{
    ConflictFallback, ConflictMergeRule, ConflictMergeStrategy, ConflictPlan, ConflictStrategy,
    CrdtDocument, resolve_conflict,
};
use serde_json::{Value, json};

#[test]
fn automerge_combines_concurrent_text_edits_and_keeps_mvcc_variants() {
    let base = knowledge(json!({ "kind": "text", "text": "some value" }), "base");
    let first = knowledge(json!({ "kind": "text", "text": "some day" }), "first");
    let second = knowledge(json!({ "kind": "text", "text": "another value" }), "second");

    let resolution = resolve_conflict(&merge_plan(), &base, &[first, second]).unwrap();

    assert_eq!(resolution.variants.len(), 2);
    assert!(resolution.variants.iter().all(|variant| {
        variant.value.pointer("/content/text") == Some(&json!("another day"))
            && variant.crdt_fields.contains_key("/content")
    }));
    assert_eq!(
        resolution
            .variants
            .iter()
            .map(|variant| variant.value.pointer("/metadata/source").unwrap().clone())
            .collect::<Vec<_>>(),
        [json!("first"), json!("second")]
    );
}

#[test]
fn automerge_combines_structured_fields_and_collapses_identical_mvcc_data() {
    let base = knowledge(
        json!({
            "kind": "structured",
            "value": { "left": 0, "right": 0 }
        }),
        "same",
    );
    let first = knowledge(
        json!({
            "kind": "structured",
            "value": { "left": 1, "right": 0 }
        }),
        "same",
    );
    let second = knowledge(
        json!({
            "kind": "structured",
            "value": { "left": 0, "right": 2 }
        }),
        "same",
    );

    let resolution = resolve_conflict(&merge_plan(), &base, &[first, second]).unwrap();

    assert_eq!(resolution.variants.len(), 1);
    assert_eq!(
        resolution.variants[0].value.pointer("/content/value"),
        Some(&json!({ "left": 1, "right": 2 }))
    );
}

#[test]
fn incompatible_content_kinds_remain_separate_mvcc_versions() {
    let base = knowledge(json!({ "kind": "text", "text": "base" }), "same");
    let text = knowledge(json!({ "kind": "text", "text": "updated" }), "same");
    let structured = knowledge(
        json!({ "kind": "structured", "value": { "key": "value" } }),
        "same",
    );

    let resolution = resolve_conflict(&merge_plan(), &base, &[text, structured]).unwrap();

    assert_eq!(resolution.variants.len(), 2);
}

#[test]
fn request_selected_mvcc_and_reject_modes_are_applied_directly() {
    let base = json!({ "content": 0 });
    let candidates = [json!({ "content": 1 }), json!({ "content": 2 })];
    let mut plan = merge_plan();

    plan.strategy = ConflictStrategy::Mvcc;
    assert_eq!(
        resolve_conflict(&plan, &base, &candidates)
            .unwrap()
            .variants
            .into_iter()
            .map(|candidate| candidate.value)
            .collect::<Vec<_>>(),
        candidates
    );

    plan.strategy = ConflictStrategy::Reject;
    assert!(resolve_conflict(&plan, &base, &candidates).is_err());
}

#[test]
fn crdt_documents_expose_durable_changes_and_frontier() {
    let base = CrdtDocument::from_json(&json!({ "text": "base" })).unwrap();
    let changed = base.change(&json!({ "text": "changed" })).unwrap();
    let reloaded = CrdtDocument::from_bytes(changed.as_bytes().to_vec()).unwrap();

    assert_eq!(reloaded.json().unwrap(), json!({ "text": "changed" }));
    assert!(!reloaded.heads().unwrap().is_empty());
    let changes = reloaded.changes().unwrap();
    assert!(changes.len() >= 2);
    assert!(changes.iter().all(|change| !change.bytes.is_empty()));
}

#[test]
fn a_candidate_can_use_multiple_crdt_heads_as_its_base() {
    let base = CrdtDocument::from_json(&json!({ "left": 0, "right": 0 })).unwrap();
    let left = base.change(&json!({ "left": 1, "right": 0 })).unwrap();
    let right = base.change(&json!({ "left": 0, "right": 2 })).unwrap();
    let merged_base = CrdtDocument::merge(&[left, right]).unwrap();

    assert_eq!(merged_base.heads().unwrap().len(), 2);
    let candidate = merged_base
        .change(&json!({ "left": 1, "right": 2, "confirmed": true }))
        .unwrap();
    assert_eq!(candidate.heads().unwrap().len(), 1);
    assert!(
        candidate
            .changes()
            .unwrap()
            .iter()
            .any(|change| change.parents.len() == 2)
    );
}

fn merge_plan() -> ConflictPlan {
    ConflictPlan {
        strategy: ConflictStrategy::Merge,
        base_versions_field: "base_versions".to_owned(),
        base_versions: None,
        merge: vec![ConflictMergeRule {
            path: "/content".to_owned(),
            strategy: ConflictMergeStrategy::Automerge,
            group_by: vec!["/kind".to_owned()],
        }],
        otherwise: ConflictFallback::Mvcc,
    }
}

fn knowledge(content: Value, source: &str) -> Value {
    json!({
        "content": content,
        "metadata": { "source": source },
        "artifact": [],
        "profile": {}
    })
}
