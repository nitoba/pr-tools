#!/usr/bin/env node
/**
 * generate-newsletter-email.mjs
 *
 * Converts a Markdown newsletter file into a production-ready HTML email.
 *
 * Usage:
 *   node generate-newsletter-email.mjs <tag> <input.md> <output.html>
 *
 * Color science:
 *   - HUE_BRAND = 292  (purple — pr-tools accent)
 *   - HUE_BASE  = 220  (neutral blue-gray for surfaces and text)
 *   - HUE_CODE  = 270  (slightly warmer purple for code blocks)
 *   - Light bg ~95% lightness → text ~15% lightness → ratio >7:1 (WCAG AAA)
 *   - Dark bg ~8% lightness → text ~84% lightness → ratio >7:1 (WCAG AAA)
 *   - Borders at ~45-55% lightness: visible accent without dominating
 */

import { readFileSync, writeFileSync } from 'node:fs'

// ─── Args ────────────────────────────────────────────────────────────────────

const [, , tag, inputPath, outputPath] = process.argv

if (!tag || !inputPath || !outputPath) {
  console.error('Usage: generate-newsletter-email.mjs <tag> <input.md> <output.html>')
  process.exit(1)
}

const markdown = readFileSync(inputPath, 'utf-8')

// ─── HSL System ──────────────────────────────────────────────────────────────

// Hue distribution — single-brand palette, surfaces use HUE_BASE
const HUE_BRAND = 292 // violet-purple: pr-tools primary
const HUE_BASE = 220 // blue-gray: neutral surfaces, text
const HUE_CODE = 270 // muted purple: code blocks

/** @param {number} h @param {number} s @param {number} l */
const hsl = (h, s, l) => `hsl(${h},${s}%,${l}%)`

/**
 * Light theme — used as baseline and for email clients without dark-mode support.
 *
 * Lightness rules:
 *   bg      93–96% — pastel, no glare
 *   cardBg  100%   — max contrast for content area
 *   text    14–16% — >7:1 on cardBg (WCAG AAA)
 *   muted   48–55% — >4.5:1 on cardBg (WCAG AA)
 *   accent  42–48% — >3:1 on cardBg (AA large text), links
 *   border  85–90% — hairline dividers, low weight
 *   codeBg  93%    — tinted surface, code distinct from prose
 */
const _LIGHT = {
  bg: hsl(HUE_BASE, 15, 95),
  cardBg: '#ffffff',
  headerBg: hsl(HUE_BRAND, 55, 18), // dark purple header
  headerText: '#ffffff',
  text: hsl(HUE_BASE, 15, 14),
  textSecondary: hsl(HUE_BASE, 10, 38),
  textMuted: hsl(HUE_BASE, 8, 52),
  accent: hsl(HUE_BRAND, 65, 45),
  accentText: '#ffffff',
  border: hsl(HUE_BASE, 15, 88),
  divider: hsl(HUE_BASE, 12, 90),
  codeBg: hsl(HUE_CODE, 20, 94),
  codeBorder: hsl(HUE_CODE, 20, 85),
  codeText: hsl(HUE_BRAND, 55, 35),
  footerBg: hsl(HUE_BASE, 10, 93),
  footerText: hsl(HUE_BASE, 8, 52),
  tagBg: hsl(HUE_BRAND, 50, 92),
  tagText: hsl(HUE_BRAND, 60, 28)
}

/**
 * Dark theme — activated via @media (prefers-color-scheme: dark).
 *
 * Lightness inversion rules:
 *   bg      7–9%   — near-black surface (#0A0A0A equivalent)
 *   cardBg  10–12% — slight elevation (#0F0F0F)
 *   text    83–85% — >7:1 on cardBg (WCAG AAA)
 *   muted   44–48% — >3:1 on dark bg (WCAG AA)
 *   accent  68–72% — high-lightness purple, >3:1 on dark bg
 *   border  14–17% — subtle hairline on dark (#1A1A1A)
 *   codeBg  12–14% — tinted dark surface for code
 */
