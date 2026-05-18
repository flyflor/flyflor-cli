import stringWidth from "string-width"

export type WrappedLine = {
  text: string
  width: number
}

export type TextSpan = {
  text: string
  [key: string]: unknown
}

export type WrappedSpanLine<T extends TextSpan> = {
  spans: T[]
  width: number
}

function charWidth(char: string): number {
  return Math.max(0, stringWidth(char))
}

function sliceToWidth(text: string, maxWidth: number): string {
  if (maxWidth <= 0) return ""
  let width = 0
  let out = ""
  for (const char of text) {
    const next = charWidth(char)
    if (width + next > maxWidth) break
    width += next
    out += char
  }
  return out
}

export function padToWidth(text: string, width: number): string {
  const clipped = sliceToWidth(text, width)
  const remaining = Math.max(0, width - stringWidth(clipped))
  return clipped + " ".repeat(remaining)
}

export function wrapText(text: string, width: number, indent = ""): WrappedLine[] {
  const targetWidth = Math.max(1, width)
  const hardLines = text.split("\n")
  const rows: WrappedLine[] = []

  for (const raw of hardLines) {
    if (raw.length === 0) {
      rows.push({ text: "", width: 0 })
      continue
    }

    let line = ""
    let lineWidth = 0
    let prefix = ""

    for (const char of raw) {
      const cw = charWidth(char)
      if (cw > targetWidth) continue
      if (lineWidth + cw > targetWidth) {
        rows.push({ text: line, width: lineWidth })
        prefix = indent
        line = prefix
        lineWidth = stringWidth(prefix)
      }
      line += char
      lineWidth += cw
    }

    rows.push({ text: line, width: lineWidth })
  }

  return rows
}

export function wrapSpans<T extends TextSpan>(spans: T[], width: number, indent: T[] = []): WrappedSpanLine<T>[] {
  const targetWidth = Math.max(1, width)
  const rows: WrappedSpanLine<T>[] = []
  let current: T[] = []
  let currentWidth = 0

  const pushRow = () => {
    rows.push({ spans: current, width: currentWidth })
    current = indent.map((span) => ({ ...span }))
    currentWidth = current.reduce((sum, span) => sum + stringWidth(span.text), 0)
  }

  for (const span of spans) {
    const hardLines = span.text.split("\n")
    for (let partIndex = 0; partIndex < hardLines.length; partIndex++) {
      if (partIndex > 0) pushRow()
      const part = hardLines[partIndex]
      for (const char of part) {
        const cw = charWidth(char)
        if (cw > targetWidth) continue
        if (currentWidth + cw > targetWidth) pushRow()
        const last = current[current.length - 1]
        if (last && canMergeSpan(last, span)) {
          last.text += char
        } else {
          current.push({ ...span, text: char })
        }
        currentWidth += cw
      }
    }
  }

  rows.push({ spans: current, width: currentWidth })
  return rows.length > 0 ? rows : [{ spans: [], width: 0 }]
}

function canMergeSpan<T extends TextSpan>(a: T, b: T): boolean {
  return a.color === b.color && a.bg === b.bg && a.attributes === b.attributes
}
