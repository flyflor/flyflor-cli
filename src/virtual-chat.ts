import {
  createTextAttributes,
  type MouseEvent,
  type OptimizedBuffer,
  parseColor,
  Renderable,
  type RenderableOptions,
  type RenderContext,
  type KeyEvent,
} from "@opentui/core"
import stringWidth from "string-width"
import { FenwickTree } from "./fenwick.ts"
import { renderMarkdown, type MarkdownSpan } from "./markdown.ts"
import type { ChatBlock } from "./mock-data.ts"
import { colors } from "./theme.ts"
import { padToWidth, wrapSpans } from "./text-layout.ts"

type RenderRow = {
  spans: RenderSpan[]
}

type RenderSpan = {
  text: string
  color: ReturnType<typeof parseColor>
  bg?: ReturnType<typeof parseColor>
  attributes?: number
}

type MeasureEntry = {
  width: number
  rows: RenderRow[]
}

export interface VirtualChatOptions extends RenderableOptions<VirtualChatRenderable> {
  blocks: ChatBlock[]
  stickToBottom?: boolean
}

export class VirtualChatRenderable extends Renderable {
  private static readonly paddingLeft = 2
  private static readonly scrollbarGutter = 2
  private blocks: ChatBlock[]
  private cache = new Map<number, MeasureEntry>()
  private heights: number[]
  private tree: FenwickTree
  private scrollTop = 0
  private lastContentWidth = 0
  private readonly overscan = 4
  private stickToBottom: boolean
  private forceBottom = false
  private drag:
    | {
        startY: number
        startScrollTop: number
        trackTop: number
        trackHeight: number
        thumbHeight: number
      }
    | null = null

  constructor(ctx: RenderContext, options: VirtualChatOptions) {
    super(ctx, {
      ...options,
      buffered: true,
    })
    this.focusable = true
    this.blocks = options.blocks
    this.stickToBottom = options.stickToBottom ?? false
    this.heights = blocksToInitialHeights(this.blocks)
    this.tree = new FenwickTree(this.heights)
    this.onMouseScroll = (event) => this.handleMouseScroll(event)
    this.onMouseDown = (event) => this.handleMouseDown(event)
    this.onMouseDrag = (event) => this.handleMouseDrag(event)
    this.onMouseUp = (event) => this.handleMouseUp(event)
  }

  get maxScrollTop(): number {
    return Math.max(0, this.tree.total() - this.contentHeight)
  }

  scrollBy(delta: number): void {
    this.setScrollTop(this.scrollTop + delta)
  }

  scrollToBottom(): void {
    this.stickToBottom = true
    this.forceBottom = true
    this.setScrollTop(this.maxScrollTop)
    this.requestRender()
  }

  appendBlock(block: ChatBlock): void {
    const wasAtBottom = this.isAtBottom()
    this.blocks.push(block)
    const index = this.blocks.length - 1
    const height = this.lastContentWidth > 0 ? this.measureBlockRows(block, this.lastContentWidth).length : block.lines.length + 1
    this.heights.push(height)
    this.tree.push(height)
    this.cache.set(index, { width: this.lastContentWidth, rows: this.measureBlockRows(block, this.lastContentWidth || this.contentWidth) })
    if (this.stickToBottom || wasAtBottom) {
      this.scrollTop = this.maxScrollTop
      this.stickToBottom = true
    }
    this.requestRender()
  }

  handleKeyPress(key: KeyEvent): boolean {
    const name = key.name
    if (key.ctrl && name === "e") {
      this.scrollToBottom()
      return true
    }
    if (name === "up") {
      this.scrollBy(-3)
      return true
    }
    if (name === "down") {
      this.scrollBy(3)
      return true
    }
    if (name === "pageup") {
      this.scrollBy(-Math.max(1, this.contentHeight - 2))
      return true
    }
    if (name === "pagedown" || key.sequence === " ") {
      this.scrollBy(Math.max(1, this.contentHeight - 2))
      return true
    }
    if (name === "home") {
      this.setScrollTop(0)
      return true
    }
    if (name === "end") {
      this.scrollToBottom()
      return true
    }
    return false
  }

  protected override onResize(width: number, height: number): void {
    super.onResize(width, height)
    const previousContentWidth = this.lastContentWidth
    if (previousContentWidth > 0 && this.contentWidth !== previousContentWidth) {
      const anchor = this.findAnchor()
      this.cache.clear()
      this.remeasureAll(this.contentWidth)
      this.scrollTop = this.stickToBottom ? this.maxScrollTop : Math.min(this.maxScrollTop, this.tree.prefix(anchor.blockIndex) + anchor.offset)
    }
    this.lastContentWidth = this.contentWidth
  }

