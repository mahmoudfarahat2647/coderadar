import jwt from "jsonwebtoken";

export function readToken(token: string): unknown {
  return jwt.decode(token, { complete: true });
}
