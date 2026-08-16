<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import {
  Handle,
  MarkerType,
  Position,
  VueFlow,
  useVueFlow,
  type Edge,
  type Node,
  type NodeMouseEvent,
} from '@vue-flow/core'

type Locale = 'en' | 'zh'
type DiagramKind = 'backend' | 'identity'
type LayoutName = 'wide' | 'narrow' | 'phone'

interface ConceptNodeCopy {
  label: string
  role: string
  description: string
}

interface ConceptNodeSpec {
  id: string
  accent: string
  width: Record<LayoutName, number>
  position: Record<LayoutName, { x: number; y: number }>
}

interface ConceptEdgeSpec {
  id: string
  source: string
  target: string
  wideHandles: [string, string]
  narrowHandles: [string, string]
  phoneHandles: [string, string]
}

interface DiagramDefinition {
  ariaLabel: Record<Locale, string>
  instruction: Record<Locale, string>
  fitView: Record<Locale, string>
  canvasHeight: Record<LayoutName, number>
  nodes: ConceptNodeSpec[]
  edges: ConceptEdgeSpec[]
  nodeCopy: Record<Locale, Record<string, ConceptNodeCopy>>
  edgeLabels: Record<Locale, Record<string, string>>
}

interface ConceptNodeData extends ConceptNodeCopy {
  accent: string
  selected: boolean
}

const props = withDefaults(defineProps<{
  kind: DiagramKind
  locale?: Locale
}>(), {
  locale: 'en',
})

const backendDefinition: DiagramDefinition = {
  ariaLabel: {
    en: 'Backend request path from JSON-RPC to the database provider',
    zh: '从 JSON-RPC 到数据库 Provider 的后端请求路径',
  },
  instruction: {
    en: 'Select a stage to inspect its responsibility.',
    zh: '选择一个阶段查看其职责。',
  },
  fitView: { en: 'Fit view', zh: '适应画布' },
  canvasHeight: { wide: 260, narrow: 560, phone: 560 },
  nodes: [
    {
      id: 'adapter',
      accent: '#4c75a3',
      width: { wide: 170, narrow: 210, phone: 210 },
      position: { wide: { x: 20, y: 75 }, narrow: { x: 70, y: 20 }, phone: { x: 20, y: 20 } },
    },
    {
      id: 'controller',
      accent: '#8551a5',
      width: { wide: 170, narrow: 210, phone: 210 },
      position: { wide: { x: 250, y: 75 }, narrow: { x: 70, y: 150 }, phone: { x: 20, y: 150 } },
    },
    {
      id: 'policy',
      accent: '#c08b18',
      width: { wide: 170, narrow: 210, phone: 210 },
      position: { wide: { x: 480, y: 75 }, narrow: { x: 70, y: 280 }, phone: { x: 20, y: 280 } },
    },
    {
      id: 'provider',
      accent: '#a5573e',
      width: { wide: 170, narrow: 210, phone: 210 },
      position: { wide: { x: 710, y: 75 }, narrow: { x: 70, y: 410 }, phone: { x: 20, y: 410 } },
    },
  ],
  edges: [
    {
      id: 'adapter-controller',
      source: 'adapter',
      target: 'controller',
      wideHandles: ['source-right', 'target-left'],
      narrowHandles: ['source-bottom', 'target-top'],
      phoneHandles: ['source-bottom', 'target-top'],
    },
    {
      id: 'controller-policy',
      source: 'controller',
      target: 'policy',
      wideHandles: ['source-right', 'target-left'],
      narrowHandles: ['source-bottom', 'target-top'],
      phoneHandles: ['source-bottom', 'target-top'],
    },
    {
      id: 'policy-provider',
      source: 'policy',
      target: 'provider',
      wideHandles: ['source-right', 'target-left'],
      narrowHandles: ['source-bottom', 'target-top'],
      phoneHandles: ['source-bottom', 'target-top'],
    },
  ],
  nodeCopy: {
    en: {
      adapter: {
        label: 'JSON-RPC adapter',
        role: 'Wire boundary',
        description: 'Decodes the common request envelope and returns structured protocol responses.',
      },
      controller: {
        label: 'Backend controller',
        role: 'Operation control',
        description: 'Validates schemas and identity, then coordinates work units, conflicts, idempotency, and publication.',
      },
      policy: {
        label: 'Configured policy engine',
        role: 'Behavior selection',
        description: 'Interprets configured metadata fields and compiles the selected identity, consistency, ordering, and conflict rules.',
      },
      provider: {
        label: 'Provider boundary',
        role: 'Durable storage',
        description: 'Executes the planned operation transactionally against the deterministically routed local or remote provider.',
      },
    },
    zh: {
      adapter: {
        label: 'JSON-RPC Adapter',
        role: 'Wire 边界',
        description: '解析统一请求 Envelope，并返回结构化协议响应。',
      },
      controller: {
        label: 'Backend Controller',
        role: '操作控制',
        description: '验证 Schema 与身份，并协调 Work Unit、冲突、幂等和发布。',
      },
      policy: {
        label: '配置策略引擎',
        role: '行为选择',
        description: '解释配置映射的 Metadata 字段，编译选中的身份、一致性、顺序和冲突规则。',
      },
      provider: {
        label: 'Provider Boundary',
        role: '持久化存储',
        description: '在确定性路由到的本地或远程 Provider 上，以事务方式执行规划后的操作。',
      },
    },
  },
  edgeLabels: {
    en: {
      'adapter-controller': 'call',
      'controller-policy': 'metadata',
      'policy-provider': 'plan',
    },
    zh: {
      'adapter-controller': '调用',
      'controller-policy': 'Metadata',
      'policy-provider': '计划',
    },
  },
}

