export class FenwickTree {
  private tree: number[]
  private values: number[]

  constructor(values: number[]) {
    this.values = new Array(values.length).fill(0)
    this.tree = new Array(values.length + 1).fill(0)
    values.forEach((value, index) => this.add(index, value))
  }

  get length(): number {
    return this.tree.length - 1
  }

  add(index: number, delta: number): void {
    if (index >= 0 && index < this.values.length) {
      this.values[index] += delta
    }
    for (let i = index + 1; i < this.tree.length; i += i & -i) {
      this.tree[i] += delta
    }
  }

  push(value: number): void {
    this.values.push(value)
    const treeIndex = this.values.length
    const span = treeIndex & -treeIndex
    let sum = 0
    for (let i = treeIndex - span; i < treeIndex; i++) {
      sum += this.values[i]
    }
    this.tree.push(sum)
  }

  prefix(indexExclusive: number): number {
    let sum = 0
    for (let i = Math.max(0, Math.min(indexExclusive, this.length)); i > 0; i -= i & -i) {
      sum += this.tree[i]
    }
    return sum
  }

  total(): number {
    return this.prefix(this.length)
  }

  lowerBound(target: number): number {
    if (target <= 0) return 0
    let index = 0
    let bit = 1
    while (bit << 1 < this.tree.length) bit <<= 1

    for (; bit > 0; bit >>= 1) {
      const next = index + bit
      if (next < this.tree.length && this.tree[next] <= target) {
        target -= this.tree[next]
        index = next
      }
    }

    return Math.min(index, this.length - 1)
  }
}
