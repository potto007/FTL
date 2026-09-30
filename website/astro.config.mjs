import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';
import sitemap from '@astrojs/sitemap';

export default defineConfig({
  site: process.env.FTL_SITE_URL || 'http://localhost:4321',
  integrations: [mdx(), sitemap()],
  trailingSlash: 'ignore',
  redirects: {
    '/docs': '/docs/quickstart',
    '/docs/introduction': '/docs/quickstart',
    '/docs/install': '/docs/quickstart',
    '/docs/first-run': '/docs/quickstart',
  },
  build: {
    format: 'directory',
  },
});