const identityDefinition: DiagramDefinition = {
  ariaLabel: {
    en: 'Relationship between storage identity, entity envelope, and fact value',
    zh: '存储身份、实体 Envelope 与 Fact Value 的关系',
  },
  instruction: {
    en: 'Select an identity component to inspect what it owns.',
    zh: '选择一个身份组成部分查看其职责。',
  },
  fitView: { en: 'Fit view', zh: '适应画布' },
  canvasHeight: { wide: 500, narrow: 550, phone: 625 },
  nodes: [
    {
      id: 'scope',
      accent: '#4b7d68',
      width: { wide: 190, narrow: 110, phone: 120 },
      position: { wide: { x: 20, y: 30 }, narrow: { x: 0, y: 20 }, phone: { x: 0, y: 20 } },
    },
    {
      id: 'reference',
      accent: '#4c75a3',
      width: { wide: 190, narrow: 110, phone: 120 },
      position: { wide: { x: 300, y: 30 }, narrow: { x: 120, y: 20 }, phone: { x: 130, y: 20 } },
    },
    {
      id: 'version',
      accent: '#c08b18',
      width: { wide: 190, narrow: 110, phone: 120 },
      position: { wide: { x: 580, y: 30 }, narrow: { x: 240, y: 20 }, phone: { x: 65, y: 175 } },
    },
    {
      id: 'envelope',
      accent: '#8551a5',
      width: { wide: 210, narrow: 190, phone: 210 },
      position: { wide: { x: 290, y: 195 }, narrow: { x: 80, y: 220 }, phone: { x: 20, y: 345 } },
    },
    {
      id: 'value',
      accent: '#a5573e',
      width: { wide: 270, narrow: 230, phone: 250 },
      position: { wide: { x: 260, y: 355 }, narrow: { x: 60, y: 390 }, phone: { x: 0, y: 500 } },
    },
  ],
  edges: [
    {
      id: 'scope-envelope',
      source: 'scope',
      target: 'envelope',
      wideHandles: ['source-bottom', 'target-top-left'],
      narrowHandles: ['source-bottom', 'target-top-left'],
      phoneHandles: ['source-bottom', 'target-left'],
    },
    {
      id: 'reference-envelope',
      source: 'reference',
      target: 'envelope',
      wideHandles: ['source-bottom', 'target-top'],
      narrowHandles: ['source-bottom', 'target-top'],
      phoneHandles: ['source-bottom', 'target-right'],
    },
    {
      id: 'version-envelope',
      source: 'version',
      target: 'envelope',
      wideHandles: ['source-bottom', 'target-top-right'],
      narrowHandles: ['source-bottom', 'target-top-right'],
      phoneHandles: ['source-bottom', 'target-top'],
    },
    {
      id: 'envelope-value',
      source: 'envelope',
      target: 'value',
      wideHandles: ['source-bottom', 'target-top'],
      narrowHandles: ['source-bottom', 'target-top'],
      phoneHandles: ['source-bottom', 'target-top'],
    },
  ],
  nodeCopy: {
    en: {
      scope: {
        label: 'Configured storage scope',
        role: 'Storage boundary',
        description: 'Derived from trusted request metadata and included in every database identity and authorization decision.',
      },
      reference: {
        label: 'EntityRef(type, id)',
        role: 'Stable reference',
        description: 'Names the entity type and stable entity id used by generic CRUD and typed references.',
      },
      version: {
        label: 'Opaque EntityVersion',
        role: 'Stored revision',
        description: 'Identifies an immutable stored version without exposing provider-specific revision mechanics.',
      },
      envelope: {
        label: 'Entity envelope',
        role: 'Identity authority',
        description: 'Combines scope, reference, and version as the sole authority for entity identity and storage revision.',
      },
      value: {
        label: 'Fact value',
        role: 'Typed payload',
        description: 'Carries ArtifactValue, KnowledgeValue, or KnowledgeRelationValue without duplicating entity id or revision.',
      },
    },
    zh: {
      scope: {
        label: '配置生成的存储 Scope',
        role: '存储边界',
        description: '由可信请求 Metadata 生成，并参与每个数据库身份和授权判断。',
      },
      reference: {
        label: 'EntityRef(type, id)',
        role: '稳定引用',
        description: '给出通用 CRUD 和类型化引用使用的实体类型与稳定实体 ID。',
      },
      version: {
        label: '不透明 EntityVersion',
        role: '存储版本',
        description: '标识不可变存储版本，而不暴露 Provider 特有的版本机制。',
      },
      envelope: {
        label: '实体 Envelope',
        role: '身份权威',
        description: '组合 Scope、Reference 与 Version，作为实体身份和存储版本的唯一权威。',
      },
      value: {
        label: 'Fact Value',
        role: '类型化载荷',
        description: '承载 ArtifactValue、KnowledgeValue 或 KnowledgeRelationValue，不重复实体 ID 或 Revision。',
      },
    },
  },
  edgeLabels: {
    en: {
      'scope-envelope': 'scope key',
      'reference-envelope': 'entity key',
      'version-envelope': 'revision',
      'envelope-value': 'contains',
    },
    zh: {
      'scope-envelope': 'Scope Key',
      'reference-envelope': 'Entity Key',
      'version-envelope': 'Revision',
      'envelope-value': '承载',
    },
  },
}

