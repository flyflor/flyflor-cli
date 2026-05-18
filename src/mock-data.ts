export type ChatBlock = {
  id: number
  role: "assistant" | "user" | "system"
  lines: string[]
}

const sections = [
  {
    title: "4) \"绝对安全\" 不是可实现目标",
    body: [
      "任何现实协议都至少受制于：",
      "- 终端被攻破",
      "- 供应链植入",
      "- 随机数失效",
      "- 宽字符测量偏差",
      "- 侧信道攻击",
      "- 运维失误",
      "所以“绝对安全”只能作为目标口号，不能作为工程保证。",
    ],
  },
  {
    title: "可行的最小修改方向",
    body: [
      "如果你要的是尽量接近，只能在下面几条里选至少一条放松：",
      "1. 允许最小审计元数据",
      "   - 保留不可读、不可篡改的追责锚点",
      "   - 不保留明文日志",
      "2. 把 100% 溯源改成“在司法/审计授权不可溯源”",
      "   - 由外部可信机构隐蔽身份",
      "   - 协议本身不直接暴露身份",
      "3. 把“单片机直连亿级并发”改成“单片机终端 + 边缘网关/集群”",
      "   - MCU 只做轻量加密与签名",
      "   - 高并发状态由边缘层承载",
    ],
  },
  {
    title: "最终阻塞清单",
    body: [
      "如果不放松任何一条，明显点是：",
      "- 零信任与可追责不兼容",
      "- 100% 溯源需要外部可信基础设施",
      "- 亿级并发与单片机承载能力不兼容",
      "- 绝对安全不可工程化保证",
      "我可以下一步直接给你两版方案：保守可落地版、激进可演示版。",
    ],
  },
  {
    title: "虚拟滚动观测",
    body: [
      "这条消息故意包含长段中文、英文、数字和符号，用来测量换行稳定性。",
      "OpenTUI renders terminal cells, not DOM pixels. The viewport should keep its anchor while resizing.",
      "如果滚轮、PageUp、PageDown、Home、End 都稳定，才算过了第一关。",
      "混合宽度字符：AI、终端、中文、emoji-like 文本、/exit、Ctrl+B、Cmd/Ctrl+C。",
    ],
  },
]

export function createMockBlocks(count = 1200): ChatBlock[] {
  const blocks: ChatBlock[] = [
    {
      id: 0,
      role: "system",
      lines: ["▲ Type /exit to quit Flyflor chat."],
    },
  ]

  for (let i = 1; i <= count; i++) {
    const section = sections[i % sections.length]
    const role = i % 7 === 0 ? "user" : "assistant"
    blocks.push({
      id: i,
      role,
      lines: [
        `${String(i).padStart(4, "0")} · ${section.title}`,
        ...section.body,
        i % 9 === 0
          ? "额外长行：这是为了压测 CJK wrap 的一整段文本，宽度变化时必须重新测量高度，但滚动位置应该仍然围绕同一个内容锚点，而不是突然跳到奇怪的位置。"
          : "",
      ].filter(Boolean),
    })
  }

  return blocks
}

