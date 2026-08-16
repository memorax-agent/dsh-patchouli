<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import {
  Handle,
  MarkerType,
  Position,
  VueFlow,
  useVueFlow,
  type Edge,
  type EdgeMouseEvent,
  type Node,
  type NodeDragEvent,
  type NodeMouseEvent,
} from '@vue-flow/core'
import architectureSource from './patchouli-architecture.data.json'

type Locale = 'en' | 'zh'
type Route = 'overview' | 'retrieve' | 'update' | 'subscribe' | 'artifact'
type EditableRoute = Exclude<Route, 'overview'>
type LayoutName = 'wide' | 'narrow'
type SaveState = 'clean' | 'dirty' | 'saving' | 'saved' | 'error'
type EditorSection = 'node' | 'edge'
type EdgeType = 'smoothstep' | 'step' | 'straight' | 'default'
type EdgeMarker = 'none' | 'arrow' | 'closed'
type SourceHandle = 'out-left-top' | 'out-right-bottom' | 'out-top-left' | 'out-bottom-right'
type TargetHandle = 'in-left-bottom' | 'in-right-top' | 'in-top-right' | 'in-bottom-left'

interface Placement {
  position: { x: number; y: number }
  width: number
  height: number
}

interface EdgeStyle {
  type: EdgeType
  marker: EdgeMarker
  markerSize: number
  color: string
}

interface ArchitectureNode {
  id: string
  insideBoundary: boolean
}

interface EdgeSpec {
  id: string
  source: string
  target: string
  handles: Record<LayoutName, [SourceHandle, TargetHandle]>
}

interface ArchitectureData {
  canvas: Record<LayoutName, { height: number }>
  nodes: ArchitectureNode[]
  edges: EdgeSpec[]
  layouts: Record<LayoutName, Record<string, Placement>>
  edgeLabels: Record<Locale, Record<string, string>>
  nodeCopy: Record<Locale, Record<string, NodeCopy>>
  edgeRoutes: Record<string, EditableRoute[]>
  edgeStyles: Record<string, EdgeStyle>
}

interface NodeCopy {
  label: string
  role: string
  description: string
}

interface DiagramCopy {
  ariaLabel: string
  routeLabel: string
  fitView: string
  instruction: string
  dsh: string
  routes: Record<Route, string>
  nodes: Record<string, NodeCopy>
  edges: Record<string, string>
  editor: {
    open: string
    close: string
    title: string
    nodeSection: string
    edgeSection: string
    wide: string
    narrow: string
    node: string
    edge: string
    geometry: string
    x: string
    y: string
    width: string
    height: string
    canvasHeight: string
    english: string
    chinese: string
    label: string
    role: string
    description: string
    paths: string
    edgeType: string
    smoothStep: string
    step: string
    straight: string
    bezier: string
    marker: string
    markerNone: string
    markerArrow: string
    markerClosed: string
    markerSize: string
    color: string
    topology: string
    source: string
    target: string
    sourceHandle: string
    targetHandle: string
    reverseEdge: string
    addNode: string
    deleteNode: string
    addEdge: string
    deleteEdge: string
    insideBoundary: string
    noEdges: string
    left: string
    right: string
    top: string
    bottom: string
    upper: string
    lower: string
    englishLabel: string
    chineseLabel: string
    save: string
    saving: string
    saved: string
    unsaved: string
    reset: string
    clean: string
    error: string
  }
}

interface ModuleNodeData extends NodeCopy {
  kind: string
  selected: boolean
}

const props = withDefaults(defineProps<{ locale?: Locale }>(), {
  locale: 'en',
})

function cloneData(data: ArchitectureData): ArchitectureData {
  return JSON.parse(JSON.stringify(data)) as ArchitectureData
}

const sourceData = architectureSource as ArchitectureData
const savedData = ref<ArchitectureData>(cloneData(sourceData))
const draftData = ref<ArchitectureData>(cloneData(savedData.value))

const translations: Record<Locale, Omit<DiagramCopy, 'dsh' | 'nodes' | 'edges'>> = {
  en: {
    ariaLabel: 'Interactive diagram of Patchouli module calls',
    routeLabel: 'Highlight a call path',
    fitView: 'Fit view',
    instruction: 'Choose a path, then select a module to inspect its responsibility.',
    routes: {
      overview: 'Overview',
      retrieve: 'Retrieve',
      update: 'Update',
      subscribe: 'Subscribe',
      artifact: 'Artifact',
    },
    editor: {
      open: 'Edit diagram',
      close: 'Exit editor',
      title: 'Diagram editor',
      nodeSection: 'Nodes',
      edgeSection: 'Edges',
      wide: 'Desktop',
      narrow: 'Mobile',
      node: 'Node',
      edge: 'Edge',
      geometry: 'Geometry',
      x: 'X',
      y: 'Y',
      width: 'Width',
      height: 'Height',
      canvasHeight: 'Canvas height',
      english: 'English',
      chinese: 'Chinese',
      label: 'Label',
      role: 'Role',
      description: 'Description',
      paths: 'Visible in paths',
      edgeType: 'Line type',
      smoothStep: 'Smooth step',
      step: 'Step',
      straight: 'Straight',
      bezier: 'Bezier',
      marker: 'Arrow',
      markerNone: 'None',
      markerArrow: 'Open',
      markerClosed: 'Closed',
      markerSize: 'Arrow size',
      color: 'Color',
      topology: 'Connection',
      source: 'Source node',
      target: 'Target node',
      sourceHandle: 'Source handle',
      targetHandle: 'Target handle',
      reverseEdge: 'Reverse direction',
      addNode: 'Add node',
      deleteNode: 'Delete node',
      addEdge: 'Add edge',
      deleteEdge: 'Delete edge',
      insideBoundary: 'Inside DeepSeek Harness',
      noEdges: 'No edges yet. Add one to define a call.',
      left: 'Left',
      right: 'Right',
      top: 'Top',
      bottom: 'Bottom',
      upper: 'Upper',
      lower: 'Lower',
      englishLabel: 'English annotation',
      chineseLabel: 'Chinese annotation',
      save: 'Save to source',
      saving: 'Saving…',
      saved: 'Saved to source.',
      unsaved: 'Unsaved changes.',
      reset: 'Discard changes',
      clean: 'No changes.',
      error: 'Save failed.',
    },
  },
  zh: {
    ariaLabel: 'Patchouli 模块调用关系交互图',
    routeLabel: '高亮调用路径',
    fitView: '适应画布',
    instruction: '选择一条路径，再选择模块查看其职责。',
    routes: {
      overview: '总览',
      retrieve: '读取',
      update: '写入',
      subscribe: '订阅',
      artifact: 'Artifact',
    },
    editor: {
      open: '编辑图表',
      close: '退出编辑',
      title: '图表编辑器',
      nodeSection: '节点',
      edgeSection: '连线',
      wide: '桌面布局',
      narrow: '移动布局',
      node: '节点',
      edge: '连线',
      geometry: '几何参数',
      x: 'X 坐标',
      y: 'Y 坐标',
      width: '宽度',
      height: '高度',
      canvasHeight: '画布高度',
      english: '英文',
      chinese: '中文',
      label: '名称',
      role: '角色',
      description: '说明',
      paths: '显示路径',
      edgeType: '线型',
      smoothStep: '平滑折线',
      step: '折线',
      straight: '直线',
      bezier: '贝塞尔曲线',
      marker: '箭头',
      markerNone: '无',
      markerArrow: '开放箭头',
      markerClosed: '闭合箭头',
      markerSize: '箭头大小',
      color: '颜色',
      topology: '连接关系',
      source: '起点节点',
      target: '终点节点',
      sourceHandle: '起点 Handle',
      targetHandle: '终点 Handle',
      reverseEdge: '反转方向',
      addNode: '新增节点',
      deleteNode: '删除节点',
      addEdge: '新增连线',
      deleteEdge: '删除连线',
      insideBoundary: '位于 DeepSeek Harness 内部',
      noEdges: '还没有连线。新增一条连线来定义调用关系。',
      left: '左侧',
      right: '右侧',
      top: '上侧',
      bottom: '下侧',
      upper: '靠上',
      lower: '靠下',
      englishLabel: '英文注释',
      chineseLabel: '中文注释',
      save: '保存到源码',
      saving: '正在保存…',
      saved: '已保存到源码。',
      unsaved: '有尚未保存的修改。',
      reset: '放弃修改',
      clean: '没有修改。',
      error: '保存失败。',
    },
  },
}

