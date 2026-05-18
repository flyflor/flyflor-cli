import {
  type MouseEvent,
  type KeyEvent,
  type OptimizedBuffer,
  Renderable,
  type RenderableOptions,
  type RenderContext,
} from "@opentui/core"
import { createMockBlocks, type ChatBlock } from "./mock-data.ts"
import { colors } from "./theme.ts"
import { padToWidth } from "./text-layout.ts"
import { VirtualChatRenderable } from "./virtual-chat.ts"

export class FlyflorShellRenderable extends Renderable {
  private readonly chat: VirtualChatRenderable
  private readonly bottomButton: BottomButtonRenderable
  private input = ""
  private exitRequested = false
  private nextBlockId = 10_000
  private turns = 9

  constructor(ctx: RenderContext, options: RenderableOptions<FlyflorShellRenderable> = {}) {
    super(ctx, {
      ...options,
      width: "100%",
      height: "100%",
      buffered: true,
    })
    this.focusable = true

    this.chat = new VirtualChatRenderable(ctx, {
      id: "virtual-chat",
      blocks: createMockBlocks(1600),
      stickToBottom: true,
      position: "absolute",
      left: 1,
      top: 3,
      width: 80,
      height: 20,
    })
    this.bottomButton = new BottomButtonRenderable(ctx, {
      id: "bottom-button",
      position: "absolute",
      left: 120,
      top: 39,
      onClick: () => {
        this.chat.scrollToBottom()
        this.requestRender()
      },
    })
    this.add(this.chat)
    this.add(this.bottomButton)
  }

  handleKeyPress(key: KeyEvent): boolean {
    if (key.ctrl && key.name === "c") {
      this.exit()
      return true
    }
    if (key.ctrl && key.name === "e") {
      this.chat.scrollToBottom()
      this.requestRender()
      return true
    }
    if (key.name === "escape" || this.input === "/exit") {
      this.exit()
      return true
    }
    if (this.chat.handleKeyPress(key)) return true
    if (key.name === "backspace") {
      this.input = this.input.slice(0, -1)
      this.requestRender()
      return true
    }
    if (key.name === "return" || key.name === "linefeed") {
      this.submit()
      this.requestRender()
      return true
    }
    if (!key.ctrl && !key.meta && key.sequence && key.sequence.length === 1 && key.sequence >= " ") {
      this.input += key.sequence
      this.requestRender()
      return true
    }
    return false
  }

  shouldExit(): boolean {
    return this.exitRequested
  }

  private exit(): void {
    this.exitRequested = true
    this.requestRender()
  }

  protected override onResize(width: number, height: number): void {
    super.onResize(width, height)
    this.positionChildren(width, height)
  }

  protected override onUpdate(): void {
    this.positionChildren(this.width, this.height)
  }

  protected override renderSelf(buffer: OptimizedBuffer): void {
    if (this.width <= 0 || this.height <= 0) return
    this.positionChildren(this.width, this.height)
    buffer.fillRect(this.x, this.y, this.width, this.height, colors.bg)
    this.renderTopBar(buffer)
    this.renderRightPanel(buffer)
    this.renderInput(buffer)
  }

  private positionChildren(width: number, height: number): void {
    const layout = this.getLayout(width, height)

    this.chat.left = layout.chatX
    this.chat.top = layout.mainY
    this.chat.width = layout.chatWidth
    this.chat.height = layout.mainHeight
    this.bottomButton.left = Math.max(2, width - 16)
    this.bottomButton.top = Math.max(0, height - 3)
  }

