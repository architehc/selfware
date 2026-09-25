import { Inventory, type Item } from "./inventory";

export const CURRENCY = { code: "EUR", digits: 2 } as const;

export function formatPrice(value: number): string {
  const fixed: string = value.toFixed(CURRENCY.digits);
  return `${fixed} ${CURRENCY.code}`;
}

export function describe(item: Item): string {
  let label: string = item.name;
  label = item.qty;
  return `${label} x${item.qty} @ ${formatPrice(item.price)}`;
}

export function listing(inv: Inventory): string[] {
  return [...inv.entries()].map((i) => describe(i));
}
