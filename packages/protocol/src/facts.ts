import type { EntityVersion } from './entity.js'
import type { JsonObject, JsonValue } from './json.js'

export const factEntityTypes = {
  artifact: 'artifact',
  knowledge: 'knowledge',
  knowledgeRelation: 'knowledge_relation',
} as const

export const factSchemaUris = {
  artifact: 'urn:patchouli:schema:artifact:1',
  common: 'urn:patchouli:schema:fact-common:1',
  knowledge: 'urn:patchouli:schema:knowledge:1',
  knowledgeRelation: 'urn:patchouli:schema:knowledge-relation:1',
} as const

export type FactEntityType = typeof factEntityTypes[keyof typeof factEntityTypes]

export interface KnowledgeValue {
  readonly content: KnowledgeContent
  readonly metadata: FactMetadata<'patchouli.knowledge@1'>
  readonly artifact: readonly ArtifactReference[]
  readonly profile: KnowledgeProfile
}

export type KnowledgeContent =
  | { readonly kind: 'text'; readonly text: string }
  | { readonly kind: 'structured'; readonly value: JsonValue }

export interface ArtifactValue {
  readonly media_type: string
  readonly name: string | null
  readonly byte_length: number | null
  readonly digest: string | null
  readonly placement: ArtifactPlacement
  readonly metadata: FactMetadata<'patchouli.artifact@1'>
}

export type ArtifactPlacement = ManagedArtifactPlacement | IndexedArtifactPlacement

export interface ManagedArtifactPlacement {
  readonly kind: 'managed'
  readonly provider: string
  readonly key: string
}

export interface IndexedArtifactPlacement {
  readonly kind: 'indexed'
  readonly provider: string
  readonly locator: string
  readonly revision: string | null
}

export interface FactMetadata<TSchema extends string> {
  readonly core: FactMetadataCore<TSchema>
  readonly extensions: Readonly<Record<string, JsonObject>>
}

export interface FactMetadataCore<TSchema extends string> {
  readonly schema: TSchema
  readonly scope: FactScope
  readonly origin: FactOrigin
  readonly time: FactTime
  readonly lifecycle: FactLifecycle
  readonly provenance: readonly Provenance[]
}

export interface FactScope {
  readonly tenant: string | null
  readonly workspace: string | null
  readonly user: string | null
  readonly session: string | null
}

export interface FactOrigin {
  readonly provider: string | null
  readonly binding: string | null
  readonly native_type: string | null
  readonly native_id: string | null
  readonly native_revision: string | null
}

export interface FactTime {
  readonly event_at: string | null
  readonly source_created_at: string | null
  readonly source_updated_at: string | null
  readonly observed_at: string
  readonly ingested_at: string
}

export interface FactLifecycle {
  readonly status: 'active' | 'superseded' | 'retracted'
  readonly expires_at: string | null
}

export interface Provenance {
  readonly kind: 'observed' | 'asserted' | 'derived' | 'imported'
  readonly actor: string
  readonly source: string | null
  readonly recorded_at: string
}

export interface ArtifactReference {
  readonly type: 'artifact'
  readonly id: string
  readonly role: 'source' | 'attachment' | 'embedding'
}

export interface KnowledgeProfile {
  readonly epistemic: 'unknown' | 'observation' | 'hypothesis' | 'belief' | 'knowledge' | 'derived'
  readonly temporal: TemporalGrounding
  readonly ownership: 'unknown' | 'world' | 'agent' | 'user' | 'shared'
  readonly abstraction: 'unknown' | 'instance' | 'pattern' | 'concept' | 'rule'
  readonly persistence: 'unknown' | 'working' | 'short_term' | 'long_term' | 'permanent'
  readonly retrieval: readonly RetrievalMode[]
  readonly actionability: 'unknown' | 'informational' | 'directive' | 'constraint'
}

export type TemporalGrounding =
  | { readonly kind: 'unknown' | 'timeless' }
  | { readonly kind: 'instant'; readonly at: string }
  | { readonly kind: 'interval'; readonly start: string; readonly end: string }
  | { readonly kind: 'sequence'; readonly items: readonly KnowledgeRef[] }

export type RetrievalMode =
  | 'unknown'
  | 'exact'
  | 'associative'
  | 'contextual'
  | 'causal'
  | 'procedural'

export interface KnowledgeRef {
  readonly type: 'knowledge'
  readonly id: string
}

export interface KnowledgeRelationValue {
  readonly type: KnowledgeRelationType
  readonly from: readonly KnowledgeRef[]
  readonly to: readonly KnowledgeRef[]
  readonly metadata: FactMetadata<'patchouli.knowledge-relation@1'>
}

export type KnowledgeRelationType =
  | 'supports'
  | 'contradicts'
  | 'derived_from'
  | 'generalized_from'
  | 'causes'
  | 'supersedes'

export type FactValue = ArtifactValue | KnowledgeValue | KnowledgeRelationValue
export type ArtifactEntityVersion = EntityVersion<'artifact', ArtifactValue & JsonObject>
export type KnowledgeEntityVersion = EntityVersion<'knowledge', KnowledgeValue & JsonObject>
export type KnowledgeRelationEntityVersion = EntityVersion<
  'knowledge_relation',
  KnowledgeRelationValue & JsonObject
>