const copy = computed<DiagramCopy>(() => ({
  ...translations[props.locale],
  dsh: draftData.value.nodeCopy[props.locale].boundary.label,
  nodes: draftData.value.nodeCopy[props.locale],
  edges: draftData.value.edgeLabels[props.locale],
}))
const initialNodeId = sourceData.nodes.find((node) => node.id === 'coordinator')?.id ?? sourceData.nodes[0]?.id ?? null
const activeRoute = ref<Route>('overview')
const selectedNodeId = ref<string | null>(initialNodeId)
const selectedLayoutNodeId = ref(initialNodeId ?? 'boundary')
const selectedEdgeId = ref<string | null>(sourceData.edges[0]?.id ?? null)
const editorSection = ref<EditorSection>('node')
const autoCompact = ref(false)
const editorLayout = ref<LayoutName>('wide')
const authoring = ref(false)
const saveState = ref<SaveState>('clean')
const saveError = ref('')
const compact = computed(() => authoring.value ? editorLayout.value === 'narrow' : autoCompact.value)
const architectureRoot = ref<HTMLElement>()
const flowId = `patchouli-architecture-${props.locale}`
const { fitView } = useVueFlow(flowId)
const isDevelopment = import.meta.env.DEV
let resizeObserver: ResizeObserver | undefined
let refitTimer: ReturnType<typeof setTimeout> | undefined

const routes: Route[] = ['overview', 'retrieve', 'update', 'subscribe', 'artifact']
const editableRoutes: EditableRoute[] = ['retrieve', 'update', 'subscribe', 'artifact']
const locales: Locale[] = ['en', 'zh']
const sourceHandles: SourceHandle[] = ['out-left-top', 'out-right-bottom', 'out-top-left', 'out-bottom-right']
const targetHandles: TargetHandle[] = ['in-left-bottom', 'in-right-top', 'in-top-right', 'in-bottom-left']
const nodeIds = computed(() => draftData.value.nodes.map((node) => node.id))
const layoutNodeIds = computed(() => ['boundary', ...nodeIds.value])
const edgeSpecs = computed(() => draftData.value.edges)

const saveStatus = computed(() => {
  if (saveState.value === 'saving') return copy.value.editor.saving
  if (saveState.value === 'saved') return copy.value.editor.saved
  if (saveState.value === 'dirty') return copy.value.editor.unsaved
  if (saveState.value === 'error') return saveError.value || copy.value.editor.error
  return copy.value.editor.clean
})

const activeNodeIds = computed(() => {
  if (activeRoute.value === 'overview') return new Set(nodeIds.value)
  const active = new Set<string>()
  for (const edge of edgeSpecs.value) {
    if (draftData.value.edgeRoutes[edge.id].includes(activeRoute.value as EditableRoute)) {
      active.add(edge.source)
      active.add(edge.target)
    }
  }
  return active
})

const nodes = computed<Node<ModuleNodeData>[]>(() => {
  const layout = draftData.value.layouts[compact.value ? 'narrow' : 'wide']

  return [
    {
      id: 'dsh',
      type: 'boundary',
      position: layout.boundary.position,
      width: layout.boundary.width,
      height: layout.boundary.height,
      selectable: false,
      focusable: false,
      zIndex: 0,
      data: {
        label: copy.value.dsh,
        role: '',
        description: '',
        kind: 'coordinator',
        selected: false,
      },
    },
    ...draftData.value.nodes.map((definition): Node<ModuleNodeData> => {
      const id = definition.id
      const placement = layout[id]
      const insideDsh = definition.insideBoundary
      return {
        id,
        type: 'module',
        position: placement.position,
        width: placement.width,
        height: placement.height,
        parentNode: insideDsh ? 'dsh' : undefined,
        extent: insideDsh ? 'parent' : undefined,
        draggable: authoring.value,
        connectable: false,
        selectable: false,
        focusable: false,
        zIndex: 2,
        class: [
          `pa-flow-node--${id}`,
          activeNodeIds.value.has(id) ? 'is-active' : 'is-muted',
          selectedNodeId.value === id ? 'is-selected' : '',
        ].filter(Boolean).join(' '),
        data: {
          ...copy.value.nodes[id],
          kind: id,
          selected: selectedNodeId.value === id,
        },
      }
    }),
  ]
})

const edges = computed<Edge[]>(() => edgeSpecs.value.map((edge) => {
  const active = activeRoute.value === 'overview'
    || draftData.value.edgeRoutes[edge.id].includes(activeRoute.value as EditableRoute)
  const handles = edge.handles[compact.value ? 'narrow' : 'wide']
  const edgeStyle = draftData.value.edgeStyles[edge.id]
  const selected = authoring.value && selectedEdgeId.value === edge.id
  return {
    id: edge.id,
    source: edge.source,
    target: edge.target,
    sourceHandle: handles[0],
    targetHandle: handles[1],
    type: edgeStyle.type,
    label: selected || (activeRoute.value !== 'overview' && active) ? copy.value.edges[edge.id] : undefined,
    animated: activeRoute.value !== 'overview' && active,
    selectable: authoring.value,
    focusable: false,
    class: [
      'pa-flow-edge',
      active ? 'is-active' : 'is-muted',
      selected ? 'is-selected' : '',
    ].filter(Boolean).join(' '),
    style: { stroke: active ? edgeStyle.color : '#aaa2ad' },
    markerEnd: edgeStyle.marker === 'none'
      ? undefined
      : {
          type: edgeStyle.marker === 'arrow' ? MarkerType.Arrow : MarkerType.ArrowClosed,
          color: active ? edgeStyle.color : '#aaa2ad',
          width: edgeStyle.markerSize,
          height: edgeStyle.markerSize,
        },
    labelShowBg: true,
    labelBgPadding: [5, 3],
    labelBgBorderRadius: 5,
    ariaLabel: `${copy.value.nodes[edge.source].label}: ${copy.value.edges[edge.id]} → ${copy.value.nodes[edge.target].label}`,
  }
}))