  private getLayout(width = this.width, height = this.height): {
    mainY: number
    mainHeight: number
    chatX: number
    chatWidth: number
    panelX: number
    panelWidth: number
  } {
    const inputHeight = 4
    const topHeight = 1
    const gutter = 1
    const outerX = 0
    const mainY = topHeight
    const mainHeight = Math.max(6, height - inputHeight - topHeight)
    const rightWidth = Math.max(34, Math.min(52, Math.floor(width * 0.32)))
    const panelWidth = Math.max(20, Math.min(rightWidth, width - outerX * 2 - gutter - 24))
    const panelX = Math.max(outerX + 24 + gutter, width - panelWidth - outerX)
    const chatWidth = Math.max(24, panelX - outerX - gutter)

    return { mainY, mainHeight, chatX: outerX, chatWidth, panelX, panelWidth }
  }

  private renderTopBar(buffer: OptimizedBuffer): void {
    const left = "⊙ flyflor-chat · powered by OpenTUI"
    const right = `● flyflor · ready · ${this.turns} turns`
    buffer.fillRect(this.x, this.y, this.width, 1, colors.panel)
    buffer.drawText(left, this.x + 2, this.y, colors.muted, colors.bg)
    buffer.drawText("⊙", this.x + 2, this.y, colors.purple, colors.bg)
    const rx = Math.max(this.x + 2, this.x + this.width - right.length - 3)
    buffer.drawText(right, rx, this.y, colors.muted, colors.bg)
    buffer.drawText("●", rx, this.y, colors.green, colors.bg)
  }

  private submit(): void {
    const text = this.input.trim()
    if (!text) {
      this.chat.scrollToBottom()
      return
    }
    if (text === "/exit") {
      this.exit()
      return
    }

    this.chat.appendBlock({
      id: this.nextBlockId++,
      role: "user",
      lines: [text],
    })
    this.chat.appendBlock(this.createAssistantReply(text))
    this.turns += 1
    this.input = ""
    this.chat.scrollToBottom()
  }

  private createAssistantReply(text: string): ChatBlock {
    return {
      id: this.nextBlockId++,
      role: "assistant",
      lines: [
        "## 收到，我用 **mock** 模型跑一轮。",
        `你刚才说：\`${text}\``,
        "我会把它追加到虚拟列表底部，并保持 bottom anchor；如果你向上翻页，后续内容不会把阅读位置硬拽走。",
        "- 现在可以继续输入",
        "- 或者拖动左侧滚动条测试大列表定位",
        "> `Ctrl+E` 可以一键回到底部。",
      ],
    }
  }

  private renderRightPanel(buffer: OptimizedBuffer): void {
    const layout = this.getLayout()
    const panelX = layout.panelX
    const panelY = layout.mainY
    const panelWidth = layout.panelWidth
    const panelHeight = layout.mainHeight
    if (panelWidth < 20) return

    drawOpenTopBorder(buffer, panelX, panelY, panelWidth, panelHeight)
    const x = panelX + 2
    let y = panelY
    const w = panelWidth - 4
    const line = (text: string, color = colors.text) => {
      if (y >= panelY + panelHeight - 1) return
      buffer.drawText(padToWidth(text, w), x, y++, color, colors.bg)
    }

    line("Blackboard  [Ctrl+B Thinking]", colors.text)
    y++
    line("Questions", colors.blue)
    line("  1. 花花宝宝晚上好。", colors.muted)
    line("> 2. 我要设计一个绝对安全的加密通讯协议。", colors.pink)
    line("  全天候！服务器不留任何日志，...", colors.pink)
    y++
    line("Blackboard", colors.text)
    line("  failed · 1 steps · 0 decisions", colors.text)
    line("  goal:", colors.text)
    line("我想设计一个绝对安全的加密通讯协议，", colors.muted)
    line("全天候！服务器不留任何日志，也不留任何元数据。", colors.muted)
    line("全溯源：在发生非法传输时，必须能100%溯源到", colors.muted)
    line("发送者身份。高性能：支持亿级并发，且协议必须在", colors.muted)
    line("片机低算力设备上运行。", colors.muted)
    y++
    horizontal(buffer, x, y++, w)
    y++
    line("TODO List", colors.text)
    const todos = ["明确需求边界与冲突", "设计协议核心架构", "威胁模型与安全假设", "关键技术选型与权衡", "性能与扩展性设计", "审计与形式化验证方案", "形成两版方案对比"]
    todos.forEach((todo, index) => {
      line(`${index === 1 ? "›" : "○"} ${todo}${" ".repeat(Math.max(1, w - todo.length - 12))}${index === 0 ? "进行中" : "待开始"}`, index === 1 ? colors.pink : colors.text)
    })
    y++
    horizontal(buffer, x, y++, w)
    y++
    line("MODEL", colors.blue)
    line("model      flyflor-pro", colors.text)
    line("provider   OpenTUI", colors.text)
    line("temperature 0.7", colors.text)
    line("top_p      1.0", colors.text)
    y++
    line("TOKENS                 CONTEXT WINDOW", colors.blue)
    line("input      2,131       128K        24%", colors.text)
    line("output     1,024       █████░░░░░░░░", colors.purple)
    line("total      3,155       31,744 / 128,000", colors.text)
    line("◷ 00:12:34 | 9 turns | local | ● healthy", colors.muted)
  }

