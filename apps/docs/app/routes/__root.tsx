import { HeadContent, Outlet, Scripts, createRootRoute } from '@tanstack/react-router'
import { RootProvider } from 'fumadocs-ui/provider/tanstack'
import * as React from 'react'

import '../styles/app.css'

export const Route = createRootRoute({
  component: RootComponent
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
    <html suppressHydrationWarning>
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
