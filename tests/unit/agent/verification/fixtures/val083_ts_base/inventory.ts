export interface Item {
  readonly sku: string;
  name: string;
  qty: number;
  price: number;
}

export class Inventory {
  #items = new Map<string, Item>();

  addItem(item: Item): void {
    const existing = this.#items.get(item.sku);
    if (existing) {
      existing.qty += item.qty;
      return;
    }
    this.#items.set(item.sku, { ...item });
  }

  get(sku: string): Item | undefined {
    return this.#items.get(sku);
  }

  *entries(): IterableIterator<Item> {
    for (const item of this.#items.values()) {
      yield item;
    }
  }

  get size(): number {
    return this.#items.size;
  }
}