const definitions: Record<DiagramKind, DiagramDefinition> = {
  backend: backendDefinition,
  identity: identityDefinition,
}

const definition = computed(() => definitions[props.kind])
const layout = ref<LayoutName>('wide')
const compact = computed(() => layout.value !== 'wide')
const selectedNodeId = ref(definition.value.nodes[0].id)
const root = ref<HTMLElement>()
const flowId = `patchouli-concept-${props.kind}-${props.locale}`
const { fitView } = useVueFlow(flowId)
let resizeObserver: ResizeObserver | undefined
let refitTimer: ReturnType<typeof setTimeout> | undefined

const nodes = computed<Node<ConceptNodeData>[]>(() => definition.value.nodes.map((node) => ({
  id: node.id,
  type: 'concept',
  position: node.position[layout.value],
  width: node.width[layout.value],
  height: props.kind === 'identity'
    && layout.value !== 'wide'
    && ['scope', 'reference', 'version'].includes(node.id)
    ? 120
    : 88,
  draggable: false,
  connectable: false,
  selectable: false,
  focusable: false,
  data: {
    ...definition.value.nodeCopy[props.locale][node.id],
    accent: node.accent,
    selected: selectedNodeId.value === node.id,
  },
})))

const edges = computed<Edge[]>(() => definition.value.edges.map((edge) => {
  const handles = layout.value === 'wide'
    ? edge.wideHandles
    : layout.value === 'narrow'
      ? edge.narrowHandles
      : edge.phoneHandles
  const selected = edge.source === selectedNodeId.value || edge.target === selectedNodeId.value
  const source = definition.value.nodeCopy[props.locale][edge.source].label
  const target = definition.value.nodeCopy[props.locale][edge.target].label
  const label = definition.value.edgeLabels[props.locale][edge.id]
  return {
    id: edge.id,
    source: edge.source,
    target: edge.target,
    sourceHandle: handles[0],
    targetHandle: handles[1],
    type: 'smoothstep',
    label,
    ariaLabel: `${source}: ${label} → ${target}`,
    focusable: false,
    selectable: false,
    class: ['pc-flow-edge', selected ? 'is-selected' : ''].filter(Boolean).join(' '),
    style: { stroke: selected ? '#8551a5' : '#aaa2ad' },
    markerEnd: {
      type: MarkerType.ArrowClosed,
      color: selected ? '#8551a5' : '#aaa2ad',
      width: 16,
      height: 16,
    },
    labelShowBg: true,
    labelBgPadding: [5, 3],
    labelBgBorderRadius: 5,
  }
}))