const DARK = {
  bg: hsl(HUE_BASE, 10, 7),
  cardBg: hsl(HUE_BASE, 10, 10),
  headerBg: hsl(HUE_BRAND, 45, 14),
  headerText: '#ffffff',
  text: hsl(HUE_BASE, 10, 84),
  textSecondary: hsl(HUE_BASE, 8, 63),
  textMuted: hsl(HUE_BASE, 5, 46),
  accent: hsl(HUE_BRAND, 85, 70),
  accentText: hsl(HUE_BASE, 10, 7),
  border: hsl(HUE_BASE, 10, 16),
  divider: hsl(HUE_BASE, 10, 18),
  codeBg: hsl(HUE_CODE, 25, 12),
  codeBorder: hsl(HUE_CODE, 25, 20),
  codeText: hsl(HUE_BRAND, 80, 72),
  footerBg: hsl(HUE_BASE, 10, 8),
  footerText: hsl(HUE_BASE, 5, 46),
  tagBg: hsl(HUE_BRAND, 30, 18),
  tagText: hsl(HUE_BRAND, 65, 72)
}

// ─── Font ────────────────────────────────────────────────────────────────────

const FONT_SANS =
  "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif"
const FONT_MONO = "'Geist Mono', 'Courier New', Courier, monospace"

// ─── Table helpers ───────────────────────────────────────────────────────────

/** Wraps content in a <td> with inline style */
function td(style, content) {
  return `<td style="${style}">${content}</td>`
}

/** Wraps content in a <tr> */
function row(content) {
  return `<tr>${content}</tr>`
}

/** Empty spacer row */
function spacer(height) {
  return row(`<td style="font-size:0;line-height:0;height:${height}px">&nbsp;</td>`)
}

/** Horizontal rule row */
function dividerRow(t) {
  return row(`<td><hr style="border:none;border-top:1px solid ${t.divider};margin:0" /></td>`)
}

// ─── Markdown → HTML (inline, no deps) ──────────────────────────────────────

/**
 * Minimal but correct MD → HTML converter.
 * Processes block elements first, then inline elements within each block.
 * Avoids regex catastrophic backtracking by processing line-by-line.
 *
 * @param {string} md
 * @param {typeof _LIGHT} t — theme tokens for inline styles
 */
