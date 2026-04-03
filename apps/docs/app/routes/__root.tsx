import {
  HeadContent,
  Outlet,
  Scripts,
  createRootRoute,
} from '@tanstack/react-router';
import { RootProvider } from 'fumadocs-ui/provider/tanstack';
import 'fumadocs-ui/style.css';

export const Route = createRootRoute({
  component: RootComponent,
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { title: 'pr-tools docs' },
    ],
  }),
});

function RootComponent() {
  return (
    <html suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body className="flex flex-col min-h-screen">
        <RootProvider>
          <Outlet />
        </RootProvider>
        <Scripts />
      </body>
    </html>
  );
}