const selectedNode = computed(() => selectedNodeId.value ? copy.value.nodes[selectedNodeId.value] : undefined)
const selectedPlacement = computed(() => draftData.value.layouts[editorLayout.value][selectedLayoutNodeId.value])
const selectedNodeCopy = computed(() => ({
  en: draftData.value.nodeCopy.en[selectedLayoutNodeId.value],
  zh: draftData.value.nodeCopy.zh[selectedLayoutNodeId.value],
}))
const selectedNodeDefinition = computed(() => draftData.value.nodes.find((node) => node.id === selectedLayoutNodeId.value))
const selectedEdgeSpec = computed(() => draftData.value.edges.find((edge) => edge.id === selectedEdgeId.value))
const canAddNode = computed(() => draftData.value.nodes.length < 64)
const canAddEdge = computed(() => draftData.value.nodes.length > 0 && draftData.value.edges.length < 256)

function selectRoute(route: Route): void {
  activeRoute.value = route
}

function selectNode(id: string): void {
  selectedNodeId.value = id
  if (authoring.value) selectedLayoutNodeId.value = id
}

function selectLayoutNode(id: string): void {
  selectedLayoutNodeId.value = id
  if (id !== 'boundary') selectedNodeId.value = id
}

function updateSelectedLayoutNode(event: Event): void {
  selectLayoutNode((event.target as HTMLSelectElement).value)
}

function handleNodeClick({ node }: NodeMouseEvent): void {
  if (node.id !== 'dsh') {
    selectNode(node.id)
    if (authoring.value) editorSection.value = 'node'
  }
}

function handleNodeDragStop({ node }: NodeDragEvent): void {
  if (!authoring.value || node.id === 'dsh') return
  const placement = draftData.value.layouts[editorLayout.value][node.id]
  selectedLayoutNodeId.value = node.id
  placement.position = {
    x: Math.round(node.position.x),
    y: Math.round(node.position.y),
  }
  markDirty()
}

function handleEdgeClick({ edge }: EdgeMouseEvent): void {
  if (authoring.value) {
    selectedEdgeId.value = edge.id
    editorSection.value = 'edge'
  }
}

function markDirty(): void {
  saveState.value = 'dirty'
  saveError.value = ''
}

function updateEdgeLabel(locale: Locale, event: Event): void {
  if (!selectedEdgeId.value) return
  draftData.value.edgeLabels[locale][selectedEdgeId.value] = (event.target as HTMLInputElement).value
  markDirty()
}

function updatePlacement(field: 'x' | 'y' | 'width' | 'height', event: Event): void {
  const value = Number((event.target as HTMLInputElement).value)
  if (!Number.isFinite(value)) return
  const normalized = field === 'width' || field === 'height'
    ? Math.max(field === 'width' ? 80 : 48, Math.round(value))
    : Math.round(value)
  const placement = draftData.value.layouts[editorLayout.value][selectedLayoutNodeId.value]
  if (field === 'x' || field === 'y') placement.position[field] = normalized
  else placement[field] = normalized
  markDirty()
}

function updateCanvasHeight(event: Event): void {
  const value = Number((event.target as HTMLInputElement).value)
  if (!Number.isFinite(value)) return
  draftData.value.canvas[editorLayout.value].height = Math.min(2000, Math.max(320, Math.round(value)))
  markDirty()
}

function updateNodeCopy(locale: Locale, field: keyof NodeCopy, event: Event): void {
  const target = event.target as HTMLInputElement | HTMLTextAreaElement
  draftData.value.nodeCopy[locale][selectedLayoutNodeId.value][field] = target.value
  markDirty()
}

function nextId(prefix: string, existing: string[]): string {
  let suffix = 1
  while (existing.includes(`${prefix}-${suffix}`)) suffix += 1
  return `${prefix}-${suffix}`
}

function addNode(): void {
  if (!canAddNode.value) return
  const id = nextId('node', nodeIds.value)
  const offset = (draftData.value.nodes.length % 6) * 18
  draftData.value.nodes.push({ id, insideBoundary: true })
  draftData.value.layouts.wide[id] = {
    position: { x: 44 + offset, y: 92 + offset },
    width: 174,
    height: 86,
  }
  draftData.value.layouts.narrow[id] = {
    position: { x: 24, y: 80 + offset },
    width: 174,
    height: 86,
  }
  draftData.value.nodeCopy.en[id] = {
    label: 'New node',
    role: 'Module',
    description: 'Describe this module responsibility.',
  }
  draftData.value.nodeCopy.zh[id] = {
    label: '新节点',
    role: '模块',
    description: '描述这个模块的职责。',
  }
  selectedNodeId.value = id
  selectedLayoutNodeId.value = id
  editorSection.value = 'node'
  markDirty()
  void nextTick(refit)
}

function removeEdge(id: string): void {
  draftData.value.edges = draftData.value.edges.filter((edge) => edge.id !== id)
  for (const locale of locales) delete draftData.value.edgeLabels[locale][id]
  delete draftData.value.edgeRoutes[id]
  delete draftData.value.edgeStyles[id]
}

function deleteSelectedNode(): void {
  const id = selectedLayoutNodeId.value
  if (id === 'boundary') return
  const incidentEdges = draftData.value.edges.filter((edge) => edge.source === id || edge.target === id)
  for (const edge of incidentEdges) removeEdge(edge.id)
  draftData.value.nodes = draftData.value.nodes.filter((node) => node.id !== id)
  for (const layout of ['wide', 'narrow'] as const) delete draftData.value.layouts[layout][id]
  for (const locale of locales) delete draftData.value.nodeCopy[locale][id]

  const nextNodeId = draftData.value.nodes[0]?.id ?? null
  selectedNodeId.value = nextNodeId
  selectedLayoutNodeId.value = nextNodeId ?? 'boundary'
  if (!selectedEdgeId.value || !draftData.value.edges.some((edge) => edge.id === selectedEdgeId.value)) {
    selectedEdgeId.value = draftData.value.edges[0]?.id ?? null
  }
  markDirty()
  void nextTick(refit)
}

function toggleInsideBoundary(event: Event): void {
  if (!selectedNodeDefinition.value) return
  const insideBoundary = (event.target as HTMLInputElement).checked
  if (selectedNodeDefinition.value.insideBoundary === insideBoundary) return

  for (const layoutName of ['wide', 'narrow'] as const) {
    const layout = draftData.value.layouts[layoutName]
    const placement = layout[selectedNodeDefinition.value.id]
    const boundary = layout.boundary
    const offset = insideBoundary ? -1 : 1
    const x = placement.position.x + boundary.position.x * offset
    const y = placement.position.y + boundary.position.y * offset
    placement.position = insideBoundary
      ? {
          x: Math.min(Math.max(0, x), Math.max(0, boundary.width - placement.width)),
          y: Math.min(Math.max(0, y), Math.max(0, boundary.height - placement.height)),
        }
      : { x, y }
  }

  selectedNodeDefinition.value.insideBoundary = insideBoundary
  markDirty()
  void nextTick(refit)
}