function markdownToHtml(md, t) {
  const lines = md.split('\n')
  const out = []
  let inCode = false
  let codeLines = []
  let inList = false
  let listTag = ''

  function flushList() {
    if (!inList) return
    out.push(`</${listTag}>`)
    inList = false
    listTag = ''
  }

  function inlineStyles(text) {
    return (
      text
        // Code spans — process before bold/italic to avoid conflicts
        .replace(
          /`([^`]+)`/g,
          (_, c) =>
            `<code style="font-family:${FONT_MONO};font-size:12px;background:${t.codeBg};color:${t.codeText};padding:2px 6px;border-radius:3px;border:1px solid ${t.codeBorder}">${escHtml(c)}</code>`
        )
        // Bold
        .replace(
          /\*\*(.+?)\*\*/g,
          (_, c) => `<strong style="font-weight:700;color:${t.text}">${c}</strong>`
        )
        // Italic
        .replace(/\*(.+?)\*/g, (_, c) => `<em style="font-style:italic">${c}</em>`)
        // Links — validate href scheme to prevent phishing
        .replace(
          /\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g,
          (_, label, href) =>
            `<a href="${escAttr(href)}" style="color:${t.accent};text-decoration:underline" target="_blank" rel="noopener noreferrer">${label}</a>`
        )
    )
  }

  function escHtml(s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  }

  function escAttr(s) {
    return s.replace(/"/g, '&quot;').replace(/'/g, '&#39;')
  }

  for (const raw of lines) {
    const line = raw

    // ── Fenced code block ──────────────────────────────────────────────────
    if (line.startsWith('```')) {
      if (!inCode) {
        flushList()
        inCode = true
        codeLines = []
        continue
      } else {
        const preStyle = `background:${t.codeBg};border:1px solid ${t.codeBorder};border-radius:6px;padding:14px 16px;margin:8px 0 14px;overflow-x:auto;`
        const codeStyle = `font-family:${FONT_MONO};font-size:12px;line-height:1.6;color:${t.codeText};white-space:pre;`
        out.push(
          `<table width="100%" cellpadding="0" cellspacing="0" role="presentation"><tr><td style="${preStyle}"><pre style="margin:0;padding:0"><code style="${codeStyle}">${escHtml(codeLines.join('\n'))}</code></pre></td></tr></table>`
        )
        inCode = false
        codeLines = []
        continue
      }
    }

    if (inCode) {
      codeLines.push(line)
      continue
    }

    // ── Headings ───────────────────────────────────────────────────────────
    const h3 = line.match(/^### (.+)/)
    const h2 = line.match(/^## (.+)/)
    const h1 = line.match(/^# (.+)/)

    if (h1) {
      flushList()
      const s = `font-family:${FONT_SANS};font-size:20px;font-weight:700;color:${t.text};margin:0 0 10px;padding:0;line-height:1.3`
      out.push(`<h1 style="${s}">${inlineStyles(h1[1])}</h1>`)
      continue
    }

    if (h2) {
      flushList()
      const s = `font-family:${FONT_SANS};font-size:17px;font-weight:700;color:${t.text};margin:22px 0 8px;padding:0 0 6px;line-height:1.3;border-bottom:1px solid ${t.divider}`
      out.push(`<h2 style="${s}">${inlineStyles(h2[1])}</h2>`)
      continue
    }

    if (h3) {
      flushList()
      const s = `font-family:${FONT_SANS};font-size:14px;font-weight:600;color:${t.textSecondary};margin:16px 0 6px;padding:0;line-height:1.4`
      out.push(`<h3 style="${s}">${inlineStyles(h3[1])}</h3>`)
      continue
    }

    // ── Horizontal rule ────────────────────────────────────────────────────
    if (/^---+$/.test(line.trim())) {
      flushList()
      out.push(`<hr style="border:none;border-top:1px solid ${t.divider};margin:16px 0" />`)
      continue
    }

    // ── Unordered list item ────────────────────────────────────────────────
    const ulItem = line.match(/^- (.+)/)
    if (ulItem) {
      if (!inList || listTag !== 'ul') {
        flushList()
        out.push(
          `<ul style="font-family:${FONT_SANS};font-size:14px;line-height:1.6;color:${t.text};margin:6px 0 12px;padding-left:20px">`
        )
        inList = true
        listTag = 'ul'
      }
      out.push(`<li style="margin:0 0 4px;color:${t.text}">${inlineStyles(ulItem[1])}</li>`)
      continue
    }

    // ── Ordered list item ──────────────────────────────────────────────────
    const olItem = line.match(/^(\d+)\. (.+)/)
    if (olItem) {
      if (!inList || listTag !== 'ol') {
        flushList()
        out.push(
          `<ol style="font-family:${FONT_SANS};font-size:14px;line-height:1.6;color:${t.text};margin:6px 0 12px;padding-left:20px">`
        )
        inList = true
        listTag = 'ol'
      }
      out.push(`<li style="margin:0 0 4px;color:${t.text}">${inlineStyles(olItem[2])}</li>`)
      continue
    }

    // ── Blank line ─────────────────────────────────────────────────────────
    if (line.trim() === '') {
      flushList()
      continue
    }

    // ── Paragraph ──────────────────────────────────────────────────────────
    flushList()
    const pStyle = `font-family:${FONT_SANS};font-size:14px;line-height:1.7;color:${t.text};margin:0 0 12px;padding:0`
    out.push(`<p style="${pStyle}">${inlineStyles(escHtml(line).replace(/\\n/g, '<br />'))}</p>`)
  }

  flushList()
  return out.join('\n')
}

// ─── Section renderers ───────────────────────────────────────────────────────

function renderHeader(tag, t) {
  const titleStyle = `font-family:${FONT_SANS};font-size:22px;font-weight:700;color:${t.headerText};margin:0 0 4px;padding:0;letter-spacing:-0.5px`
  const subtitleStyle = `font-family:${FONT_MONO};font-size:12px;color:${t.headerText};opacity:.75;margin:0;padding:0;letter-spacing:.5px`

  return row(
    td(
      `background:${t.headerBg};padding:28px 32px;border-radius:8px 8px 0 0`,
      `<h1 style="${titleStyle}">pr-tools</h1>` +
        `<p style="${subtitleStyle}">${tag} · Release Notes</p>`
    )
  )
}

function renderBody(contentHtml, t) {
  const wrapStyle = `background:${t.cardBg};padding:28px 32px`
  return row(td(wrapStyle, contentHtml))
}

function renderCta(t) {
  const buttonStyle = `display:inline-block;background:${t.accent};color:${t.accentText};text-decoration:none;padding:11px 28px;border-radius:9999px;font-family:${FONT_SANS};font-size:14px;font-weight:600;letter-spacing:.2px;line-height:1`

  return (
    spacer(8) +
    dividerRow(t) +
    spacer(20) +
    row(
      td(
        'text-align:center',
        `<a href="https://docs-pr-tools.nitodev.com.br" style="${buttonStyle}">Ver documentação &rarr;</a>`
      )
    ) +
    spacer(24)
  )
}

function renderFooter(t) {
  const wrapStyle = `background:${t.footerBg};padding:18px 32px;border-radius:0 0 8px 8px;border-top:1px solid ${t.divider}`
  const textStyle = `font-family:${FONT_MONO};font-size:11px;color:${t.footerText};margin:0 0 4px;padding:0;text-align:center`
  const linkStyle = `color:${t.accent};text-decoration:underline`

  return row(
    td(
      wrapStyle,
      `<p style="${textStyle}">pr-tools &middot; ` +
        `<a href="https://github.com/nitoba/pr-tools" style="${linkStyle}">GitHub</a> &middot; ` +
        `<a href="https://docs-pr-tools.nitodev.com.br" style="${linkStyle}">Docs</a></p>` +
        `<p style="${textStyle}">` +
        `<a href="{{unsubscribe_url}}" style="${linkStyle}">Cancelar inscri&ccedil;&atilde;o</a></p>`
    )
  )
}

// ─── Dark mode media query override ─────────────────────────────────────────

function darkModeStyles() {
  // Most modern email clients (Apple Mail, Gmail app iOS/Android, Outlook iOS)
  // honour @media (prefers-color-scheme: dark) via <style> in <head>.
  const D = DARK
  return `
<style type="text/css">
  @media (prefers-color-scheme: dark) {
    .email-wrapper  { background-color: ${D.bg} !important; }
    .email-card     { background-color: ${D.cardBg} !important; }
    .email-header   { background-color: ${D.headerBg} !important; }
    .email-footer   { background-color: ${D.footerBg} !important; border-top-color: ${D.divider} !important; }
    .email-divider  { border-top-color: ${D.divider} !important; }
    .email-body td  { background-color: ${D.cardBg} !important; }
    h1, h2, h3, h4, .email-text      { color: ${D.text} !important; }
    .email-muted, .email-footer p     { color: ${D.footerText} !important; }
    .email-accent, a                  { color: ${D.accent} !important; }
    .email-cta-btn  { background-color: ${D.accent} !important; color: ${D.accentText} !important; }
    pre, code, .email-code            { background-color: ${D.codeBg} !important; color: ${D.codeText} !important; border-color: ${D.codeBorder} !important; }
  }
</style>`
}

// ─── Full HTML document ──────────────────────────────────────────────────────

function renderEmail(tag, markdown) {
  const t = DARK
  const bodyRows =
    renderHeader(tag, t) +
    spacer(0) +
    renderBody(markdownToHtml(markdown, t), t) +
    renderCta(t) +
    renderFooter(t)

  // Outer table approach: wrapper → centered 600px card → content rows
  return `<!DOCTYPE html>
<html lang="pt-BR" xmlns="http://www.w3.org/1999/xhtml">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta name="color-scheme" content="light dark" />
  <meta name="supported-color-schemes" content="light dark" />
  <meta http-equiv="X-UA-Compatible" content="IE=edge" />
  <title>pr-tools ${escapeHtmlAttr(tag)}</title>
  ${darkModeStyles()}
</head>
<body style="margin:0;padding:0;background:${t.bg}" class="email-wrapper">
  <!-- Preview text (hidden) -->
  <div style="display:none;font-size:1px;line-height:1px;max-height:0;max-width:0;opacity:0;overflow:hidden">
    pr-tools ${escapeHtmlAttr(tag)} — veja o que mudou nesta versão.
  </div>

  <!-- Outer wrapper -->
  <table width="100%" cellpadding="0" cellspacing="0" role="presentation"
         style="background:${t.bg}" class="email-wrapper">
    <tr>
      <td align="center" style="padding:40px 20px">

        <!-- Content card — max 600px -->
        <table width="600" cellpadding="0" cellspacing="0" role="presentation"
               style="max-width:600px;width:100%;border-radius:8px;overflow:hidden;
                      border:1px solid ${t.border}" class="email-card">
          ${bodyRows}
        </table>

      </td>
    </tr>
  </table>
</body>
</html>`
}

function escapeHtmlAttr(s) {
  return s
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
}

// ─── Run ─────────────────────────────────────────────────────────────────────

const html = renderEmail(tag, markdown)
writeFileSync(outputPath, html, 'utf-8')
console.log(`✓ Email HTML generated: ${outputPath} (${(html.length / 1024).toFixed(1)} KB)`)
