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

  const [fontBold, fontRegular] = await Promise.all([
    fetch(
      'https://cdn.jsdelivr.net/fontsource/fonts/geist-sans@latest/latin-700-normal.woff'
    ).then((r) => r.arrayBuffer()),
    fetch(
      'https://cdn.jsdelivr.net/fontsource/fonts/geist-sans@latest/latin-400-normal.woff'
    ).then((r) => r.arrayBuffer()),
  ])

  const svg = await satori(
    {
      type: 'div',
      props: {
        style: {
          display: 'flex',
          flexDirection: 'column',
          width: '100%',
          height: '100%',
          backgroundColor: '#111318',
          padding: '60px 70px',
          fontFamily: 'Geist Sans',
          position: 'relative',
          overflow: 'hidden',
        },
        children: [
          // Subtle gradient accent in top-right corner
          {
            type: 'div',
            props: {
              style: {
                position: 'absolute',
                top: '-120px',
                right: '-120px',
                width: '400px',
                height: '400px',
                borderRadius: '50%',
                background: 'radial-gradient(circle, rgba(52,211,153,0.08) 0%, transparent 70%)',
              },
            },
          },
          // Top bar: logo + domain
          {
            type: 'div',
            props: {
              style: {
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                width: '100%',
                marginBottom: '48px',
              },
              children: [
                {
                  type: 'div',
                  props: {
                    style: {
                      display: 'flex',
                      alignItems: 'center',
                      gap: '14px',
                    },
                    children: [
                      // Terminal icon
                      {
                        type: 'div',
                        props: {
                          style: {
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            width: '44px',
                            height: '44px',
                            borderRadius: '10px',
                            backgroundColor: '#1a1d24',
                            border: '1px solid #2a2d35',
                            fontSize: '20px',
                            fontWeight: 700,
                            color: '#34d399',
                          },
                          children: '>_',
                        },
                      },
                      {
                        type: 'div',
                        props: {
                          style: {
                            fontSize: '22px',
                            fontWeight: 700,
                            color: '#e5e7eb',
                            letterSpacing: '-0.02em',
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
                      fontSize: '16px',
                      color: '#4b5563',
                      fontWeight: 400,
                    },
                    children: 'pr-tools.dev',
                  },
                },
              ],
            },
          },
          // Main content
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
                      fontSize: '56px',
                      fontWeight: 700,
                      color: '#f3f4f6',
                      lineHeight: 1.1,
                      letterSpacing: '-0.03em',
                      marginBottom: '20px',
                      maxWidth: '900px',
                    },
                    children: 'AI-powered PR tools para devs no Azure DevOps',
                  },
                },
                {
                  type: 'div',
                  props: {
                    style: {
                      fontSize: '24px',
                      color: '#6b7280',
                      lineHeight: 1.5,
                      fontWeight: 400,
                      maxWidth: '700px',
                    },
                    children:
                      'Gere descrições de PR e cards de teste automaticamente. Suporta OpenRouter, Groq, Gemini e Ollama.',
                  },
                },
              ],
            },
          },
          // Bottom bar: accent line
          {
            type: 'div',
            props: {
              style: {
                display: 'flex',
                alignItems: 'center',
                gap: '12px',
              },
              children: [
                {
                  type: 'div',
                  props: {
                    style: {
                      width: '32px',
                      height: '3px',
                      backgroundColor: '#34d399',
                      borderRadius: '2px',
                    },
                  },
                },
                {
                  type: 'div',
                  props: {
                    style: {
                      fontSize: '14px',
                      color: '#4b5563',
                      fontWeight: 400,
                      letterSpacing: '0.05em',
                      textTransform: 'uppercase' as const,
                    },
                    children: 'CLI  ·  Open Source  ·  Multi-provider',
                  },
                },
              ],
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
          data: fontBold,
          weight: 700,
          style: 'normal' as const,
        },
        {
          name: 'Geist Sans',
          data: fontRegular,
          weight: 400,
          style: 'normal' as const,
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