function addEdge(): void {
  if (!canAddEdge.value) return
  const id = nextId('edge', draftData.value.edges.map((edge) => edge.id))
  const source = selectedNodeId.value && nodeIds.value.includes(selectedNodeId.value)
    ? selectedNodeId.value
    : nodeIds.value[0]
  const target = nodeIds.value.find((nodeId) => nodeId !== source) ?? source
  draftData.value.edges.push({
    id,
    source,
    target,
    handles: {
      wide: ['out-right-bottom', 'in-left-bottom'],
      narrow: ['out-bottom-right', 'in-top-right'],
    },
  })
  draftData.value.edgeLabels.en[id] = 'New call'
  draftData.value.edgeLabels.zh[id] = '新调用'
  draftData.value.edgeRoutes[id] = []
  draftData.value.edgeStyles[id] = {
    type: 'smoothstep',
    marker: 'closed',
    markerSize: 17,
    color: '#8551a5',
  }
  selectedEdgeId.value = id
  editorSection.value = 'edge'
  markDirty()
  void nextTick(refit)
}

function deleteSelectedEdge(): void {
  if (!selectedEdgeId.value) return
  const index = draftData.value.edges.findIndex((edge) => edge.id === selectedEdgeId.value)
  removeEdge(selectedEdgeId.value)
  selectedEdgeId.value = draftData.value.edges[Math.min(index, draftData.value.edges.length - 1)]?.id ?? null
  markDirty()
  void nextTick(refit)
}

function updateEdgeEndpoint(field: 'source' | 'target', event: Event): void {
  if (!selectedEdgeSpec.value) return
  selectedEdgeSpec.value[field] = (event.target as HTMLSelectElement).value
  markDirty()
}

function updateEdgeHandle(role: 'source' | 'target', event: Event): void {
  if (!selectedEdgeSpec.value) return
  const value = (event.target as HTMLSelectElement).value
  if (role === 'source') selectedEdgeSpec.value.handles[editorLayout.value][0] = value as SourceHandle
  else selectedEdgeSpec.value.handles[editorLayout.value][1] = value as TargetHandle
  markDirty()
}

function reverseSelectedEdge(): void {
  if (!selectedEdgeSpec.value) return
  const sourceToTarget: Record<SourceHandle, TargetHandle> = {
    'out-left-top': 'in-left-bottom',
    'out-right-bottom': 'in-right-top',
    'out-top-left': 'in-top-right',
    'out-bottom-right': 'in-bottom-left',
  }
  const targetToSource: Record<TargetHandle, SourceHandle> = {
    'in-left-bottom': 'out-left-top',
    'in-right-top': 'out-right-bottom',
    'in-top-right': 'out-top-left',
    'in-bottom-left': 'out-bottom-right',
  }
  const edge = selectedEdgeSpec.value
  const previousSource = edge.source
  edge.source = edge.target
  edge.target = previousSource
  for (const layout of ['wide', 'narrow'] as const) {
    const [sourceHandle, targetHandle] = edge.handles[layout]
    edge.handles[layout] = [targetToSource[targetHandle], sourceToTarget[sourceHandle]]
  }
  markDirty()
}

function handleLabel(handle: SourceHandle | TargetHandle): string {
  const [, side, alignment] = handle.split('-')
  const sideLabel = copy.value.editor[side as 'left' | 'right' | 'top' | 'bottom']
  const alignmentLabel = side === 'left' || side === 'right'
    ? copy.value.editor[alignment === 'top' ? 'upper' : 'lower']
    : copy.value.editor[alignment as 'left' | 'right']
  return `${sideLabel} · ${alignmentLabel}`
}

function toggleEdgeRoute(route: EditableRoute, event: Event): void {
  if (!selectedEdgeId.value) return
  const checked = (event.target as HTMLInputElement).checked
  const selected = new Set(draftData.value.edgeRoutes[selectedEdgeId.value])
  if (checked) selected.add(route)
  else selected.delete(route)
  draftData.value.edgeRoutes[selectedEdgeId.value] = editableRoutes.filter((candidate) => selected.has(candidate))
  markDirty()
}

function updateEdgeStyle(field: keyof EdgeStyle, event: Event): void {
  if (!selectedEdgeId.value) return
  const target = event.target as HTMLInputElement | HTMLSelectElement
  const style = draftData.value.edgeStyles[selectedEdgeId.value]
  if (field === 'markerSize') {
    const value = Number(target.value)
    if (!Number.isFinite(value)) return
    style.markerSize = Math.min(40, Math.max(8, Math.round(value)))
  }
  else if (field === 'type') style.type = target.value as EdgeType
  else if (field === 'marker') style.marker = target.value as EdgeMarker
  else style.color = target.value
  markDirty()
}

function setEditorLayout(layout: LayoutName): void {
  editorLayout.value = layout
  void nextTick(refit)
}

function normalizeSelections(): void {
  if (selectedLayoutNodeId.value !== 'boundary' && !nodeIds.value.includes(selectedLayoutNodeId.value)) {
    selectedLayoutNodeId.value = 'boundary'
  }
  if (!selectedNodeId.value || !nodeIds.value.includes(selectedNodeId.value)) {
    selectedNodeId.value = nodeIds.value[0] ?? null
  }
  if (!selectedEdgeId.value || !draftData.value.edges.some((edge) => edge.id === selectedEdgeId.value)) {
    selectedEdgeId.value = draftData.value.edges[0]?.id ?? null
  }
}

function setAuthoring(enabled: boolean): void {
  if (!isDevelopment) return
  authoring.value = enabled
  if (enabled) {
    editorLayout.value = autoCompact.value ? 'narrow' : 'wide'
    normalizeSelections()
  }

  const url = new URL(window.location.href)
  if (enabled) url.searchParams.set('edit-architecture', '1')
  else url.searchParams.delete('edit-architecture')
  window.history.replaceState({}, '', url)
  void nextTick(refit)
}

function discardChanges(): void {
  draftData.value = cloneData(savedData.value)
  normalizeSelections()
  saveState.value = 'clean'
  saveError.value = ''
  void nextTick(refit)
}

async function saveDiagram(): Promise<void> {
  if (saveState.value !== 'dirty' && saveState.value !== 'error') return
  saveState.value = 'saving'
  saveError.value = ''

  try {
    const response = await fetch(`${import.meta.env.BASE_URL}__patchouli/architecture`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(draftData.value),
    })
    if (!response.ok) {
      const result = await response.json() as { error?: string }
      throw new Error(result.error || copy.value.editor.error)
    }
    savedData.value = cloneData(draftData.value)
    saveState.value = 'saved'
  }
  catch (error) {
    saveState.value = 'error'
    saveError.value = error instanceof Error ? error.message : copy.value.editor.error
  }
}

function refit(): void {
  void fitView({ padding: compact.value ? 0.04 : 0.08, duration: 260 })
}

function syncLayout(width: number): void {
  const nextCompact = width <= 600
  if (autoCompact.value === nextCompact) return
  autoCompact.value = nextCompact
  if (authoring.value) return
  void nextTick(() => {
    if (refitTimer) clearTimeout(refitTimer)
    refitTimer = setTimeout(refit, 100)
  })
}

