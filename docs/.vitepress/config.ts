import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Patchouli',
  description: 'Knowledge and memory middleware for AI harnesses',
  lang: 'en-US',
  base: '/dsh-patchouli/',
  cleanUrls: true,
  lastUpdated: true,
  sitemap: {
    hostname: 'https://memorax-agent.github.io/dsh-patchouli/',
  },
  head: [
    ['meta', { name: 'theme-color', content: '#6d3f8c' }],
    ['meta', { name: 'color-scheme', content: 'light dark' }],
  ],
  markdown: {
    lineNumbers: true,
  },
  vite: {
    publicDir: '../assets',
  },
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
    search: {
      provider: 'local',
    },
    socialLinks: [
      { icon: 'github', link: 'https://github.com/memorax-agent/dsh-patchouli' },
    ],
    editLink: {
      pattern: 'https://github.com/memorax-agent/dsh-patchouli/edit/docs/docs/:path',
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
})
