import type { APIRoute } from 'astro'
import satori from 'satori'
import { Resvg, initWasm } from '@resvg/resvg-wasm'
// @ts-expect-error wasm import
import resvgWasm from '@resvg/resvg-wasm/index_bg.wasm'

let wasmInitialized = false

export const GET: APIRoute = async () => {
  if (!wasmInitialized) {
    await initWasm(resvgWasm)
    wasmInitialized = true
  }

  const fontData = await fetch(
    'https://cdn.jsdelivr.net/fontsource/fonts/geist-sans@latest/latin-700-normal.woff'
  ).then((r) => r.arrayBuffer())

  const svg = await satori(
    {
      type: 'div',
      props: {
        style: {
          display: 'flex',
          flexDirection: 'column',
          width: '100%',
          height: '100%',
          backgroundColor: '#0F172A',
          padding: '60px',
          fontFamily: 'Geist Sans',
        },
        children: [
          {
            type: 'div',
            props: {
              style: {
                display: 'flex',
                alignItems: 'center',
                gap: '16px',
                marginBottom: '40px',
              },
              children: [
                {
                  type: 'div',
                  props: {
                    style: {
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      width: '56px',
                      height: '56px',
                      borderRadius: '12px',
                      backgroundColor: '#1E293B',
                      fontSize: '28px',
                      color: '#38BDF8',
                    },
                    children: '>_',
                  },
                },
                {
                  type: 'div',
                  props: {
                    style: {
                      fontSize: '32px',
                      fontWeight: 700,
                      color: '#38BDF8',
                    },
                    children: 'pr-tools',
                  },
                },
              ],
            },
          },
          {
            type: 'div',
            props: {
              style: {
                display: 'flex',
                flexDirection: 'column',
                flex: 1,
                justifyContent: 'center',
              },
              children: [
                {
                  type: 'div',
                  props: {
                    style: {
                      fontSize: '72px',
                      fontWeight: 700,
                      color: '#F8FAFC',
                      lineHeight: 1.1,
                      marginBottom: '24px',
                    },
                    children: 'AI-powered PR tools para devs no Azure DevOps',
                  },
                },
                {
                  type: 'div',
                  props: {
                    style: {
                      fontSize: '32px',
                      color: '#94A3B8',
                      lineHeight: 1.4,
                    },
                    children:
                      'Gere descrições de PR e cards de teste automaticamente. Suporta OpenRouter, Groq, Gemini e Ollama.',
                  },
                },
              ],
            },
          },
          {
            type: 'div',
            props: {
              style: {
                display: 'flex',
                justifyContent: 'flex-end',
                fontSize: '24px',
                color: '#475569',
              },
              children: 'pr-tools.dev',
            },
          },
        ],
      },
    },
    {
      width: 1200,
      height: 630,
      fonts: [
        {
          name: 'Geist Sans',
          data: fontData,
          weight: 700,
          style: 'normal',
        },
      ],
    }
  )

  const resvg = new Resvg(svg)
  const pngData = resvg.render()
  const pngBuffer = pngData.asPng()

  return new Response(pngBuffer, {
    headers: {
      'Content-Type': 'image/png',
      'Cache-Control': 'public, max-age=86400, s-maxage=86400',
    },
  })
}
