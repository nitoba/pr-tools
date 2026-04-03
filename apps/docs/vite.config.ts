import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import mdx from 'fumadocs-mdx/vite';
import * as MdxConfig from './source.config';

export default defineConfig({
  plugins: [mdx(MdxConfig), react()],
});
