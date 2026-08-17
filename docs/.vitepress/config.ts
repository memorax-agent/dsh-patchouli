import { rename, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitepress'

const architectureDataFile = fileURLToPath(new URL('../components/patchouli-architecture.data.json', import.meta.url))
const architectureRoutes = ['retrieve', 'update', 'subscribe', 'artifact']
const architectureEdgeTypes = ['smoothstep', 'step', 'straight', 'default']
const architectureMarkers = ['none', 'arrow', 'closed']
const architectureSourceHandles = ['out-left-top', 'out-right-bottom', 'out-top-left', 'out-bottom-right']
const architectureTargetHandles = ['in-left-bottom', 'in-right-top', 'in-top-right', 'in-bottom-left']
const architectureIdPattern = /^[a-z][a-z0-9-]{0,63}$/

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isArchitectureData(value: unknown): boolean {
  if (!isRecord(value) || !isRecord(value.layouts) || !isRecord(value.edgeLabels)) return false
  if (!isRecord(value.nodeCopy) || !isRecord(value.edgeRoutes) || !isRecord(value.edgeStyles)) return false
  if (!isRecord(value.canvas) || !Array.isArray(value.nodes) || !Array.isArray(value.edges)) return false
  if (value.nodes.length > 64 || value.edges.length > 256) return false

  const nodeIds: string[] = []
  for (const node of value.nodes) {
    if (!isRecord(node) || typeof node.id !== 'string' || !architectureIdPattern.test(node.id)) return false
    if (node.id === 'boundary' || typeof node.insideBoundary !== 'boolean') return false
    if (nodeIds.includes(node.id)) return false
    nodeIds.push(node.id)
  }

  const edgeIds: string[] = []
  for (const edge of value.edges) {
    if (!isRecord(edge) || typeof edge.id !== 'string' || !architectureIdPattern.test(edge.id)) return false
    if (edgeIds.includes(edge.id) || typeof edge.source !== 'string' || typeof edge.target !== 'string') return false
    if (!nodeIds.includes(edge.source) || !nodeIds.includes(edge.target) || !isRecord(edge.handles)) return false
    for (const layoutName of ['wide', 'narrow']) {
      const handles = edge.handles[layoutName]
      if (!Array.isArray(handles) || handles.length !== 2) return false
      if (!architectureSourceHandles.includes(handles[0]) || !architectureTargetHandles.includes(handles[1])) return false
    }
    edgeIds.push(edge.id)
  }

  for (const layoutName of ['wide', 'narrow']) {
    const canvas = value.canvas[layoutName]
    const layout = value.layouts[layoutName]
    if (!isRecord(canvas) || typeof canvas.height !== 'number' || !Number.isFinite(canvas.height)) return false
    if (canvas.height < 320 || canvas.height > 2000 || !isRecord(layout)) return false
    if (Object.keys(layout).length !== nodeIds.length + 1) return false
    for (const nodeId of ['boundary', ...nodeIds]) {
      const node = layout[nodeId]
      if (!isRecord(node) || !isRecord(node.position)) return false
      if (![node.position.x, node.position.y, node.width, node.height].every((entry) => typeof entry === 'number' && Number.isFinite(entry))) return false
      if ((node.width as number) < 80 || (node.height as number) < 48) return false
    }
  }

  for (const locale of ['en', 'zh']) {
    const labels = value.edgeLabels[locale]
    const nodeCopy = value.nodeCopy[locale]
    if (!isRecord(labels) || !isRecord(nodeCopy)) return false
    if (Object.keys(labels).length !== edgeIds.length || !edgeIds.every((edgeId) => typeof labels[edgeId] === 'string')) return false
    if (Object.keys(nodeCopy).length !== nodeIds.length + 1) return false
    for (const nodeId of ['boundary', ...nodeIds]) {
      const copy = nodeCopy[nodeId]
      if (!isRecord(copy) || !['label', 'role', 'description'].every((field) => typeof copy[field] === 'string')) return false
    }
  }

  if (Object.keys(value.edgeRoutes).length !== edgeIds.length || Object.keys(value.edgeStyles).length !== edgeIds.length) return false
  for (const edgeId of edgeIds) {
    const routes = value.edgeRoutes[edgeId]
    const style = value.edgeStyles[edgeId]
    if (!Array.isArray(routes) || !routes.every((route) => architectureRoutes.includes(route))) return false
    if (new Set(routes).size !== routes.length) return false
    if (!isRecord(style) || !architectureEdgeTypes.includes(style.type as string)) return false
    if (!architectureMarkers.includes(style.marker as string)) return false
    if (typeof style.markerSize !== 'number' || !Number.isFinite(style.markerSize)) return false
    if (style.markerSize < 8 || style.markerSize > 40) return false
    if (typeof style.color !== 'string' || !/^#[0-9a-f]{6}$/i.test(style.color)) return false
  }

  return true
}

export default defineConfig({
  base: '/dsh-patchouli/',
  cleanUrls: true,
  lastUpdated: true,
  sitemap: {
    hostname: 'https://memorax-ai.github.io/dsh-patchouli/',
  },
  head: [
    ['meta', { name: 'theme-color', content: '#75439a' }],
    ['meta', { name: 'color-scheme', content: 'light dark' }],
  ],
  markdown: {
    lineNumbers: true,
  },
  vite: {
    publicDir: '../assets',
    plugins: [
      {
        name: 'patchouli-architecture-editor',
        apply: 'serve',
        configureServer(server) {
          server.middlewares.use(async (request, response, next) => {
            if (request.method !== 'POST' || !request.url?.endsWith('/__patchouli/architecture')) {
              next()
              return
            }

            const contentType = request.headers['content-type']
            const origin = request.headers.origin
            if (!contentType?.startsWith('application/json')) {
              response.statusCode = 415
              response.end('Expected application/json')
              return
            }
            if (origin && new URL(origin).host !== request.headers.host) {
              response.statusCode = 403
              response.end('Cross-origin writes are not allowed')
              return
            }

            try {
              let body = ''
              for await (const chunk of request) {
                body += chunk
                if (body.length > 100_000) throw new Error('Architecture data exceeds 100 KB')
              }
              const data: unknown = JSON.parse(body)
              if (!isArchitectureData(data)) throw new Error('Invalid architecture data')

              const temporaryFile = `${architectureDataFile}.tmp`
              await writeFile(temporaryFile, `${JSON.stringify(data, null, 2)}\n`, 'utf8')
              await rename(temporaryFile, architectureDataFile)
              response.statusCode = 204
              response.end()
            }
            catch (error) {
              response.statusCode = 400
              response.setHeader('Content-Type', 'application/json')
              response.end(JSON.stringify({ error: error instanceof Error ? error.message : 'Unable to save architecture data' }))
            }
          })
        },
      },
    ],
  },
  locales: {
    root: {
      label: 'English',
      lang: 'en-US',
      title: 'Patchouli',
      description: 'Knowledge and memory middleware for AI harnesses',
      themeConfig: {
        nav: [
          { text: 'Guide', link: '/getting-started' },
          { text: 'Architecture', link: '/architecture' },
          { text: 'Data model', link: '/knowledge-model' },
          {
            text: 'Backend',
            items: [
              { text: 'Installation', link: '/installation' },
              { text: 'Configuration', link: '/backend-configuration' },
            ],
          },
        ],
        sidebar: [
          {
            text: 'Start here',
            items: [
              { text: 'Introduction', link: '/' },
              { text: 'Getting started', link: '/getting-started' },
              { text: 'DSH integration', link: '/dsh-integration' },
            ],
          },
          {
            text: 'Concepts',
            items: [
              { text: 'Architecture', link: '/architecture' },
              { text: 'Knowledge model', link: '/knowledge-model' },
            ],
          },
          {
            text: 'Backend',
            items: [
              { text: 'Installation', link: '/installation' },
              { text: 'Configuration', link: '/backend-configuration' },
            ],
          },
          {
            text: 'Contributing',
            items: [{ text: 'Development', link: '/development' }],
          },
        ],
        search: { provider: 'local' },
        socialLinks: [
          { icon: 'github', link: 'https://github.com/memorax-ai/dsh-patchouli' },
        ],
        editLink: {
          pattern: 'https://github.com/memorax-ai/dsh-patchouli/edit/docs/docs/:path',
          text: 'Edit this page on GitHub',
        },
        outline: {
          level: [2, 3],
          label: 'On this page',
        },
        footer: {
          message: 'Released under the MIT License. Minecraft-derived icon assets have separate notices.',
          copyright: 'Copyright © 2026 MemoraX Agent contributors',
        },
      },
    },
    zh: {
      label: '简体中文',
      lang: 'zh-CN',
      link: '/zh/',
      title: 'Patchouli',
      description: '面向 AI Harness 的知识与记忆中台',
      themeConfig: {
        nav: [
          { text: '指南', link: '/zh/getting-started' },
          { text: '架构', link: '/zh/architecture' },
          { text: '数据模型', link: '/zh/knowledge-model' },
          {
            text: '后端',
            items: [
              { text: '安装', link: '/zh/installation' },
              { text: '配置', link: '/zh/backend-configuration' },
            ],
          },
        ],
        sidebar: [
          {
            text: '开始使用',
            items: [
              { text: '简介', link: '/zh/' },
              { text: '快速开始', link: '/zh/getting-started' },
              { text: 'DSH 集成', link: '/zh/dsh-integration' },
            ],
          },
          {
            text: '核心概念',
            items: [
              { text: '架构', link: '/zh/architecture' },
              { text: '知识模型', link: '/zh/knowledge-model' },
            ],
          },
          {
            text: '后端',
            items: [
              { text: '安装', link: '/zh/installation' },
              { text: '配置', link: '/zh/backend-configuration' },
            ],
          },
          {
            text: '参与贡献',
            items: [{ text: '开发指南', link: '/zh/development' }],
          },
        ],
        search: {
          provider: 'local',
          options: {
            translations: {
              button: {
                buttonText: '搜索文档',
                buttonAriaLabel: '搜索文档',
              },
              modal: {
                displayDetails: '显示详细列表',
                resetButtonTitle: '清除查询',
                backButtonTitle: '关闭搜索',
                noResultsText: '没有找到相关结果',
                footer: {
                  selectText: '选择',
                  navigateText: '切换',
                  closeText: '关闭',
                },
              },
            },
          },
        },
        socialLinks: [
          { icon: 'github', link: 'https://github.com/memorax-ai/dsh-patchouli' },
        ],
        editLink: {
          pattern: 'https://github.com/memorax-ai/dsh-patchouli/edit/docs/docs/:path',
          text: '在 GitHub 上编辑此页',
        },
        outline: {
          level: [2, 3],
          label: '本页目录',
        },
        lastUpdated: { text: '最后更新' },
        docFooter: { prev: '上一页', next: '下一页' },
        darkModeSwitchLabel: '外观',
        lightModeSwitchTitle: '切换到浅色主题',
        darkModeSwitchTitle: '切换到深色主题',
        sidebarMenuLabel: '菜单',
        returnToTopLabel: '返回顶部',
        langMenuLabel: '切换语言',
        skipToContentLabel: '跳到正文',
        footer: {
          message: '基于 MIT License 发布。源自 Minecraft 的图标素材另有声明。',
          copyright: 'Copyright © 2026 MemoraX Agent contributors',
        },
      },
    },
  },
})
