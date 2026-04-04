import { HeadContent, Outlet, Scripts, createRootRoute } from '@tanstack/react-router'
import { RootProvider } from 'fumadocs-ui/provider/tanstack'
import * as React from 'react'

import '../styles/app.css'

export const Route = createRootRoute({
  component: RootComponent,
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1.0' },
      { title: 'pr-tools Docs — Documentação e Guias' },
      {
        name: 'description',
        content:
          'Documentação completa do pr-tools: CLI para gerar descrições de PR e cards de teste no Azure DevOps com IA.'
      },
      { property: 'og:type', content: 'website' },
      {
        property: 'og:title',
        content: 'pr-tools Docs — Documentação e Guias'
      },
      {
        property: 'og:description',
        content:
          'Documentação completa do pr-tools: CLI para gerar descrições de PR e cards de teste no Azure DevOps com IA.'
      },
      { property: 'og:image', content: '/api/og' },
      { property: 'og:image:width', content: '1200' },
      { property: 'og:image:height', content: '630' },
      { property: 'og:site_name', content: 'pr-tools' },
      { property: 'og:locale', content: 'pt_BR' },
      { name: 'twitter:card', content: 'summary_large_image' },
      {
        name: 'twitter:title',
        content: 'pr-tools Docs — Documentação e Guias'
      },
      {
        name: 'twitter:description',
        content:
          'Documentação completa do pr-tools: CLI para gerar descrições de PR e cards de teste no Azure DevOps com IA.'
      },
      { name: 'twitter:image', content: '/api/og' },
      { name: 'author', content: 'pr-tools' },
      { name: 'robots', content: 'index, follow' },
      { name: 'theme-color', content: '#0F172A' }
    ],
    links: [
      { rel: 'icon', type: 'image/svg+xml', href: '/favicon.svg' },
      { rel: 'icon', type: 'image/x-icon', href: '/favicon.ico' }
    ]
  })
})

function RootComponent() {
  return (
    <RootDocument>
      <Outlet />
    </RootDocument>
  )
}

function RootDocument({ children }: { children: React.ReactNode }) {
  return (
    <html lang="pt-BR" suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body className="flex flex-col min-h-screen">
        <RootProvider>{children}</RootProvider>
        <Scripts />
      </body>
    </html>
  )
}
