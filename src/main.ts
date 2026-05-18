#!/usr/bin/env bun
import { createCliRenderer } from "@opentui/core"
import { FlyflorShellRenderable } from "./shell.ts"
import { colors } from "./theme.ts"

const renderer = await createCliRenderer({
  targetFps: 60,
  maxFps: 60,
  useMouse: true,
  enableMouseMovement: true,
  exitOnCtrlC: true,
  backgroundColor: colors.bg,
})

const shell = new FlyflorShellRenderable(renderer, { id: "flyflor-shell" })
renderer.root.add(shell)
shell.focus()
renderer.setFrameCallback(async () => {
  if (shell.shouldExit()) renderer.destroy()
})
renderer.start()
