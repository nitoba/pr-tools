import { postInstall } from 'fumadocs-mdx/vite';

await postInstall({
  configPath: 'source.config.ts',
  index: {
    target: 'bun',
  },
});