const selectedNode = computed(() => definition.value.nodeCopy[props.locale][selectedNodeId.value])
const canvasHeight = computed(() => definition.value.canvasHeight[layout.value])

function selectNode({ node }: NodeMouseEvent): void {
  selectedNodeId.value = node.id
}

function refit(): void {
  void fitView({ padding: layout.value === 'phone' ? 0.04 : compact.value ? 0.06 : 0.1, duration: 240 })
}

function syncLayout(width: number): void {
  const nextLayout: LayoutName = width <= 400 ? 'phone' : width <= 600 ? 'narrow' : 'wide'
  if (layout.value === nextLayout) return
  layout.value = nextLayout
  void nextTick(() => {
    if (refitTimer) clearTimeout(refitTimer)
    refitTimer = setTimeout(refit, 100)
  })
}

onMounted(() => {
  if (root.value) syncLayout(root.value.clientWidth)
  resizeObserver = new ResizeObserver(([entry]) => {
    if (entry) syncLayout(entry.contentRect.width)
  })
  if (root.value) resizeObserver.observe(root.value)
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  if (refitTimer) clearTimeout(refitTimer)
})
</script>

<template>
  <section ref="root" class="pc-flow" :aria-label="definition.ariaLabel[props.locale]">
    <div class="pc-flow__toolbar">
      <p>{{ definition.instruction[props.locale] }}</p>
      <button type="button" @click="refit">
        {{ definition.fitView[props.locale] }}
      </button>
    </div>

    <div class="pc-flow__canvas" :style="{ height: `${canvasHeight}px` }">
      <VueFlow
        :key="layout"
        :id="flowId"
        :nodes="nodes"
        :edges="edges"
        :nodes-draggable="false"
        :nodes-connectable="false"
        :elements-selectable="false"
        :zoom-on-scroll="false"
        :zoom-on-double-click="false"
        :pan-on-scroll="false"
        :prevent-scrolling="false"
        :min-zoom="0.62"
        :max-zoom="1.2"
        fit-view-on-init
        class="pc-flow__surface"
        @node-click="selectNode"
        @nodes-initialized="refit"
      >
        <template #node-concept="{ data }">
          <Handle id="source-right" type="source" :position="Position.Right" :connectable="false" class="pc-handle" />
          <Handle id="target-left" type="target" :position="Position.Left" :connectable="false" class="pc-handle" />
          <Handle id="target-right" type="target" :position="Position.Right" :connectable="false" class="pc-handle" />
          <Handle id="source-bottom" type="source" :position="Position.Bottom" :connectable="false" class="pc-handle" />
          <Handle id="target-top" type="target" :position="Position.Top" :connectable="false" class="pc-handle" />
          <Handle id="target-top-left" type="target" :position="Position.Top" :connectable="false" :style="{ left: '22%' }" class="pc-handle" />
          <Handle id="target-top-right" type="target" :position="Position.Top" :connectable="false" :style="{ left: '78%' }" class="pc-handle" />
          <button
            type="button"
            class="pc-node"
            :style="{ '--pc-accent': data.accent }"
            :aria-pressed="data.selected"
          >
            <span>{{ data.role }}</span>
            <strong>{{ data.label }}</strong>
          </button>
        </template>
      </VueFlow>
    </div>

    <div class="pc-flow__detail" aria-live="polite">
      <span>{{ selectedNode.role }}</span>
      <strong>{{ selectedNode.label }}</strong>
      <p>{{ selectedNode.description }}</p>
    </div>
  </section>
</template>

<style scoped>
.pc-flow {
  width: min(860px, calc(100vw - 48px));
  margin: 24px 0 34px 50%;
  color: var(--vp-c-text-1);
  transform: translateX(-50%);
}

.pc-flow__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 10px;
}

