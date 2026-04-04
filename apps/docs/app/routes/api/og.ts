import { createFileRoute } from '@tanstack/react-router'
import { ImageResponse } from '@vercel/og'

export const Route = createFileRoute('/api/og')({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const url = new URL(request.url)
        const title = url.searchParams.get('title') ?? 'pr-tools Docs'
        const description =
          url.searchParams.get('description') ??
          'Documentação completa do pr-tools'

        const html = {
          type: 'div',
          props: {
            style: {
              display: 'flex',
              flexDirection: 'column',
              width: '100%',
              height: '100%',
              backgroundColor: '#0F172A',
              padding: '60px',
              fontFamily: 'sans-serif',
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
                    {
                      type: 'div',
                      props: {
                        style: {
                          fontSize: '24px',
                          color: '#475569',
                          marginLeft: '8px',
                        },
                        children: '/ docs',
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
                          fontSize: '68px',
                          fontWeight: 800,
                          color: '#F8FAFC',
                          lineHeight: 1.1,
                          marginBottom: '24px',
                        },
                        children: title,
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
                        children: description,
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
                  children: 'docs.pr-tools.dev',
                },
              },
            ],
          },
        }

        return new ImageResponse(html, {
          width: 1200,
          height: 630,
        }) as unknown as Response
      },
    },
  },
})
