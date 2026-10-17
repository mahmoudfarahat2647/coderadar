export async function notify(url: string): Promise<void> {
  await fetch(url);
}
