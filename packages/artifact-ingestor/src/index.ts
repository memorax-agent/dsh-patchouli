import type {
  ImageAttachmentRef,
  ImageMediaType,
} from '@deepseek-ai/dsh-attachment'
import type { Context } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {
  FactMetadata,
  JsonObject,
} from '@memorax-agent/patchouli-protocol'
import type {
  MemoryCallMeta,
  MemoryData,
  MemoryPlugin,
} from 'dsh-patchouli'
import type {} from '@deepseek-ai/dsh-fs'
import type {} from 'dsh-patchouli/storage'

export const name = 'dsh-patchouli-artifact-ingestor'

export const inject = [
  'patchouli',
  'patchouliStorage',
  'attachments',
  'fs',
] as const

const pluginId = 'artifact-ingestor'
const agentLoopSource = 'agent-loop'
const imageMediaTypes = new Set<ImageMediaType>([
  'image/png',
  'image/jpeg',
  'image/webp',
  'image/gif',
])

export interface Config {
  ingestSessionImages?: boolean
  maxFileBytes?: number
  metaFields?: {
    scope?: string
    source?: string
    session?: string
  }
  fixedMeta?: Record<string, string>
}

export const Config: z<Config> = z.object({
  ingestSessionImages: z.boolean().default(true),
  maxFileBytes: z.natural().min(1).default(32 * 1024 * 1024),
  metaFields: z.object({
    scope: z.string().min(1).default('workspace_id'),
    source: z.string().min(1).default('user_id'),
    session: z.string().min(1).default('channel_id'),
  }).default({
    scope: 'workspace_id',
    source: 'user_id',
    session: 'channel_id',
  }),
  fixedMeta: z.dict(z.string()).default({}),
})

interface WorkspaceFileResource {
  readonly kind: 'workspace-file'
  readonly path: string
  readonly mediaType?: string
  readonly name?: string
  readonly role?: 'source' | 'attachment'
}

