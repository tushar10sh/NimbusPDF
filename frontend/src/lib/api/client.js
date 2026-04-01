const BASE = '/api';

async function request(method, path, body) {
  const opts = {
    method,
    headers: {},
  };

  if (body instanceof FormData) {
    opts.body = body;
  } else if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }

  const res = await fetch(`${BASE}${path}`, opts);

  if (!res.ok) {
    let message;
    try {
      const json = await res.json();
      message = json.error ?? JSON.stringify(json);
    } catch {
      message = await res.text().catch(() => res.statusText);
    }
    throw new Error(message || `HTTP ${res.status}`);
  }

  if (res.status === 204) return null;
  return res.json();
}

export const api = {
  get: (path) => request('GET', path),
  post: (path, body) => request('POST', path, body),
  put: (path, body) => request('PUT', path, body),
  delete: (path) => request('DELETE', path),
  upload: (path, formData) => request('POST', path, formData),
};
