import type { APIRoute } from 'astro'
import { Resend } from 'resend'

const resend = new Resend(import.meta.env.RESEND_API_KEY)
const audienceId = import.meta.env.RESEND_AUDIENCE_ID

export const POST: APIRoute = async ({ request }) => {
  let email: string

  try {
    const body = await request.json()
    email = (body.email ?? '').trim().toLowerCase()
  } catch {
    return json({ error: 'Invalid JSON body' }, 400)
  }

  if (!email || !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
    return json({ error: 'Invalid email address' }, 422)
  }

  if (!import.meta.env.RESEND_API_KEY || !audienceId) {
    return json({ error: 'Service unavailable' }, 503)
  }

  try {
    await resend.contacts.create({
      email,
      audienceId,
      unsubscribed: false,
    })

    return json({ ok: true }, 201)
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : 'Unknown error'

    // Resend returns 409 when contact already exists — treat as success
    if (message.includes('already exists') || message.includes('409')) {
      return json({ ok: true }, 200)
    }

    console.error('[subscribe]', message)
    return json({ error: 'Failed to subscribe. Try again later.' }, 500)
  }
}

function json(body: Record<string, unknown>, status: number): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}
