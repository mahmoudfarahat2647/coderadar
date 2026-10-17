declare const db: { query(sql: string): Promise<number> };

export async function countAll(ids: string[]): Promise<number> {
  let total = 0;
  for (const id of ids) {
    total += await db.query("select count(*) from t");
  }
  return total;
}
