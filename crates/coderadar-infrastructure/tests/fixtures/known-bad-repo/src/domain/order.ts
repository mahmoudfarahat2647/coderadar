import express from "express";
import { db } from "../infrastructure/db";

export interface Order {
  id: string;
}

export function listOrders(): Order[] {
  return db.selectAll("orders");
}
