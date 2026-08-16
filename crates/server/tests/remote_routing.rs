use std::{collections::BTreeMap, sync::Arc};

use patchouli_backend::{
    BackendConfig, BackendEngine, BackendService, CreateEntityData, EntityVersion,
    RetrieveEntitiesData, RpcParams,
};
use patchouli_provider::{EntityKey, Provider};
use patchouli_provider_remote::{RemoteProvider, remote_provider_router};
use patchouli_provider_router::{RoutingProvider, ScopeRoute};
use patchouli_provider_sqlite::SqliteProvider;
use serde_json::{Value, json};

const BACKEND: &str = include_str!("../../../config/patchouli.default.json");
const KNOWLEDGE: &str =
    include_str!("../../../packages/protocol/schemas/examples/knowledge@1.json");

#[tokio::test]
async fn backend_routes_one_scope_to_a_remote_storage_node() {
    let directory = tempfile::tempdir().unwrap();
    let remote_storage: Arc<dyn Provider> = Arc::new(
        SqliteProvider::open(directory.path().join("remote.db"))
            .await
            .unwrap(),
    );
    let recovery = remote_storage.initialize().await.unwrap();
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let app = remote_provider_router(
        Arc::clone(&remote_storage),
        "routing-secret".to_owned(),
        recovery,
        shutdown_rx,
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let local: Arc<dyn Provider> = Arc::new(
        SqliteProvider::open(directory.path().join("local.db"))
            .await
            .unwrap(),
    );
    let remote: Arc<dyn Provider> = Arc::new(
        RemoteProvider::connect(&format!("http://{address}"), "routing-secret".to_owned())
            .await
            .unwrap(),
    );
    let router = RoutingProvider::new(
        BTreeMap::from([
            ("local".to_owned(), Arc::clone(&local)),
            ("shared".to_owned(), remote),
        ]),
        "local".to_owned(),
        vec![ScopeRoute {
            scope: BTreeMap::from([("workspace_id".to_owned(), json!("shared"))]),
            provider: "shared".to_owned(),
        }],
    )
    .unwrap();
    let engine = BackendEngine::start(BackendConfig::from_json(BACKEND).unwrap(), Arc::new(router))
        .await
        .unwrap();

    engine
        .create(RpcParams {
            meta: BTreeMap::from([
                ("workspace_id".to_owned(), json!("shared")),
                ("user_id".to_owned(), json!("user-1")),
            ]),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("remote-knowledge".to_owned()),
                value: serde_json::from_str::<Value>(KNOWLEDGE).unwrap(),
            },
        })
        .await
        .unwrap();
    engine
        .create(RpcParams {
            meta: BTreeMap::from([
                ("workspace_id".to_owned(), json!("shared")),
                ("user_id".to_owned(), json!("user-1")),
            ]),
            data: CreateEntityData {
                entity_type: "knowledge".to_owned(),
                id: Some("remote-knowledge-2".to_owned()),
                value: serde_json::from_str::<Value>(KNOWLEDGE).unwrap(),
            },
        })
        .await
        .unwrap();

    let key = EntityKey {
        scope_json: serde_json::to_string(&BTreeMap::from([
            ("user_id", "user-1"),
            ("workspace_id", "shared"),
        ]))
        .unwrap(),
        entity_type: "knowledge".to_owned(),
        entity_id: "remote-knowledge".to_owned(),
    };
    assert!(remote_storage.read_entity(&key).await.unwrap().is_some());
    assert!(local.read_entity(&key).await.unwrap().is_none());

    let first_page = engine
        .retrieve(RpcParams {
            meta: BTreeMap::from([
                ("workspace_id".to_owned(), json!("shared")),
                ("user_id".to_owned(), json!("user-1")),
            ]),
            data: RetrieveEntitiesData {
                query: json!({ "order": "id_asc" }).to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 1,
            },
        })
        .await
        .unwrap();
    let cursor = first_page.meta.get("next_cursor").unwrap().clone();
    assert!(matches!(
        &first_page.data.hits[0].variants[0],
        EntityVersion::Active { entity_ref, .. } if entity_ref.id == "remote-knowledge"
    ));
    let second_page = engine
        .retrieve(RpcParams {
            meta: BTreeMap::from([
                ("workspace_id".to_owned(), json!("shared")),
                ("user_id".to_owned(), json!("user-1")),
            ]),
            data: RetrieveEntitiesData {
                query: json!({ "order": "id_asc", "cursor": cursor }).to_string(),
                types: Some(vec!["knowledge".to_owned()]),
                limit: 1,
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        &second_page.data.hits[0].variants[0],
        EntityVersion::Active { entity_ref, .. } if entity_ref.id == "remote-knowledge-2"
    ));

    engine.shutdown().await.unwrap();
    server.abort();
    remote_storage.shutdown().await.unwrap();
}
