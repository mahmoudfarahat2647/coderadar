interface Request {
  body: { user: { name: string } };
}

export function userName(req: Request): string {
  const name = req.body.user.name;
  return name;
}