onMounted(() => {
  if (architectureRoot.value) syncLayout(architectureRoot.value.clientWidth)
  if (isDevelopment && new URL(window.location.href).searchParams.get('edit-architecture') === '1') {
    authoring.value = true
    editorLayout.value = autoCompact.value ? 'narrow' : 'wide'
  }
  resizeObserver = new ResizeObserver(([entry]) => {
    if (entry) syncLayout(entry.contentRect.width)
  })
  if (architectureRoot.value) resizeObserver.observe(architectureRoot.value)
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  if (refitTimer) clearTimeout(refitTimer)
})
</script>

<template>
  <section ref="architectureRoot" class="pa-architecture" :aria-label="copy.ariaLabel">
    <div class="pa-architecture__toolbar">
      <div class="pa-architecture__routes" role="group" :aria-label="copy.routeLabel">
        <button
          v-for="route in routes"
          :key="route"
          type="button"
          :aria-pressed="activeRoute === route"
          @click="selectRoute(route)"
        >
          {{ copy.routes[route] }}
        </button>
      </div>
      <div class="pa-architecture__tools">
        <button class="pa-architecture__fit" type="button" @click="refit">
          {{ copy.fitView }}
        </button>
        <button
          v-if="isDevelopment"
          class="pa-architecture__edit"
          type="button"
          :aria-pressed="authoring"
          @click="setAuthoring(!authoring)"
        >
          {{ authoring ? copy.editor.close : copy.editor.open }}
        </button>
      </div>
    </div>

    <div v-if="authoring" class="pa-editor" :aria-label="copy.editor.title">
      <div class="pa-editor__bar">
        <div class="pa-editor__section" role="group" :aria-label="copy.editor.title">
          <button type="button" :aria-pressed="editorSection === 'node'" @click="editorSection = 'node'">
            {{ copy.editor.nodeSection }}
          </button>
          <button type="button" :aria-pressed="editorSection === 'edge'" @click="editorSection = 'edge'">
            {{ copy.editor.edgeSection }}
          </button>
        </div>
        <div class="pa-editor__layout" role="group" :aria-label="copy.editor.geometry">
          <button type="button" :aria-pressed="editorLayout === 'wide'" @click="setEditorLayout('wide')">
            {{ copy.editor.wide }}
          </button>
          <button type="button" :aria-pressed="editorLayout === 'narrow'" @click="setEditorLayout('narrow')">
            {{ copy.editor.narrow }}
          </button>
          <label class="pa-editor__canvas-height">
            <span>{{ copy.editor.canvasHeight }}</span>
            <input
              type="number"
              min="320"
              max="2000"
              step="10"
              :value="draftData.canvas[editorLayout].height"
              @input="updateCanvasHeight"
            >
          </label>
        </div>
      </div>

      <div v-if="editorSection === 'node'" class="pa-editor__panel pa-editor__panel--node">
        <div class="pa-editor__selector-group">
          <label class="pa-editor__selector">
            <span>{{ copy.editor.node }}</span>
            <select :value="selectedLayoutNodeId" @change="updateSelectedLayoutNode">
              <option v-for="nodeId in layoutNodeIds" :key="nodeId" :value="nodeId">
                {{ draftData.nodeCopy[props.locale][nodeId].label }}
              </option>
            </select>
          </label>
          <div class="pa-editor__entity-actions">
            <button type="button" :disabled="!canAddNode" @click="addNode">
              {{ copy.editor.addNode }}
            </button>
            <button type="button" :disabled="selectedLayoutNodeId === 'boundary'" @click="deleteSelectedNode">
              {{ copy.editor.deleteNode }}
            </button>
          </div>
          <label v-if="selectedNodeDefinition" class="pa-editor__containment">
            <input
              type="checkbox"
              :checked="selectedNodeDefinition.insideBoundary"
              @change="toggleInsideBoundary"
            >
            <span>{{ copy.editor.insideBoundary }}</span>
          </label>
        </div>

        <fieldset class="pa-editor__geometry">
          <legend>{{ copy.editor.geometry }} · {{ editorLayout === 'wide' ? copy.editor.wide : copy.editor.narrow }}</legend>
          <label>
            <span>{{ copy.editor.x }}</span>
            <input type="number" step="1" :value="selectedPlacement.position.x" @input="updatePlacement('x', $event)">
          </label>
          <label>
            <span>{{ copy.editor.y }}</span>
            <input type="number" step="1" :value="selectedPlacement.position.y" @input="updatePlacement('y', $event)">
          </label>
          <label>
            <span>{{ copy.editor.width }}</span>
            <input type="number" min="80" step="1" :value="selectedPlacement.width" @input="updatePlacement('width', $event)">
          </label>
          <label>
            <span>{{ copy.editor.height }}</span>
            <input type="number" min="48" step="1" :value="selectedPlacement.height" @input="updatePlacement('height', $event)">
          </label>
        </fieldset>

        <div class="pa-editor__copy">
          <fieldset v-for="locale in locales" :key="locale" class="pa-editor__language">
            <legend>{{ locale === 'en' ? copy.editor.english : copy.editor.chinese }}</legend>
            <label>
              <span>{{ copy.editor.label }}</span>
              <input type="text" :value="selectedNodeCopy[locale].label" @input="updateNodeCopy(locale, 'label', $event)">
            </label>
            <label v-if="selectedLayoutNodeId !== 'boundary'">
              <span>{{ copy.editor.role }}</span>
              <input type="text" :value="selectedNodeCopy[locale].role" @input="updateNodeCopy(locale, 'role', $event)">
            </label>
            <label v-if="selectedLayoutNodeId !== 'boundary'">
              <span>{{ copy.editor.description }}</span>
              <textarea :value="selectedNodeCopy[locale].description" rows="3" @input="updateNodeCopy(locale, 'description', $event)"></textarea>
            </label>
          </fieldset>
        </div>
      </div>

      <div v-else class="pa-editor__panel pa-editor__panel--edge">
        <div class="pa-editor__selector-group">
          <label class="pa-editor__edge">
            <span>{{ copy.editor.edge }}</span>
            <select v-model="selectedEdgeId" :disabled="edgeSpecs.length === 0">
              <option v-for="edge in edgeSpecs" :key="edge.id" :value="edge.id">
                {{ copy.nodes[edge.source].label }} → {{ copy.nodes[edge.target].label }}
              </option>
            </select>
          </label>
          <div class="pa-editor__entity-actions">
            <button type="button" :disabled="!canAddEdge" @click="addEdge">
              {{ copy.editor.addEdge }}
            </button>
            <button type="button" :disabled="!selectedEdgeSpec" @click="deleteSelectedEdge">
              {{ copy.editor.deleteEdge }}
            </button>
          </div>
        </div>

        <template v-if="selectedEdgeSpec">
          <label>
            <span>{{ copy.editor.englishLabel }}</span>
            <input
              type="text"
              :value="draftData.edgeLabels.en[selectedEdgeSpec.id]"
              @input="updateEdgeLabel('en', $event)"
            >
          </label>

          <label>
            <span>{{ copy.editor.chineseLabel }}</span>
            <input
              type="text"
              :value="draftData.edgeLabels.zh[selectedEdgeSpec.id]"
              @input="updateEdgeLabel('zh', $event)"
            >
          </label>

          <fieldset class="pa-editor__edge-topology">
            <legend>{{ copy.editor.topology }} · {{ editorLayout === 'wide' ? copy.editor.wide : copy.editor.narrow }}</legend>
            <label>
              <span>{{ copy.editor.source }}</span>
              <select :value="selectedEdgeSpec.source" @change="updateEdgeEndpoint('source', $event)">
                <option v-for="nodeId in nodeIds" :key="nodeId" :value="nodeId">
                  {{ copy.nodes[nodeId].label }}
                </option>
              </select>
            </label>
            <label>
              <span>{{ copy.editor.sourceHandle }}</span>
              <select :value="selectedEdgeSpec.handles[editorLayout][0]" @change="updateEdgeHandle('source', $event)">
                <option v-for="handle in sourceHandles" :key="handle" :value="handle">
                  {{ handleLabel(handle) }}
                </option>
              </select>
            </label>
            <label>
              <span>{{ copy.editor.target }}</span>
              <select :value="selectedEdgeSpec.target" @change="updateEdgeEndpoint('target', $event)">
                <option v-for="nodeId in nodeIds" :key="nodeId" :value="nodeId">
                  {{ copy.nodes[nodeId].label }}
                </option>
              </select>
            </label>
            <label>
              <span>{{ copy.editor.targetHandle }}</span>
              <select :value="selectedEdgeSpec.handles[editorLayout][1]" @change="updateEdgeHandle('target', $event)">
                <option v-for="handle in targetHandles" :key="handle" :value="handle">
                  {{ handleLabel(handle) }}
                </option>
              </select>
            </label>
            <button type="button" @click="reverseSelectedEdge">
              {{ copy.editor.reverseEdge }}
            </button>
          </fieldset>

          <fieldset class="pa-editor__edge-style">
            <legend>{{ copy.editor.edgeSection }}</legend>
            <label>
              <span>{{ copy.editor.edgeType }}</span>
              <select :value="draftData.edgeStyles[selectedEdgeSpec.id].type" @change="updateEdgeStyle('type', $event)">
                <option value="smoothstep">{{ copy.editor.smoothStep }}</option>
                <option value="step">{{ copy.editor.step }}</option>
                <option value="straight">{{ copy.editor.straight }}</option>
                <option value="default">{{ copy.editor.bezier }}</option>
              </select>
            </label>
            <label>
              <span>{{ copy.editor.marker }}</span>
              <select :value="draftData.edgeStyles[selectedEdgeSpec.id].marker" @change="updateEdgeStyle('marker', $event)">
                <option value="none">{{ copy.editor.markerNone }}</option>
                <option value="arrow">{{ copy.editor.markerArrow }}</option>
                <option value="closed">{{ copy.editor.markerClosed }}</option>
              </select>
            </label>
            <label>
              <span>{{ copy.editor.markerSize }}</span>
              <input
                type="number"
                min="8"
                max="40"
                step="1"
                :disabled="draftData.edgeStyles[selectedEdgeSpec.id].marker === 'none'"
                :value="draftData.edgeStyles[selectedEdgeSpec.id].markerSize"
                @input="updateEdgeStyle('markerSize', $event)"
              >
            </label>
            <label>
              <span>{{ copy.editor.color }}</span>
              <span class="pa-editor__color">
                <input
                  type="color"
                  :value="draftData.edgeStyles[selectedEdgeSpec.id].color"
                  @input="updateEdgeStyle('color', $event)"
                >
                <output>{{ draftData.edgeStyles[selectedEdgeSpec.id].color }}</output>
              </span>
            </label>
          </fieldset>

          <fieldset class="pa-editor__paths">
            <legend>{{ copy.editor.paths }}</legend>
            <label v-for="route in editableRoutes" :key="route">
              <input
                type="checkbox"
                :checked="draftData.edgeRoutes[selectedEdgeSpec.id].includes(route)"
                @change="toggleEdgeRoute(route, $event)"
              >
              <span>{{ copy.routes[route] }}</span>
            </label>
          </fieldset>
        </template>
        <p v-else class="pa-editor__empty">{{ copy.editor.noEdges }}</p>
      </div>

      <div class="pa-editor__actions">
        <span class="pa-editor__status" :class="`is-${saveState}`" role="status">{{ saveStatus }}</span>
        <button
          type="button"
          :disabled="saveState === 'clean' || saveState === 'saved' || saveState === 'saving'"
          @click="discardChanges"
        >
          {{ copy.editor.reset }}
        </button>
        <button
          class="pa-editor__save"
          type="button"
          :disabled="saveState === 'clean' || saveState === 'saved' || saveState === 'saving'"
          @click="saveDiagram"
        >
          {{ saveState === 'saving' ? copy.editor.saving : copy.editor.save }}
        </button>
      </div>
    </div>

    <p class="pa-architecture__instruction">{{ copy.instruction }}</p>

    <div
      class="pa-architecture__canvas"
      :style="{ height: `${draftData.canvas[compact ? 'narrow' : 'wide'].height}px` }"
    >
      <VueFlow
        :key="`${compact ? 'compact' : 'wide'}-${authoring ? 'edit' : 'read'}`"
        :id="flowId"
        :nodes="nodes"
        :edges="edges"
        :nodes-draggable="authoring"
        :nodes-connectable="false"
        :elements-selectable="authoring"
        :zoom-on-scroll="false"
        :zoom-on-double-click="false"
        :pan-on-scroll="false"
        :prevent-scrolling="false"
        :min-zoom="0.55"
        :max-zoom="1.35"
        fit-view-on-init
        class="pa-architecture__flow"
        @edge-click="handleEdgeClick"
        @node-click="handleNodeClick"
        @node-drag-stop="handleNodeDragStop"
        @nodes-initialized="refit"
      >
        <template #node-boundary="{ data }">
          <div class="pa-boundary">
            <span>{{ data.label }}</span>
          </div>
        </template>

        <template #node-module="{ data }">
          <Handle id="out-left-top" type="source" :position="Position.Left" :connectable="false" :style="{ top: '36%' }" class="pa-handle" />
          <Handle id="in-left-bottom" type="target" :position="Position.Left" :connectable="false" :style="{ top: '64%' }" class="pa-handle" />
          <Handle id="in-right-top" type="target" :position="Position.Right" :connectable="false" :style="{ top: '36%' }" class="pa-handle" />
          <Handle id="out-right-bottom" type="source" :position="Position.Right" :connectable="false" :style="{ top: '64%' }" class="pa-handle" />
          <Handle id="out-top-left" type="source" :position="Position.Top" :connectable="false" :style="{ left: '36%' }" class="pa-handle" />
          <Handle id="in-top-right" type="target" :position="Position.Top" :connectable="false" :style="{ left: '64%' }" class="pa-handle" />
          <Handle id="in-bottom-left" type="target" :position="Position.Bottom" :connectable="false" :style="{ left: '36%' }" class="pa-handle" />
          <Handle id="out-bottom-right" type="source" :position="Position.Bottom" :connectable="false" :style="{ left: '64%' }" class="pa-handle" />
          <button
            type="button"
            class="pa-module"
            :class="`pa-module--${data.kind}`"
            :aria-pressed="data.selected"
          >
            <span class="pa-module__role">{{ data.role }}</span>
            <strong>{{ data.label }}</strong>
          </button>
        </template>
      </VueFlow>
    </div>

    <div v-if="selectedNode" class="pa-architecture__detail" aria-live="polite">
      <span>{{ selectedNode.role }}</span>
      <strong>{{ selectedNode.label }}</strong>
      <p>{{ selectedNode.description }}</p>
    </div>
  </section>
