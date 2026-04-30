const TOKEN_KEY = 'econ_token';
const API_BASE = '/api/econ';

export const auth = {
  get: () => localStorage.getItem(TOKEN_KEY) || '',
  set: (t: string) => localStorage.setItem(TOKEN_KEY, t),
  clear: () => localStorage.removeItem(TOKEN_KEY),
};

export class AuthError extends Error {
  constructor() { super('não autenticado'); }
}

export async function api<T = unknown>(method: string, path: string, body?: unknown): Promise<T> {
  const r = await fetch(API_BASE + path, {
    method,
    headers: {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer ' + auth.get(),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  let data: any = null;
  try { data = await r.json(); } catch {}
  if (!r.ok) {
    if (r.status === 401) {
      auth.clear();
      throw new AuthError();
    }
    throw new Error((data && data.error) || ('http ' + r.status));
  }
  return data as T;
}

export async function login(password: string): Promise<string> {
  const r = await fetch(API_BASE + '/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ password }),
  });
  let data: any = null;
  try { data = await r.json(); } catch {}
  if (!r.ok || !data.token) {
    throw new Error((data && data.error) || ('http ' + r.status));
  }
  auth.set(data.token);
  return data.token;
}
