import { defineConfig } from 'vite';
import { tanstackStart } from '@tanstack/react-start/plugin/vite';
import tsConfigPaths from 'vite-tsconfig-paths';
import mdx from 'fumadocs-mdx/vite';
import viteReact from '@vitejs/plugin-react';
import * as MdxConfig from './source.config';

export default defineConfig({
  plugins: [
    tsConfigPaths({ projects: ['./tsconfig.json'] }),
    tanstackStart({ srcDirectory: 'app' }),
    mdx(MdxConfig),
    viteReact(),
  ],
});