</template>

<style scoped>
.pa-architecture {
  --pa-edge: #8551a5;
  --pa-edge-muted: #aaa2ad;
  width: min(860px, calc(100vw - 48px));
  margin: 28px 0 40px 50%;
  color: var(--vp-c-text-1);
  transform: translateX(-50%);
}

.pa-architecture:has(.pa-editor) {
  width: 100%;
  margin-left: 0;
  transform: none;
}

.pa-architecture__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 10px;
}

.pa-architecture__routes {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.pa-architecture__tools {
  display: flex;
  flex: 0 0 auto;
  gap: 6px;
}

.pa-architecture button {
  font: inherit;
}

.pa-architecture__routes button,
.pa-architecture__fit,
.pa-architecture__edit,
.pa-editor button {
  min-height: 34px;
  padding: 6px 11px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  color: var(--vp-c-text-2);
  background: var(--vp-c-bg);
  cursor: pointer;
  white-space: nowrap;
  transition: border-color 160ms ease-out, color 160ms ease-out, background-color 160ms ease-out;
}

.pa-architecture__routes button:hover,
.pa-architecture__fit:hover,
.pa-architecture__edit:hover,
.pa-editor button:hover:not(:disabled) {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}

.pa-architecture__routes button[aria-pressed='true'],
.pa-architecture__edit[aria-pressed='true'],
.pa-editor__layout button[aria-pressed='true'],
.pa-editor__section button[aria-pressed='true'] {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
}

.pa-architecture__routes button:focus-visible,
.pa-architecture__fit:focus-visible,
.pa-architecture__edit:focus-visible,
.pa-editor button:focus-visible,
.pa-editor input:focus-visible,
.pa-editor select:focus-visible,
.pa-editor textarea:focus-visible,
.pa-module:focus-visible {
  outline: 3px solid var(--vp-c-brand-soft);
  outline-offset: 2px;
}

.pa-editor {
  display: grid;
  gap: 12px;
  margin: 14px 0 12px;
  padding: 14px 0;
  border-block: 1px solid var(--vp-c-divider);
}

.pa-editor__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.pa-editor__section,
.pa-editor__layout {
  display: flex;
  gap: 4px;
}

.pa-editor__layout {
  align-items: flex-end;
}

.pa-editor__canvas-height {
  width: 112px;
  margin-left: 6px;
}

.pa-editor__section button {
  min-width: 78px;
}

.pa-editor__panel {
  display: grid;
  gap: 12px;
}

.pa-editor__panel--node {
  grid-template-columns: minmax(180px, 0.8fr) minmax(420px, 2fr);
}

.pa-editor__panel--edge {
  grid-template-columns: minmax(220px, 1.3fr) minmax(160px, 1fr) minmax(160px, 1fr);
}

.pa-editor__selector-group {
  display: grid;
  align-content: start;
  gap: 8px;
}

.pa-editor__entity-actions {
  display: flex;
  gap: 6px;
}

.pa-editor__entity-actions button {
  flex: 1;
}

.pa-editor label.pa-editor__containment {
  display: flex;
  align-items: center;
  gap: 7px;
}

.pa-editor__containment input {
  width: 15px;
  height: 15px;
  accent-color: var(--vp-c-brand-1);
}

.pa-editor label {
  display: grid;
  gap: 5px;
  min-width: 0;
}

.pa-editor label > span {
  color: var(--vp-c-text-2);
  font-size: 11px;
  font-weight: 650;
}

.pa-editor input:not([type='checkbox']),
.pa-editor select,
.pa-editor textarea {
  width: 100%;
  padding: 5px 9px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 8px;
  color: var(--vp-c-text-1);
  font: inherit;
  font-size: 12px;
  background: var(--vp-c-bg);
}

.pa-editor input:not([type='checkbox']),
.pa-editor select {
  height: 34px;
}

.pa-editor textarea {
  min-height: 72px;
  line-height: 1.45;
  resize: vertical;
}

.pa-editor fieldset {
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.pa-editor legend {
  margin-bottom: 7px;
  color: var(--vp-c-text-2);
  font-size: 11px;
  font-weight: 700;
}

.pa-editor__geometry {
  display: grid;
  grid-template-columns: repeat(4, minmax(76px, 1fr));
  gap: 8px;
}

.pa-editor__geometry legend {
  grid-column: 1 / -1;
}

.pa-editor__edge-topology {
  display: grid;
  grid-column: 1 / -1;
  grid-template-columns: repeat(4, minmax(110px, 1fr));
  gap: 8px;
  padding-top: 10px !important;
  border-top: 1px solid var(--vp-c-divider) !important;
}

.pa-editor__edge-topology legend,
.pa-editor__edge-topology button {
  grid-column: 1 / -1;
}

.pa-editor__edge-topology button {
  justify-self: start;
}

.pa-editor__empty {
  grid-column: 1 / -1;
  margin: 0;
  color: var(--vp-c-text-2);
  font-size: 13px;
}

.pa-editor__copy {
  display: grid;
  grid-column: 1 / -1;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.pa-editor__language {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  padding-top: 10px !important;
  border-top: 1px solid var(--vp-c-divider) !important;
}

.pa-editor__language legend {
  grid-column: 1 / -1;
}

.pa-editor__language label:last-child {
  grid-column: 1 / -1;
}

.pa-editor__paths {
  display: flex;
  grid-column: 1 / -1;
  flex-wrap: wrap;
  gap: 8px 18px;
  padding-top: 10px !important;
  border-top: 1px solid var(--vp-c-divider) !important;
}

.pa-editor__edge-style {
  display: grid;
  grid-column: 1 / -1;
  grid-template-columns: repeat(4, minmax(110px, 1fr));
  gap: 8px;
  padding-top: 10px !important;
  border-top: 1px solid var(--vp-c-divider) !important;
}

.pa-editor__edge-style legend {
  grid-column: 1 / -1;
}

.pa-editor__color {
  display: flex;
  align-items: center;
  height: 34px;
  gap: 9px;
}

.pa-editor__color input[type='color'] {
  width: 42px;
  height: 34px;
  padding: 3px;
  cursor: pointer;
}

.pa-editor__color output {
  color: var(--vp-c-text-2);
  font-family: var(--vp-font-family-mono);
  font-size: 11px;
}

.pa-editor__paths legend {
  width: 100%;
}

.pa-editor__paths label {
  display: flex;
  grid-template-columns: none;
  align-items: center;
  gap: 7px;
}

.pa-editor__paths input {
  width: 15px;
  height: 15px;
  accent-color: var(--vp-c-brand-1);
}

.pa-editor button:disabled {
  cursor: not-allowed;
  opacity: 0.46;
}

.pa-editor__actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
}

.pa-editor__status {
  margin-right: auto;
  color: var(--vp-c-text-2);
  font-size: 12px;
}

.pa-editor__status.is-dirty,
.pa-editor__status.is-error {
  color: var(--vp-c-warning-1);
}

.pa-editor__status.is-saved {
  color: var(--vp-c-success-1);
}

.pa-editor .pa-editor__save {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-bg);
  background: var(--vp-c-brand-1);
}

.pa-editor .pa-editor__save:hover:not(:disabled) {
  color: var(--vp-c-bg);
  background: var(--vp-c-brand-2);
}

.pa-architecture__instruction {
  margin: 0 0 12px;
  color: var(--vp-c-text-2);
  font-size: 13px;
}

.pa-architecture__canvas {
  overflow: hidden;
  border: 1px solid var(--vp-c-divider);
  border-radius: 14px;
  background-color: var(--vp-c-bg-soft);
  background-image:
    linear-gradient(var(--vp-c-divider) 1px, transparent 1px),
    linear-gradient(90deg, var(--vp-c-divider) 1px, transparent 1px);
  background-size: 24px 24px;
}

.pa-architecture__flow {
  background: color-mix(in srgb, var(--vp-c-bg) 82%, transparent);
}

.pa-boundary {
  width: 100%;
  height: 100%;
  border: 1px dashed color-mix(in srgb, var(--vp-c-brand-1) 62%, var(--vp-c-divider));
  border-radius: 14px;
  background: color-mix(in srgb, var(--vp-c-brand-soft) 36%, transparent);
}

.pa-boundary span {
  position: absolute;
  top: 14px;
  left: 16px;
  color: var(--vp-c-brand-1);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.pa-module {
  position: relative;
  width: 100%;
  height: 100%;
  padding: 14px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  color: var(--vp-c-text-1);
  text-align: left;
  background: var(--vp-c-bg);
  cursor: pointer;
  transition: border-color 160ms ease-out, background-color 160ms ease-out, opacity 160ms ease-out;
}

.pa-module::before {
  position: absolute;
  top: 14px;
  right: 14px;
  width: 8px;
  height: 8px;
  border-radius: 2px;
  background: var(--pa-node-accent, var(--vp-c-brand-1));
  content: '';
}

.pa-module:hover {
  border-color: color-mix(in srgb, var(--pa-node-accent, var(--vp-c-brand-1)) 68%, var(--vp-c-divider));
}

.pa-module[aria-pressed='true'] {
  border-color: var(--pa-node-accent, var(--vp-c-brand-1));
  background: color-mix(in srgb, var(--pa-node-accent, var(--vp-c-brand-1)) 9%, var(--vp-c-bg));
}

.pa-module__role {
  display: block;
  margin: 0 20px 7px 0;
  color: var(--vp-c-text-2);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  line-height: 1.2;
  text-transform: uppercase;
}

.pa-module strong {
  display: block;
  font-size: 13px;
  line-height: 1.35;
}

.pa-module--agent,
.pa-module--other {
  --pa-node-accent: #4b7d68;
}

.pa-module--connector {
  --pa-node-accent: #4c75a3;
}

.pa-module--coordinator {
  --pa-node-accent: #8551a5;
}

.pa-module--plugins {
  --pa-node-accent: #c08b18;
}

.pa-module--backend {
  --pa-node-accent: #a5573e;
}

.pa-handle {
  width: 6px;
  height: 6px;
  border: 0;
  background: var(--pa-edge);
  opacity: 0;
  pointer-events: none;
}

:deep(.vue-flow__node-module) {
  border: 0;
  background: transparent;
  transition: opacity 180ms ease-out;
}

.pa-architecture:has(.pa-editor) :deep(.vue-flow__node-module) {
  cursor: grab;
}

.pa-architecture:has(.pa-editor) :deep(.vue-flow__node-module.dragging) {
  cursor: grabbing;
}

:deep(.vue-flow__node-module.is-muted) {
  opacity: 0.28;
}

:deep(.vue-flow__node-boundary) {
  border: 0;
  background: transparent;
  pointer-events: none;
}

:deep(.pa-flow-edge .vue-flow__edge-path) {
  stroke: var(--pa-edge);
  stroke-width: 1.7;
  transition: opacity 180ms ease-out;
}

:deep(.pa-flow-edge.is-muted .vue-flow__edge-path) {
  stroke: var(--pa-edge-muted);
  opacity: 0.12;
}

:deep(.pa-flow-edge.is-active .vue-flow__edge-path) {
  stroke-width: 2;
}

:deep(.pa-flow-edge.is-selected .vue-flow__edge-path) {
  stroke-width: 3;
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

:deep(.pa-flow-edge.is-muted .vue-flow__edge-text),
:deep(.pa-flow-edge.is-muted .vue-flow__edge-textbg) {
  opacity: 0.18;
}

:deep(.vue-flow__attribution) {
  display: none;
}

.pa-architecture__detail {
  display: grid;
  grid-template-columns: 138px minmax(170px, 220px) 1fr;
  align-items: baseline;
  gap: 14px;
  min-height: 78px;
  padding: 16px 18px;
  border-bottom: 1px solid var(--vp-c-divider);
}

.pa-architecture__detail > span {
  color: var(--vp-c-brand-1);
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.pa-architecture__detail strong {
  font-size: 14px;
}

.pa-architecture__detail p {
  margin: 0;
  color: var(--vp-c-text-2);
  font-size: 13px;
  line-height: 1.55;
}

@media (max-width: 700px) {
  .pa-architecture {
    width: 100%;
    margin: 24px 0 34px;
    transform: none;
  }

  .pa-architecture__toolbar {
    align-items: flex-start;
  }

  .pa-architecture__routes {
    gap: 5px;
  }

  .pa-architecture__routes button,
  .pa-architecture__fit,
  .pa-architecture__edit,
  .pa-editor button {
    min-height: 32px;
    padding: 5px 9px;
    font-size: 12px;
  }

  .pa-architecture__tools {
    flex-direction: column;
  }

  .pa-editor__bar {
    align-items: flex-start;
    flex-direction: column;
  }

  .pa-editor__layout,
  .pa-editor__entity-actions,
  .pa-editor__actions {
    flex-wrap: wrap;
  }

  .pa-editor__canvas-height {
    margin-left: 0;
  }

  .pa-editor__panel--node,
  .pa-editor__panel--edge,
  .pa-editor__copy,
  .pa-editor__language {
    grid-template-columns: 1fr;
  }

  .pa-editor__geometry {
    grid-template-columns: 1fr 1fr;
  }

  .pa-editor__edge-topology,
  .pa-editor__edge-style {
    grid-template-columns: 1fr 1fr;
  }

  .pa-editor__status {
    flex: 1 0 100%;
  }

  .pa-editor__language legend {
    grid-column: 1;
  }

  .pa-module {
    padding: 12px;
  }

  .pa-module strong {
    font-size: 12px;
  }

  .pa-architecture__detail {
    grid-template-columns: 1fr;
    gap: 5px;
    min-height: 0;
    padding: 14px 2px 16px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .pa-architecture *,
  :deep(.vue-flow__edge-path) {
    transition-duration: 0s !important;
    animation: none !important;
  }
}
</style>
