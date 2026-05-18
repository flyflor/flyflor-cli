import { createTestRenderer } from "@opentui/core/testing"
import { FlyflorShellRenderable } from "./shell.ts"

const { renderer, renderOnce, captureCharFrame, mockInput, mockMouse } = await createTestRenderer({
  width: 140,
  height: 42,
})

const shell = new FlyflorShellRenderable(renderer, { id: "flyflor-shell" })
renderer.root.add(shell)
shell.focus()
await renderOnce()
await mockInput.typeText("**粗体** `code` > quote")
await mockInput.pressKey("RETURN")
await renderOnce()
await mockMouse.drag(92, 35, 92, 8)
await renderOnce()
const beforeClick = captureCharFrame().split("\n")
const buttonY = beforeClick.findIndex((line) => line.includes("Bottom"))
const buttonX = buttonY >= 0 ? beforeClick[buttonY].indexOf("Bottom") : 125
await mockMouse.click(buttonX + 2, buttonY >= 0 ? buttonY : 39)
await renderOnce()
await renderOnce()

const lines = captureCharFrame().split("\n")
console.log(lines.slice(0, 12).join("\n"))
console.log("\n--- bottom ---")
console.log(lines.slice(-12).join("\n"))
renderer.destroy()
