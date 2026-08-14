use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use patchouli_provider::{
    ChangePage, ChangeQuery, ConsistencyAcquireOutcome, ConsistencyQuery, EntityCommit,
    EntityCommitOutcome, EntityKey, EntitySnapshot, IdempotencyReadOutcome, IdempotencyRecord,
    IdempotentCommitOutcome, Provider, ProviderCapabilities, ProviderError, ProviderErrorReason,
    ProviderRecovery, RetrieveQuery, RetrievedEntity, WorkUnit, WorkUnitCommit,
    WorkUnitCommitOutcome, WorkUnitPublish, WorkUnitReadOutcome,
};
use reqwest::{Client, Url, redirect::Policy};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use subtle::ConstantTimeEq;

const REMOTE_PROVIDER_PROTOCOL: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteProviderInfo {
    pub protocol: u16,
    pub provider: String,
    pub capabilities: ProviderCapabilities,
    pub recovery: ProviderRecovery,
}

pub struct RemoteProvider {
    endpoint: Url,
    token: String,
    client: Client,
    info: RemoteProviderInfo,
}

impl RemoteProvider {
    pub fn validate_endpoint(endpoint: &str) -> Result<(), ProviderError> {
        normalize_endpoint(endpoint).map(|_| ())
    }

    pub async fn connect(endpoint: &str, token: String) -> Result<Self, ProviderError> {
        if token.is_empty() {
            return Err(ProviderError::new(
                "remote provider token must not be empty",
            ));
        }
        let endpoint = normalize_endpoint(endpoint)?;
        let client = Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(remote_error)?;
        let info: RemoteProviderInfo = send(
            client
                .get(endpoint.join("info").map_err(remote_error)?)
                .timeout(Duration::from_secs(30)),
            &token,
        )
        .await?;
        if info.protocol != REMOTE_PROVIDER_PROTOCOL {
            return Err(ProviderError::new(format!(
                "remote provider protocol {} is unsupported",
                info.protocol
            )));
        }
        Ok(Self {
            endpoint,
            token,
            client,
            info,
        })
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderReply, ProviderError> {
        self.call_with_timeout(call, Some(Duration::from_secs(30)))
            .await
    }

    async fn call_with_timeout(
        &self,
        call: ProviderCall,
        timeout: Option<Duration>,
    ) -> Result<ProviderReply, ProviderError> {
        let mut request = self
            .client
            .post(self.endpoint.join("call").map_err(remote_error)?)
            .json(&call);
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        send(request, &self.token).await
    }
}

fn normalize_endpoint(endpoint: &str) -> Result<Url, ProviderError> {
    let mut url = Url::parse(endpoint).map_err(remote_error)?;
    match url.scheme() {
        "https" => {}
        "http" if url.host_str().is_some_and(is_loopback_host) => {}
        _ => {
            return Err(ProviderError::new(
                "remote provider endpoints must use HTTPS; HTTP is allowed only for loopback",
            ));
        }
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url = url.join("provider/v1/").map_err(remote_error)?;
    Ok(url)
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

async fn send<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
    token: &str,
) -> Result<T, ProviderError> {
    let response = request
        .bearer_auth(token)
        .send()
        .await
        .map_err(remote_error)?;
    let status = response.status();
    if !status.is_success() {
        let error = response
            .json::<RemoteError>()
            .await
            .unwrap_or_else(|_| RemoteError {
                reason: ProviderErrorReason::Internal,
                error: format!("remote provider returned HTTP {status}"),
            });
        return Err(ProviderError::with_reason(error.reason, error.error));
    }
    response.json().await.map_err(remote_error)
}

fn remote_error(error: impl std::fmt::Display) -> ProviderError {
    ProviderError::new(format!("remote provider error: {error}"))
}

fn unexpected_reply(reply: ProviderReply) -> ProviderError {
    ProviderError::new(format!(
        "remote provider returned an unexpected reply: {reply:?}"
    ))
}

#[async_trait]
impl Provider for RemoteProvider {
    fn kind(&self) -> &'static str {
        "remote"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.info.capabilities
    }

    async fn initialize(&self) -> Result<ProviderRecovery, ProviderError> {
        Ok(self.info.recovery)
    }

    async fn health_check(&self) -> Result<(), ProviderError> {
        match self.call(ProviderCall::Health).await? {
            ProviderReply::Unit => Ok(()),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn read_entity(&self, key: &EntityKey) -> Result<Option<EntitySnapshot>, ProviderError> {
        match self.call(ProviderCall::ReadEntity(key.clone())).await? {
            ProviderReply::Entity(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn acquire_consistency(
        &self,
        query: ConsistencyQuery,
    ) -> Result<ConsistencyAcquireOutcome, ProviderError> {
        match self.call(ProviderCall::AcquireConsistency(query)).await? {
            ProviderReply::Consistency(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn read_changes(&self, query: ChangeQuery) -> Result<ChangePage, ProviderError> {
        match self.call(ProviderCall::ReadChanges(query)).await? {
            ProviderReply::Changes(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn wait_for_changes(
        &self,
        scope_json: &str,
        after_cursor: u64,
    ) -> Result<(), ProviderError> {
        match self
            .call_with_timeout(
                ProviderCall::WaitForChanges {
                    scope_json: scope_json.to_owned(),
                    after_cursor,
                },
                None,
            )
            .await?
        {
            ProviderReply::Unit => Ok(()),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn retrieve_entities(
        &self,
        query: RetrieveQuery,
    ) -> Result<Vec<RetrievedEntity>, ProviderError> {
        match self.call(ProviderCall::RetrieveEntities(query)).await? {
            ProviderReply::Retrieved(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn commit_entity(
        &self,
        commit: EntityCommit,
    ) -> Result<EntityCommitOutcome, ProviderError> {
        match self.call(ProviderCall::CommitEntity(commit)).await? {
            ProviderReply::EntityCommit(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn read_idempotency(
        &self,
        identity_json: &str,
        request_json: &str,
        now_unix_ms: u64,
    ) -> Result<IdempotencyReadOutcome, ProviderError> {
        match self
            .call(ProviderCall::ReadIdempotency {
                identity_json: identity_json.to_owned(),
                request_json: request_json.to_owned(),
                now_unix_ms,
            })
            .await?
        {
            ProviderReply::IdempotencyRead(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn read_idempotency_in_work_unit(
        &self,
        work_unit: &WorkUnit,
        identity_json: &str,
        request_json: &str,
        now_unix_ms: u64,
        allow_replay: bool,
    ) -> Result<IdempotencyReadOutcome, ProviderError> {
        match self
            .call(ProviderCall::ReadIdempotencyInWorkUnit {
                work_unit: work_unit.clone(),
                identity_json: identity_json.to_owned(),
                request_json: request_json.to_owned(),
                now_unix_ms,
                allow_replay,
            })
            .await?
        {
            ProviderReply::IdempotencyRead(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn commit_entity_idempotent(
        &self,
        commit: EntityCommit,
        idempotency: IdempotencyRecord,
        now_unix_ms: u64,
    ) -> Result<IdempotentCommitOutcome, ProviderError> {
        match self
            .call(ProviderCall::CommitEntityIdempotent {
                commit,
                idempotency,
                now_unix_ms,
            })
            .await?
        {
            ProviderReply::IdempotentCommit(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn read_entity_in_work_unit(
        &self,
        work_unit: &WorkUnit,
        key: &EntityKey,
    ) -> Result<WorkUnitReadOutcome, ProviderError> {
        match self
            .call(ProviderCall::ReadEntityInWorkUnit {
                work_unit: work_unit.clone(),
                key: key.clone(),
            })
            .await?
        {
            ProviderReply::WorkUnitRead(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn commit_entity_in_work_unit(
        &self,
        commit: WorkUnitCommit,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        match self
            .call(ProviderCall::CommitEntityInWorkUnit(commit))
            .await?
        {
            ProviderReply::WorkUnitCommit(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn publish_work_unit(
        &self,
        publish: WorkUnitPublish,
    ) -> Result<WorkUnitCommitOutcome, ProviderError> {
        match self.call(ProviderCall::PublishWorkUnit(publish)).await? {
            ProviderReply::WorkUnitCommit(value) => Ok(value),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn checkpoint(&self) -> Result<(), ProviderError> {
        match self.call(ProviderCall::Checkpoint).await? {
            ProviderReply::Unit => Ok(()),
            reply => Err(unexpected_reply(reply)),
        }
    }

    async fn shutdown(&self) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[derive(Clone)]
struct ProviderServerState {
    provider: Arc<dyn Provider>,
    token: Arc<str>,
    info: RemoteProviderInfo,
}

pub fn remote_provider_router(
    provider: Arc<dyn Provider>,
    token: String,
    recovery: ProviderRecovery,
) -> Result<Router, ProviderError> {
    if token.is_empty() {
        return Err(ProviderError::new(
            "remote provider token must not be empty",
        ));
    }
    let info = RemoteProviderInfo {
        protocol: REMOTE_PROVIDER_PROTOCOL,
        provider: provider.kind().to_owned(),
        capabilities: provider.capabilities(),
        recovery,
    };
    let state = ProviderServerState {
        provider,
        token: token.into(),
        info,
    };
    Ok(Router::new()
        .route("/provider/v1/info", get(provider_info))
        .route("/provider/v1/call", post(provider_call))
        .with_state(state))
}

async fn provider_info(State(state): State<ProviderServerState>, headers: HeaderMap) -> Response {
    if !authenticated(&headers, &state.token) {
        return remote_failure(
            StatusCode::UNAUTHORIZED,
            "unauthenticated remote provider request",
        );
    }
    Json(state.info).into_response()
}

async fn provider_call(
    State(state): State<ProviderServerState>,
    headers: HeaderMap,
    Json(call): Json<ProviderCall>,
) -> Response {
    if !authenticated(&headers, &state.token) {
        return remote_failure(
            StatusCode::UNAUTHORIZED,
            "unauthenticated remote provider request",
        );
    }
    match execute_call(state.provider.as_ref(), call).await {
        Ok(reply) => Json(reply).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RemoteError {
                reason: error.reason(),
                error: error.to_string(),
            }),
        )
            .into_response(),
    }
}

fn authenticated(headers: &HeaderMap, token: &str) -> bool {
    let Some(value) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let expected = format!("Bearer {token}");
    value.len() == expected.len() && bool::from(value.as_bytes().ct_eq(expected.as_bytes()))
}

fn remote_failure(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(RemoteError {
            reason: ProviderErrorReason::Internal,
            error: message.to_owned(),
        }),
    )
        .into_response()
}

async fn execute_call(
    provider: &dyn Provider,
    call: ProviderCall,
) -> Result<ProviderReply, ProviderError> {
    Ok(match call {
        ProviderCall::Health => {
            provider.health_check().await?;
            ProviderReply::Unit
        }
        ProviderCall::ReadEntity(key) => ProviderReply::Entity(provider.read_entity(&key).await?),
        ProviderCall::AcquireConsistency(query) => {
            ProviderReply::Consistency(provider.acquire_consistency(query).await?)
        }
        ProviderCall::ReadChanges(query) => {
            ProviderReply::Changes(provider.read_changes(query).await?)
        }
        ProviderCall::WaitForChanges {
            scope_json,
            after_cursor,
        } => {
            provider.wait_for_changes(&scope_json, after_cursor).await?;
            ProviderReply::Unit
        }
        ProviderCall::RetrieveEntities(query) => {
            ProviderReply::Retrieved(provider.retrieve_entities(query).await?)
        }
        ProviderCall::CommitEntity(commit) => {
            ProviderReply::EntityCommit(provider.commit_entity(commit).await?)
        }
        ProviderCall::ReadIdempotency {
            identity_json,
            request_json,
            now_unix_ms,
        } => ProviderReply::IdempotencyRead(
            provider
                .read_idempotency(&identity_json, &request_json, now_unix_ms)
                .await?,
        ),
        ProviderCall::ReadIdempotencyInWorkUnit {
            work_unit,
            identity_json,
            request_json,
            now_unix_ms,
            allow_replay,
        } => ProviderReply::IdempotencyRead(
            provider
                .read_idempotency_in_work_unit(
                    &work_unit,
                    &identity_json,
                    &request_json,
                    now_unix_ms,
                    allow_replay,
                )
                .await?,
        ),
        ProviderCall::CommitEntityIdempotent {
            commit,
            idempotency,
            now_unix_ms,
        } => ProviderReply::IdempotentCommit(
            provider
                .commit_entity_idempotent(commit, idempotency, now_unix_ms)
                .await?,
        ),
        ProviderCall::ReadEntityInWorkUnit { work_unit, key } => {
            ProviderReply::WorkUnitRead(provider.read_entity_in_work_unit(&work_unit, &key).await?)
        }
        ProviderCall::CommitEntityInWorkUnit(commit) => {
            ProviderReply::WorkUnitCommit(provider.commit_entity_in_work_unit(commit).await?)
        }
        ProviderCall::PublishWorkUnit(publish) => {
            ProviderReply::WorkUnitCommit(provider.publish_work_unit(publish).await?)
        }
        ProviderCall::Checkpoint => {
            provider.checkpoint().await?;
            ProviderReply::Unit
        }
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
enum ProviderCall {
    Health,
    ReadEntity(EntityKey),
    AcquireConsistency(ConsistencyQuery),
    ReadChanges(ChangeQuery),
    WaitForChanges {
        scope_json: String,
        after_cursor: u64,
    },
    RetrieveEntities(RetrieveQuery),
    CommitEntity(EntityCommit),
    ReadIdempotency {
        identity_json: String,
        request_json: String,
        now_unix_ms: u64,
    },
    ReadIdempotencyInWorkUnit {
        work_unit: WorkUnit,
        identity_json: String,
        request_json: String,
        now_unix_ms: u64,
        allow_replay: bool,
    },
    CommitEntityIdempotent {
        commit: EntityCommit,
        idempotency: IdempotencyRecord,
        now_unix_ms: u64,
    },
    ReadEntityInWorkUnit {
        work_unit: WorkUnit,
        key: EntityKey,
    },
    CommitEntityInWorkUnit(WorkUnitCommit),
    PublishWorkUnit(WorkUnitPublish),
    Checkpoint,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "result", content = "data", rename_all = "snake_case")]
enum ProviderReply {
    Unit,
    Entity(Option<EntitySnapshot>),
    Consistency(ConsistencyAcquireOutcome),
    Changes(ChangePage),
    Retrieved(Vec<RetrievedEntity>),
    EntityCommit(EntityCommitOutcome),
    IdempotencyRead(IdempotencyReadOutcome),
    IdempotentCommit(IdempotentCommitOutcome),
    WorkUnitRead(WorkUnitReadOutcome),
    WorkUnitCommit(WorkUnitCommitOutcome),
}

#[derive(Serialize, Deserialize)]
struct RemoteError {
    reason: ProviderErrorReason,
    error: String,
}
