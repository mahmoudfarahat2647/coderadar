import { getRepository } from "typeorm";

export function placeOrder(orderId: string): string {
  return `order:${orderId}`;
}