  private renderInput(buffer: OptimizedBuffer): void {
    const inputHeight = 4
    const y = Math.max(0, this.height - inputHeight)
    const x = 0
    const width = Math.max(10, this.width)
    drawBorder(buffer, x, y, width, inputHeight)
    const prompt = this.input.length > 0 ? this.input : "ask anything..."
    buffer.drawText(padToWidth(prompt, Math.max(1, width - 4)), x + 2, y + 1, this.input ? colors.text : colors.muted, colors.input)
    buffer.drawText("Enter 发送  |  ↑/↓ 滚动  |  Ctrl+E 到底  |  Tab 切换  |  Cmd/Ctrl+C 复制", x + 2, y + 3, colors.muted, colors.bg)
    buffer.drawText(">_", x + width - 5, y + 1, colors.purple, colors.bg)
  }
}

class BottomButtonRenderable extends Renderable {
  private readonly onClick: () => void
  private readonly label = " Bottom "

  constructor(ctx: RenderContext, options: RenderableOptions<BottomButtonRenderable> & { onClick: () => void }) {
    super(ctx, {
      ...options,
      width: 8,
      height: 1,
      buffered: true,
      zIndex: 10,
    })
    this.onClick = options.onClick
    this.onMouseDown = (event) => {
      this.onClick()
      event.stopPropagation()
    }
  }

  protected override renderSelf(buffer: OptimizedBuffer): void {
    buffer.drawText(this.label, 0, 0, colors.text, colors.panel2)
  }
}

function drawBorder(buffer: OptimizedBuffer, x: number, y: number, width: number, height: number): void {
  if (width < 2 || height < 2) return
  buffer.drawText("┌" + "─".repeat(width - 2) + "┐", x, y, colors.border, colors.bg)
  for (let row = 1; row < height - 1; row++) {
    buffer.drawText("│", x, y + row, colors.border, colors.bg)
    buffer.drawText("│", x + width - 1, y + row, colors.border, colors.bg)
  }
  buffer.drawText("└" + "─".repeat(width - 2) + "┘", x, y + height - 1, colors.border, colors.bg)
}

function drawOpenTopBorder(buffer: OptimizedBuffer, x: number, y: number, width: number, height: number): void {
  if (width < 2 || height < 2) return
  for (let row = 0; row < height - 1; row++) {
    buffer.drawText("│", x, y + row, colors.border, colors.bg)
    buffer.drawText("│", x + width - 1, y + row, colors.border, colors.bg)
  }
  buffer.drawText("└" + "─".repeat(width - 2) + "┘", x, y + height - 1, colors.border, colors.bg)
}

function horizontal(buffer: OptimizedBuffer, x: number, y: number, width: number): void {
  buffer.drawText("─".repeat(Math.max(0, width)), x, y, colors.dimBorder, colors.bg)
}