.pc-flow__toolbar p {
  margin: 0;
  color: var(--vp-c-text-2);
  font-size: 13px;
}

.pc-flow__toolbar button {
  flex: 0 0 auto;
  min-height: 34px;
  padding: 6px 11px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  color: var(--vp-c-text-2);
  font: inherit;
  background: var(--vp-c-bg);
  cursor: pointer;
  transition: border-color 160ms ease-out, color 160ms ease-out;
}

.pc-flow__toolbar button:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}

.pc-flow__toolbar button:focus-visible,
.pc-node:focus-visible {
  outline: 3px solid var(--vp-c-brand-soft);
  outline-offset: 2px;
}

.pc-flow__canvas {
  overflow: hidden;
  border: 1px solid var(--vp-c-divider);
  border-radius: 14px;
  background-color: var(--vp-c-bg-soft);
  background-image:
    linear-gradient(var(--vp-c-divider) 1px, transparent 1px),
    linear-gradient(90deg, var(--vp-c-divider) 1px, transparent 1px);
  background-size: 24px 24px;
}

.pc-flow__surface {
  background: color-mix(in srgb, var(--vp-c-bg) 82%, transparent);
}

.pc-node {
  position: relative;
  width: 100%;
  height: 100%;
  padding: 13px 14px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  color: var(--vp-c-text-1);
  text-align: left;
  background: var(--vp-c-bg);
  cursor: pointer;
  transition: border-color 160ms ease-out, background-color 160ms ease-out;
}

.pc-node::before {
  position: absolute;
  top: 13px;
  right: 13px;
  width: 8px;
  height: 8px;
  border-radius: 2px;
  background: var(--pc-accent);
  content: '';
}

.pc-node:hover {
  border-color: color-mix(in srgb, var(--pc-accent) 68%, var(--vp-c-divider));
}

.pc-node[aria-pressed='true'] {
  border-color: var(--pc-accent);
  background: color-mix(in srgb, var(--pc-accent) 9%, var(--vp-c-bg));
}

.pc-node span {
  display: block;
  margin: 0 18px 7px 0;
  color: var(--vp-c-text-2);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.07em;
  line-height: 1.2;
  text-transform: uppercase;
}

.pc-node strong {
  display: block;
  font-size: 13px;
  line-height: 1.35;
}

.pc-handle {
  width: 6px;
  height: 6px;
  border: 0;
  background: var(--vp-c-brand-1);
  opacity: 0;
  pointer-events: none;
}

:deep(.vue-flow__node-concept) {
  border: 0;
  background: transparent;
}

:deep(.pc-flow-edge .vue-flow__edge-path) {
  stroke-width: 1.7;
  transition: stroke 160ms ease-out;
}

:deep(.pc-flow-edge.is-selected .vue-flow__edge-path) {
  stroke-width: 2.5;
}

:deep(.vue-flow__edge-text) {
  fill: var(--vp-c-text-2);
  font-size: 9px;
  font-weight: 600;
}

:deep(.vue-flow__edge-textbg) {
  fill: var(--vp-c-bg);
  fill-opacity: 0.94;
}

:deep(.vue-flow__attribution) {
  display: none;
}

.pc-flow__detail {
  display: grid;
  grid-template-columns: 130px minmax(170px, 220px) 1fr;
  align-items: baseline;
  gap: 14px;
  min-height: 78px;
  padding: 16px 18px;
  border-bottom: 1px solid var(--vp-c-divider);
}

.pc-flow__detail > span {
  color: var(--vp-c-brand-1);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.pc-flow__detail strong {
  font-size: 14px;
}

.pc-flow__detail p {
  margin: 0;
  color: var(--vp-c-text-2);
  font-size: 13px;
  line-height: 1.55;
}

@media (max-width: 700px) {
  .pc-flow {
    width: 100%;
    margin: 22px 0 30px;
    transform: none;
  }

  .pc-flow__toolbar {
    align-items: flex-start;
  }

  .pc-flow__toolbar button {
    min-height: 32px;
    padding: 5px 9px;
    font-size: 12px;
  }

  .pc-flow__detail {
    grid-template-columns: 1fr;
    gap: 5px;
    min-height: 0;
    padding: 14px 2px 16px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .pc-flow *,
  :deep(.vue-flow__edge-path) {
    transition-duration: 0s !important;
  }
}
</style>
