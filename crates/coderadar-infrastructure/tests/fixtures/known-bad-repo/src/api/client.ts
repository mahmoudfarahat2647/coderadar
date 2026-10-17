const baseUrl = "http://api.example.com/v1";
const insecureAgent = { rejectUnauthorized: false };
export function getBaseUrl(): string {
  return baseUrl;
}
