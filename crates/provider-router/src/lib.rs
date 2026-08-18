use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use patchouli_provider::{
    ChangePage, ChangeQuery, ConsistentRead, EntityCommit, EntityCommitOutcome, EntityKey,
    EntitySnapshot, IdempotencyReadOutcome, IdempotencyRecord, IdempotentCommitOutcome, Provider,
    ProviderCapabilities, ProviderError, ProviderRecovery, ReadConsistency, RetrieveQuery,
    RetrievedPage, WorkUnit, WorkUnitCommit, WorkUnitCommitOutcome, WorkUnitPublish,
    WorkUnitReadOutcome, WorkUnitRetrieveOutcome,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeRoute {
    pub scope: BTreeMap<String, Value>,
    pub provider: String,
}

pub struct RoutingProvider {
    providers: BTreeMap<String, Arc<dyn Provider>>,
    default: String,
    routes: Vec<ScopeRoute>,
    capabilities: ProviderCapabilities,
}

impl RoutingProvider {
    pub fn new(
        providers: BTreeMap<String, Arc<dyn Provider>>,
        default: String,
        routes: Vec<ScopeRoute>,
    ) -> Result<Self, ProviderError> {
        if providers.is_empty() {
            return Err(ProviderError::new("at least one provider is required"));
        }
        if !providers.contains_key(&default) {
            return Err(ProviderError::new(format!(
                "default provider {default:?} is not configured"
            )));
        }
        for (index, route) in routes.iter().enumerate() {
            if route.scope.is_empty() {
                return Err(ProviderError::new(format!(
                    "provider route {index} must match at least one scope field"
                )));
            }
            if !providers.contains_key(&route.provider) {
                return Err(ProviderError::new(format!(
                    "provider route {index} references unknown provider {:?}",
                    route.provider
                )));
            }
        }
        let capabilities = intersect_capabilities(providers.values().map(|p| p.capabilities()));
        Ok(Self {
            providers,
            default,
            routes,
            capabilities,
        })
    }

    fn route_scope(&self, scope_json: &str) -> Result<&Arc<dyn Provider>, ProviderError> {
        let scope: BTreeMap<String, Value> = serde_json::from_str(scope_json)
            .map_err(|error| ProviderError::new(format!("invalid provider scope: {error}")))?;
        let name = self
            .routes
            .iter()
            .find(|route| {
                route
                    .scope
                    .iter()
                    .all(|(key, value)| scope.get(key) == Some(value))
            })
            .map_or(self.default.as_str(), |route| route.provider.as_str());
        self.providers
            .get(name)
            .ok_or_else(|| ProviderError::new(format!("provider {name:?} is not configured")))
    }

    fn route_identity(&self, identity_json: &str) -> Result<&Arc<dyn Provider>, ProviderError> {
        let identity: ScopedIdentity = serde_json::from_str(identity_json).map_err(|error| {
            ProviderError::new(format!("invalid scoped provider identity: {error}"))
        })?;
        let scope_json = serde_json::to_string(&identity.scope)
            .map_err(|error| ProviderError::new(format!("invalid provider scope: {error}")))?;
        self.route_scope(&scope_json)
    }

    fn ensure_same_route(
        &self,
        expected: &Arc<dyn Provider>,
        scope_json: &str,
    ) -> Result<(), ProviderError> {
        let actual = self.route_scope(scope_json)?;
        if Arc::ptr_eq(expected, actual) {
            Ok(())
        } else {
            Err(ProviderError::new(
                "one atomic provider operation cannot cross provider routes",
            ))
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopedIdentity {
    scope: BTreeMap<String, Value>,
    #[serde(rename = "fields")]
    _fields: BTreeMap<String, Value>,
}

fn intersect_capabilities(
    mut capabilities: impl Iterator<Item = ProviderCapabilities>,
) -> ProviderCapabilities {
    let mut result = capabilities
        .next()
        .expect("RoutingProvider validates that providers are non-empty");
    for next in capabilities {
        result.authority &= next.authority;
        result.replica &= next.replica;
        result.change_stream &= next.change_stream;
        result.retrieval &= next.retrieval;
        result.idempotency &= next.idempotency;
        result.work_units &= next.work_units;
        result.causal_reads &= next.causal_reads;
        result.monotonic_reads &= next.monotonic_reads;
        result.read_your_writes &= next.read_your_writes;
        result.linearizable_reads &= next.linearizable_reads;
    }
    result
}

#[async_trait]
impl Provider for RoutingProvider {
    fn kind(&self) -> &'static str {
        "routed"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError> {
        let mut initialized = Vec::new();
        let mut recovery = ProviderRecovery {
            generation: 0,
            recovered_after_unclean_shutdown: false,
        };
        for (name, provider) in &self.providers {
            match provider.initialize().await {
                Ok(current) => {
                    recovery.generation = recovery.generation.max(current.generation);
                    recovery.recovered_after_unclean_shutdown |=
                        current.recovered_after_unclean_shutdown;
                    initialized.push((name.as_str(), Arc::clone(provider)));
                }
                Err(error) => {
                    let reason = error.reason();
                    let mut message = format!("provider {name:?} initialization failed: {error}");
                    let mut cleanup_errors = Vec::new();
                    for (initialized_name, provider) in initialized.into_iter().rev() {
                        if let Err(error) = provider.shutdown().await {
                            cleanup_errors.push(format!(
                                "provider {initialized_name:?} shutdown failed: {error}"
                            ));
                        }
                    }
                    if !cleanup_errors.is_empty() {
                        message.push_str("; initialization rollback also failed: ");
                        message.push_str(&cleanup_errors.join("; "));
                    }
                    return Err(ProviderError::with_reason(reason, message));
                }
            }
        }
        Ok(recovery)
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        for (name, provider) in &self.providers {
            provider
                .health_check()
                .await
                .map_err(|error| error.context(format!("provider {name:?} health check failed")))?;
        }
        Ok(())
    }

    async fn read_entity(
        &self,
        key: &EntityKey,
        consistency: ReadConsistency,
    ) -> Result<ConsistentRead<Option<EntitySnapshot>>, ProviderError> {
        self.route_scope(&key.scope_json)?
            .read_entity(key, consistency)
            .await
    }

    async fn read_changes(&self, query: ChangeQuery) -> Result<ChangePage, ProviderError> {
        self.route_scope(&query.scope_json)?
            .read_changes(query)
            .await
    }

    async fn wait_for_changes(
        &self,
        scope_json: &str,
        after_cursor: u64,
    ) -> Result<(), ProviderError> {
        self.route_scope(scope_json)?
            .wait_for_changes(scope_json, after_cursor)
            .await
    }

    async fn retrieve_entities(
        &self,
        query: RetrieveQuery,
        consistency: ReadConsistency,
    ) -> Result<ConsistentRead<RetrievedPage>, ProviderError> {
        self.route_scope(&query.scope_json)?
            .retrieve_entities(query, consistency)
            .await
    }

    async fn commit_entity(
        &self,
        commit: EntityCommit,
    ) -> Result<EntityCommitOutcome, ProviderError> {
        self.route_scope(&commit.key.scope_json)?
            .commit_entity(commit)
            .await
    }

    async fn read_idempotency(
        &self,
        scope_json: &str,
        consistency: ReadConsistency,
        identity_json: &str,
        request_json: &str,
        now_unix_ms: u64,
    ) -> Result<IdempotencyReadOutcome, ProviderError> {
        let provider = self.route_scope(scope_json)?;
        if !Arc::ptr_eq(provider, self.route_identity(identity_json)?) {
            return Err(ProviderError::new(
                "scope and idempotency identity route to different providers",
            ));
        }
        provider
            .read_idempotency(
                scope_json,
                consistency,
                identity_json,
                request_json,
                now_unix_ms,
            )
            .await
    }

    async fn read_idempotency_in_work_unit(
        &self,
        work_unit: &WorkUnit,
        consistency: ReadConsistency,
        identity_json: &str,
        request_json: &str,
        now_unix_ms: u64,
        allow_replay: bool,
    ) -> Result<IdempotencyReadOutcome, ProviderError> {
        let provider = self.route_scope(&work_unit.scope_json)?;
        let identity_provider = self.route_identity(identity_json)?;
        if !Arc::ptr_eq(provider, identity_provider) {
            return Err(ProviderError::new(
                "work unit and idempotency identity route to different providers",
            ));
        }
        provider
            .read_idempotency_in_work_unit(
                work_unit,
                consistency,
                identity_json,
                request_json,
                now_unix_ms,
                allow_replay,
            )
            .await
    }

    async fn commit_entity_idempotent(
        &self,
        commit: EntityCommit,
        idempotency: IdempotencyRecord,
        now_unix_ms: u64,
    ) -> Result<IdempotentCommitOutcome, ProviderError> {
        let provider = self.route_scope(&commit.key.scope_json)?;
        let identity_provider = self.route_identity(&idempotency.identity_json)?;
        if !Arc::ptr_eq(provider, identity_provider) {
            return Err(ProviderError::new(
                "entity and idempotency identity route to different providers",
            ));
        }
        provider
            .commit_entity_idempotent(commit, idempotency, now_unix_ms)
            .await
    }

    async fn read_entity_in_work_unit(
        &self,
        work_unit: &WorkUnit,
        key: &EntityKey,
        consistency: ReadConsistency,
    ) -> Result<WorkUnitReadOutcome, ProviderError> {
        let provider = self.route_scope(&work_unit.scope_json)?;
        self.ensure_same_route(provider, &key.scope_json)?;
        provider
            .read_entity_in_work_unit(work_unit, key, consistency)
            .await
    }

    async fn retrieve_entities_in_work_unit(
        &self,
        work_unit: &WorkUnit,
        query: RetrieveQuery,
        consistency: ReadConsistency,
    ) -> Result<WorkUnitRetrieveOutcome, ProviderError> {
        let provider = self.route_scope(&work_unit.scope_json)?;
        self.ensure_same_route(provider, &query.scope_json)?;
        provider
            .retrieve_entities_in_work_unit(work_unit, query, consistency)
            .await
    }

    async fn commit_entity_in_work_unit(
        &self,
        commit: WorkUnitCommit,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        let provider = self.route_scope(&commit.work_unit.scope_json)?;
        self.ensure_same_route(provider, &commit.entity.key.scope_json)?;
        if let Some(idempotency) = &commit.idempotency {
            let identity_provider = self.route_identity(&idempotency.identity_json)?;
            if !Arc::ptr_eq(provider, identity_provider) {
                return Err(ProviderError::new(
                    "work unit and idempotency identity route to different providers",
                ));
            }
        }
        provider.commit_entity_in_work_unit(commit).await
    }

    async fn publish_work_unit(
        &self,
        publish: WorkUnitPublish,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        let provider = self.route_scope(&publish.work_unit.scope_json)?;
        for resolution in &publish.resolutions {
            self.ensure_same_route(provider, &resolution.entity.key.scope_json)?;
        }
        provider.publish_work_unit(publish).await
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        for (name, provider) in &self.providers {
            provider
                .checkpoint()
                .await
                .map_err(|error| error.context(format!("provider {name:?} checkpoint failed")))?;
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        let mut reason = None;
        let mut errors = Vec::new();
        for (name, provider) in self.providers.iter().rev() {
            if let Err(error) = provider.shutdown().await {
                reason.get_or_insert(error.reason());
                errors.push(format!("provider {name:?} shutdown failed: {error}"));
            }
        }
        match reason {
            Some(reason) => Err(ProviderError::with_reason(reason, errors.join("; "))),
            None => Ok(()),
        }
    }
}
