import { db } from "../infrastructure/db";

export function findUser(name: string): string {
  const sql = "SELECT * FROM users WHERE name = '" + name + "'";
  return db.raw(sql);
}