type ArtifactReceipt = JsonObject & {
  readonly ref: JsonObject & { readonly type: 'artifact'; readonly id: string }
  readonly version: string
  readonly role: 'source' | 'attachment'
  readonly source: JsonObject & {
    readonly kind: 'image-attachment' | 'workspace-file'
    readonly id: string
  }
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function stringAttribute(meta: MemoryCallMeta, name: string): string | undefined {
  const value = meta.attributes?.[name]
  return typeof value === 'string' && value !== '' ? value : undefined
}

function databaseMeta(meta: MemoryCallMeta, config: Config): JsonObject {
  const fields = {
    scope: config.metaFields?.scope ?? 'workspace_id',
    source: config.metaFields?.source ?? 'user_id',
    session: config.metaFields?.session ?? 'channel_id',
  }
  return {
    ...config.fixedMeta,
    [fields.scope]: meta.scope,
    [fields.source]: `${meta.source.type}:${meta.source.id}`,
    [fields.session]: stringAttribute(meta, 'sessionId') ?? meta.source.type,
  }
}

function artifactMetadata(
  meta: MemoryCallMeta,
  origin: {
    readonly nativeType: string
    readonly nativeId: string
    readonly nativeRevision: string | null
    readonly source: string
  },
  extensions: Readonly<Record<string, JsonObject>>,
): FactMetadata<'patchouli.artifact@1'> {
  const now = new Date().toISOString()
  return {
    core: {
      schema: 'patchouli.artifact@1',
      scope: {
        tenant: null,
        workspace: meta.scope,
        user: null,
        session: stringAttribute(meta, 'sessionId') ?? null,
      },
      origin: {
        provider: 'deepseek-harness',
        binding: name,
        native_type: origin.nativeType,
        native_id: origin.nativeId,
        native_revision: origin.nativeRevision,
      },
      time: {
        event_at: null,
        source_created_at: null,
        source_updated_at: null,
        observed_at: now,
        ingested_at: now,
      },
      lifecycle: {
        status: 'active',
        expires_at: null,
      },
      provenance: [{
        kind: 'imported',
        actor: `plugin:${name}`,
        source: origin.source,
        recorded_at: now,
      }],
    },
    extensions,
  }
}

function isImageAttachment(value: unknown): value is ImageAttachmentRef {
  if (!isObject(value)) return false
  return typeof value.attachmentId === 'string'
    && value.attachmentId !== ''
    && typeof value.mediaType === 'string'
    && imageMediaTypes.has(value.mediaType as ImageMediaType)
    && Number.isSafeInteger(value.bytes)
    && Number(value.bytes) >= 0
    && Number.isSafeInteger(value.width)
    && Number(value.width) > 0
    && Number.isSafeInteger(value.height)
    && Number(value.height) > 0
    && (value.name === undefined || typeof value.name === 'string')
}

function imageAttachments(data: MemoryData): ImageAttachmentRef[] {
  const attachments = new Map<string, ImageAttachmentRef>()
  const pending: unknown[] = [data]
  while (pending.length > 0) {
    const value = pending.pop()
    if (Array.isArray(value)) {
      pending.push(...value)
      continue
    }
    if (!isObject(value)) continue
    if (value.type === 'image' && isImageAttachment(value.attachment)) {
      attachments.set(String(value.attachment.attachmentId), value.attachment)
      continue
    }
    pending.push(...Object.values(value))
  }
  return [...attachments.values()]
}

function workspaceFileResources(data: MemoryData): WorkspaceFileResource[] {
  if (!isObject(data) || data.resources === undefined) return []
  if (!Array.isArray(data.resources)) throw new TypeError('resources must be an array')
  return data.resources.map((resource): WorkspaceFileResource => {
    if (!isObject(resource)
      || resource.kind !== 'workspace-file'
      || typeof resource.path !== 'string'
      || resource.path.trim() === '') {
      throw new TypeError('workspace-file resources require a non-empty path')
    }
    if (resource.mediaType !== undefined
      && (typeof resource.mediaType !== 'string' || resource.mediaType.trim() === '')) {
      throw new TypeError('workspace-file mediaType must be a non-empty string')
    }
    if (resource.name !== undefined
      && (typeof resource.name !== 'string' || resource.name.trim() === '')) {
      throw new TypeError('workspace-file name must be a non-empty string')
    }
    if (resource.role !== undefined
      && resource.role !== 'source'
      && resource.role !== 'attachment') {
      throw new TypeError('workspace-file role must be source or attachment')
    }
    return {
      kind: 'workspace-file',
      path: resource.path.trim(),
      ...resource.mediaType === undefined ? {} : { mediaType: resource.mediaType.trim() },
      ...resource.name === undefined ? {} : { name: resource.name.trim() },
      ...resource.role === undefined ? {} : { role: resource.role },
    }
  })
}

function displayName(path: string): string | null {
  const name = path.split(/[\\/]/).filter(Boolean).at(-1)
  return name === undefined || name === '' ? null : name
}

function receipt(
  result: Awaited<ReturnType<Context['patchouliStorage']['uploadArtifact']>>,
  role: 'source' | 'attachment',
  source: ArtifactReceipt['source'],
): ArtifactReceipt {
  if (result.data.entity.state !== 'active') {
    throw new Error('artifact upload did not create an active entity')
  }
  return {
    ref: {
      type: 'artifact',
      id: result.data.entity.ref.id,
    },
    version: result.data.entity.version,
    role,
    source,
  }
}

export function apply(ctx: Context, config: Config): () => void {
  const maxFileBytes = config.maxFileBytes ?? 32 * 1024 * 1024
  const ingestSessionImages = config.ingestSessionImages ?? true

  async function ingestImage(
    meta: MemoryCallMeta,
    ref: ImageAttachmentRef,
    signal?: AbortSignal,
  ): Promise<ArtifactReceipt> {
    if (ref.bytes > maxFileBytes) {
      throw new Error(`image attachment exceeds the ${maxFileBytes} byte limit: ${String(ref.attachmentId)}`)
    }
    const stored = await ctx.attachments.readImage(ref, signal)
    const result = await ctx.patchouliStorage.uploadArtifact({
      meta: databaseMeta(meta, config),
      data: {
        media_type: stored.ref.mediaType,
        name: stored.ref.name ?? null,
        expected_byte_length: stored.ref.bytes,
        expected_digest: null,
        metadata: artifactMetadata(meta, {
          nativeType: 'image-attachment',
          nativeId: String(stored.ref.attachmentId),
          nativeRevision: String(stored.ref.attachmentId),
          source: `dsh-attachment:${String(stored.ref.attachmentId)}`,
        }, {
          'dsh.image': {
            width: stored.ref.width,
            height: stored.ref.height,
          },
        }),
      },
    }, stored.data)
    return receipt(result, 'attachment', {
      kind: 'image-attachment',
      id: String(stored.ref.attachmentId),
    })
  }

  async function ingestWorkspaceFile(
    meta: MemoryCallMeta,
    resource: WorkspaceFileResource,
    signal?: AbortSignal,
  ): Promise<ArtifactReceipt> {
    const workspaceRoot = stringAttribute(meta, 'workspaceRoot')
    if (workspaceRoot === undefined) {
      throw new Error('workspace-file ingestion requires the Agent session cwd')
    }
    const [workspace, target] = await Promise.all([
      ctx.fs.resolve(workspaceRoot, { signal }),
      ctx.fs.resolve(resource.path, { cwd: workspaceRoot, signal }),
    ])
    if (!ctx.fs.contains(workspace, target)) {
      throw new Error(`workspace-file path is outside the session workspace: ${resource.path}`)
    }
    const info = await ctx.fs.stat(target, signal)
    if (info?.type !== 'file') {
      throw new Error(`workspace-file path is not a regular file: ${resource.path}`)
    }
    if (info.size !== undefined && info.size > maxFileBytes) {
      throw new Error(`workspace-file exceeds the ${maxFileBytes} byte limit: ${resource.path}`)
    }
    const bytes = await ctx.fs.readBytes(target, signal, maxFileBytes)
    const role = resource.role ?? 'attachment'
    const result = await ctx.patchouliStorage.uploadArtifact({
      meta: databaseMeta(meta, config),
      data: {
        media_type: resource.mediaType ?? 'application/octet-stream',
        name: resource.name ?? displayName(target.displayPath),
        expected_byte_length: bytes.byteLength,
        expected_digest: null,
        metadata: artifactMetadata(meta, {
          nativeType: 'workspace-file',
          nativeId: resource.path,
          nativeRevision: String(info.version),
          source: `dsh-fs:${resource.path}`,
        }, {
          'dsh.file': {
            requested_path: resource.path,
            version: String(info.version),
          },
        }),
      },
    }, bytes)
    return receipt(result, role, {
      kind: 'workspace-file',
      id: resource.path,
    })
  }

  const plugin: MemoryPlugin = {
    id: pluginId,
    async update(request, context) {
      const point = stringAttribute(request.meta, 'point')
      const artifacts: ArtifactReceipt[] = []
      if (ingestSessionImages && point === 'session/turn-end') {
        for (const attachment of imageAttachments(request.data)) {
          artifacts.push(await ingestImage(request.meta, attachment, context.signal))
        }
      }
      if (point === 'tool/memory-update') {
        for (const resource of workspaceFileResources(request.data)) {
          artifacts.push(await ingestWorkspaceFile(request.meta, resource, context.signal))
        }
      }
      return { artifacts }
    },
    async retrieve() {
      throw new Error('artifact ingestor does not provide retrieval')
    },
  }

  return ctx.patchouli.register(plugin, {
    filter: call => {
      if (call.operation !== 'update' || call.meta.source.type !== agentLoopSource) return false
      const point = stringAttribute(call.meta, 'point')
      return point === 'tool/memory-update'
        || (ingestSessionImages && point === 'session/turn-end')
    },
  })
}