  protected override renderSelf(buffer: OptimizedBuffer): void {
    if (!this.visible || this.width <= 0 || this.height <= 0) return
    const x = this.x
    const y = this.y
    const width = this.width
    const height = this.height

    buffer.fillRect(x, y, width, height, colors.bg)
    if (this.contentWidth <= 0 || this.contentHeight <= 0) return

    if (this.contentWidth !== this.lastContentWidth) {
      this.remeasureAll(this.contentWidth)
      this.lastContentWidth = this.contentWidth
    }
    if (this.stickToBottom || this.forceBottom) {
      this.scrollTop = this.maxScrollTop
      this.stickToBottom = true
      this.forceBottom = false
    }

    const start = Math.max(0, this.scrollTop - this.overscan)
    const startBlock = this.tree.lowerBound(start)
    let rowCursor = this.tree.prefix(startBlock)
    let screenY = this.contentY + rowCursor - this.scrollTop

    for (let i = startBlock; i < this.blocks.length && screenY < this.contentY + this.contentHeight + this.overscan; i++) {
      const measured = this.measureBlock(i, this.contentWidth)
      for (let local = 0; local < measured.rows.length; local++) {
        const currentY = screenY + local
        if (currentY >= this.contentY && currentY < this.contentY + this.contentHeight) {
          const row = measured.rows[local]
          this.drawRow(buffer, row, this.contentX, currentY)
        }
      }
      rowCursor += measured.rows.length
      screenY = this.contentY + rowCursor - this.scrollTop
    }

    this.renderScrollbar(buffer)
  }

  private get contentX(): number {
    return this.x + VirtualChatRenderable.paddingLeft
  }

  private get contentY(): number {
    return this.y
  }

  private get contentWidth(): number {
    return Math.max(1, this.width - VirtualChatRenderable.paddingLeft - VirtualChatRenderable.scrollbarGutter)
  }

  private get contentHeight(): number {
    return Math.max(1, this.height)
  }

  private setScrollTop(value: number): void {
    const next = Math.max(0, Math.min(this.maxScrollTop, Math.round(value)))
    if (next === this.scrollTop) return
    this.scrollTop = next
    this.stickToBottom = this.isAtBottom()
    this.requestRender()
  }

  private handleMouseScroll(event: MouseEvent): void {
    const direction = event.scroll?.direction
    const magnitude = Math.max(1, event.scroll?.delta ?? 1)
    if (direction === "up") this.scrollBy(-magnitude * 3)
    if (direction === "down") this.scrollBy(magnitude * 3)
    event.stopPropagation()
  }

  private handleMouseDown(event: MouseEvent): void {
    if (!this.isScrollbarX(event.x)) return
    const metrics = this.scrollbarMetrics()
    if (!metrics) return

    const relative = event.y - metrics.thumbTop
    if (relative >= 0 && relative < metrics.thumbHeight) {
      this.drag = {
        startY: event.y,
        startScrollTop: this.scrollTop,
        trackTop: metrics.trackTop,
        trackHeight: metrics.trackHeight,
        thumbHeight: metrics.thumbHeight,
      }
      this.stickToBottom = false
    } else {
      const targetRatio = (event.y - metrics.trackTop - Math.floor(metrics.thumbHeight / 2)) / metrics.travel
      this.setScrollTop(targetRatio * this.maxScrollTop)
      this.drag = {
        startY: event.y,
        startScrollTop: this.scrollTop,
        trackTop: metrics.trackTop,
        trackHeight: metrics.trackHeight,
        thumbHeight: metrics.thumbHeight,
      }
      this.stickToBottom = false
    }
    event.stopPropagation()
  }

  private handleMouseDrag(event: MouseEvent): void {
    if (!this.drag) return
    const travel = Math.max(1, this.drag.trackHeight - this.drag.thumbHeight)
    const deltaY = event.y - this.drag.startY
    this.stickToBottom = false
    this.setScrollTop(this.drag.startScrollTop + (deltaY / travel) * this.maxScrollTop)
    event.stopPropagation()
  }

  private handleMouseUp(event: MouseEvent): void {
    if (!this.drag) return
    this.drag = null
    event.stopPropagation()
  }

  private isAtBottom(): boolean {
    return this.scrollTop >= this.maxScrollTop - 1
  }

  private findAnchor(): { blockIndex: number; offset: number } {
    const blockIndex = this.tree.lowerBound(this.scrollTop)
    const offset = this.scrollTop - this.tree.prefix(blockIndex)
    return { blockIndex, offset }
  }

  private remeasureAll(width: number): void {
    const heights = this.blocks.map((_, index) => this.measureBlock(index, width).rows.length)
    this.heights = heights
    this.tree = new FenwickTree(heights)
    this.scrollTop = Math.min(this.scrollTop, this.maxScrollTop)
  }

  private measureBlock(index: number, width: number): MeasureEntry {
    const cached = this.cache.get(index)
    if (cached?.width === width) return cached

    const rows = this.measureBlockRows(this.blocks[index], width)

    const entry = { width, rows }
    const previousHeight = this.heights[index]
    if (previousHeight !== undefined && previousHeight !== rows.length) {
      this.tree.add(index, rows.length - previousHeight)
      this.heights[index] = rows.length
    }
    this.cache.set(index, entry)
    return entry
  }

