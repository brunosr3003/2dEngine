export type ApiError = Error & { status?: number };

export interface RegisterInput {
  username: string;
  email: string;
  password: string;
}

export interface LoginInput {
  username: string;
  password: string;
}

export interface AccountView {
  id: number;
  username: string;
}

async function request<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const data = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  if (!res.ok) {
    const msg = typeof data.error === "string" ? data.error : `HTTP ${res.status}`;
    const err: ApiError = new Error(msg);
    err.status = res.status;
    throw err;
  }
  return data as T;
}

export function registerAccount(input: RegisterInput): Promise<AccountView> {
  return request<AccountView>("/api/register", input);
}

export function loginAccount(input: LoginInput): Promise<AccountView> {
  return request<AccountView>("/api/login", input);
}
