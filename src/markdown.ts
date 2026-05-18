import { createTextAttributes, type RGBA } from "@opentui/core"
import { colors } from "./theme.ts"

export type MarkdownSpan = {
  text: string
  color?: RGBA
  bg?: RGBA
  attributes?: number
}

type Palette = {
  text: RGBA
  accent: RGBA
  code: RGBA
  quote: RGBA
  dim: RGBA
}

export function renderMarkdown(input: string, palette: Palette): MarkdownSpan[] {
  const line = input.replace(/\t/g, "  ")

  if (/^#{1,6}\s+/.test(line)) {
    return [
      {
        text: line.replace(/^#{1,6}\s+/, ""),
        color: palette.accent,
        attributes: createTextAttributes({ bold: true }),
      },
    ]
  }

  if (/^>\s?/.test(line)) {
    return [
      { text: "| ", color: palette.quote },
      ...renderInline(line.replace(/^>\s?/, ""), palette, palette.quote),
    ]
  }

  if (/^```/.test(line)) {
    return [{ text: line, color: palette.code, attributes: createTextAttributes({ bold: true }) }]
  }

  const unordered = line.match(/^(\s*)[-*+]\s+(.*)$/)
  if (unordered) {
    return [
      { text: `${unordered[1]}- `, color: palette.accent },
      ...renderInline(unordered[2], palette, palette.text),
    ]
  }

  const ordered = line.match(/^(\s*)(\d+)\.\s+(.*)$/)
  if (ordered) {
    return [
      { text: `${ordered[1]}${ordered[2]}. `, color: palette.accent },
      ...renderInline(ordered[3], palette, palette.text),
    ]
  }

  return renderInline(line, palette, palette.text)
}

function renderInline(text: string, palette: Palette, baseColor: RGBA): MarkdownSpan[] {
  const spans: MarkdownSpan[] = []
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|\*[^*]+\*|_[^_]+_)/g
  let cursor = 0
  let match: RegExpExecArray | null

  while ((match = pattern.exec(text))) {
    if (match.index > cursor) spans.push({ text: text.slice(cursor, match.index), color: baseColor })
    const token = match[0]
    if (token.startsWith("`")) {
      spans.push({
        text: token.slice(1, -1),
        color: palette.code,
        bg: colors.panel2,
        attributes: createTextAttributes({ bold: true }),
      })
    } else if (token.startsWith("**") || token.startsWith("__")) {
      spans.push({
        text: token.slice(2, -2),
        color: baseColor,
        attributes: createTextAttributes({ bold: true }),
      })
    } else {
      spans.push({
        text: token.slice(1, -1),
        color: baseColor,
        attributes: createTextAttributes({ italic: true }),
      })
    }
    cursor = match.index + token.length
  }

  if (cursor < text.length) spans.push({ text: text.slice(cursor), color: baseColor })
  return spans
}