  private measureBlockRows(block: ChatBlock, width: number): RenderRow[] {
    const rows: RenderRow[] = []
    const accent = block.role === "user" ? colors.pink : block.role === "system" ? colors.pink : colors.blue
    const bodyColor = colors.text

    block.lines.forEach((line, lineIndex) => {
      const prefix = lineIndex === 0 ? (block.role === "user" ? "> " : "") : "  "
      const rendered = renderMarkdown(line, {
        text: lineIndex === 0 ? accent : bodyColor,
        accent,
        code: colors.yellow,
        quote: colors.muted,
        dim: colors.faint,
      })
      const spans: RenderSpan[] = [
        { text: prefix, color: lineIndex === 0 ? accent : bodyColor },
        ...rendered.map((span) => markdownSpanToRenderSpan(span, lineIndex === 0 ? accent : bodyColor)),
      ]
      const wrapped = wrapSpans(spans, Math.max(1, width), [{ text: lineIndex === 0 ? "  " : "  ", color: colors.faint }])
      for (const row of wrapped) rows.push({ spans: row.spans })
    })
    return rows
  }

  private drawRow(buffer: OptimizedBuffer, row: RenderRow, x: number, y: number): void {
    let cursor = x
    let used = 0
    for (const span of row.spans) {
      if (used >= this.contentWidth) break
      const text = trimToRemaining(span.text, this.contentWidth - used)
      if (!text) continue
      buffer.drawText(text, cursor, y, span.color, span.bg ?? colors.bg, span.attributes)
      const width = stringWidth(text)
      cursor += width
      used += width
    }
    if (used < this.contentWidth) {
      buffer.drawText(" ".repeat(this.contentWidth - used), cursor, y, colors.text, colors.bg)
    }
  }

  private renderScrollbar(buffer: OptimizedBuffer): void {
    const barX = this.x + this.width - 1
    const gutterX = this.x + this.width - 2
    const metrics = this.scrollbarMetrics()
    const ascii = shouldUseAsciiScrollbar(this.ctx)
    const trackChar = ascii ? "|" : "·"
    const thumbChar = ascii ? "#" : "█"
    const trackColor = ascii ? colors.muted : colors.dimBorder
    const thumbColor = ascii ? colors.text : colors.muted
    if (!metrics) {
      for (let row = 0; row < this.contentHeight; row++) {
        buffer.drawText(" ", gutterX, this.contentY + row, colors.faint, colors.bg)
        buffer.drawText(trackChar, barX, this.contentY + row, trackColor, colors.bg)
      }
      return
    }

    for (let row = 0; row < metrics.trackHeight; row++) {
      const y = metrics.trackTop + row
      const inThumb = y >= metrics.thumbTop && y < metrics.thumbTop + metrics.thumbHeight
      buffer.drawText(" ", gutterX, y, colors.faint, colors.bg)
      buffer.drawText(inThumb ? thumbChar : trackChar, barX, y, inThumb ? thumbColor : trackColor, colors.bg, inThumb ? createTextAttributes({ bold: true }) : undefined)
    }
  }

  private scrollbarMetrics():
    | {
        trackTop: number
        trackHeight: number
        thumbTop: number
        thumbHeight: number
        travel: number
      }
    | null {
    const total = this.tree.total()
    const viewport = this.contentHeight
    const trackHeight = this.contentHeight
    if (total <= viewport || trackHeight <= 0) return null

    const thumbHeight = Math.max(1, Math.floor((viewport / total) * trackHeight))
    const travel = Math.max(1, trackHeight - thumbHeight)
    const thumbTop = this.contentY + Math.round((this.scrollTop / this.maxScrollTop) * travel)
    return { trackTop: this.contentY, trackHeight, thumbTop, thumbHeight, travel }
  }

  private isScrollbarX(x: number): boolean {
    return x === this.x + this.width - 1 || x === this.x + this.width - 2
  }
}

function blocksToInitialHeights(blocks: ChatBlock[]): number[] {
  return blocks.map((block) => block.lines.length)
}

function markdownSpanToRenderSpan(span: MarkdownSpan, fallback: ReturnType<typeof parseColor>): RenderSpan {
  return {
    text: span.text,
    color: span.color ?? fallback,
    bg: span.bg,
    attributes: span.attributes,
  }
}

function trimToRemaining(text: string, maxWidth: number): string {
  let out = ""
  let width = 0
  for (const char of text) {
    const next = stringWidth(char)
    if (width + next > maxWidth) break
    out += char
    width += next
  }
  return out
}

function shouldUseAsciiScrollbar(ctx: RenderContext): boolean {
  if (process.env.FLYFLOR_ASCII_SCROLLBAR === "1") return true
  if (process.env.NO_COLOR || process.env.TERM === "dumb") return true
  const name = ctx.capabilities?.terminal?.name?.toLowerCase() || process.env.TERM_PROGRAM?.toLowerCase() || process.env.TERM?.toLowerCase() || ""
  return name.includes("linux") || name.includes("consolehost")
}
