import { createFileRoute, notFound } from '@tanstack/react-router';
import { createServerFn } from '@tanstack/react-start';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import {
  DocsPage,
  DocsBody,
  DocsTitle,
  DocsDescription,
} from 'fumadocs-ui/layouts/docs/page';
import { source } from '../../lib/source';

export const Route = createFileRoute('/docs/$')({
  component: DocsPageComponent,
  loader: async ({ params }) => {
    const slugs = params._splat?.split('/').filter(Boolean) ?? [];
    return loadPage({ data: slugs });
  },
});

const loadPage = createServerFn({ method: 'GET' })
  .inputValidator((slugs: string[]) => slugs)
  .handler(async ({ data: slugs }) => {
    const page = source.getPage(slugs);
    if (!page) throw notFound();

    return {
      tree: source.getPageTree() as object,
      toc: page.data.toc,
      title: page.data.title,
      description: page.data.description ?? '',
      body: page.data.body,
    };
  });

function DocsPageComponent() {
  const { tree, toc, title, description, body: Body } = Route.useLoaderData();

  return (
    <DocsLayout tree={tree as any} nav={{ title: 'pr-tools' }}>
      <DocsPage toc={toc}>
        <DocsTitle>{title}</DocsTitle>
        <DocsDescription>{description}</DocsDescription>
        <DocsBody>
          <Body />
        </DocsBody>
      </DocsPage>
    </DocsLayout>
  );
}
