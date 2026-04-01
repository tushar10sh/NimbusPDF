# NimbusPDF

A self-hosted, containerized PDF reader with an offline-capable AI assistant. All data lives on your machine in plain files you can read, edit, and download at any time.

---

## Table of Contents

1. [Features](#features)
2. [Architecture Overview](#architecture-overview)
3. [Project Structure](#project-structure)
   - [Backend](#backend-rustaxum)
   - [Frontend](#frontend-sveltekit)
   - [Infrastructure](#infrastructure)
4. [Quick Start](#quick-start)
5. [Development Setup](#development-setup)
6. [Configuration Reference](#configuration-reference)
7. [Airgapped Deployment](#airgapped-deployment)
8. [Authentication](#authentication)
9. [AI Endpoint Setup](#ai-endpoint-setup)
10. [Data Layout](#data-layout)
11. [Debugging Guide](#debugging-guide)

---

## Features

- **PDF Viewer** — continuous page layout, zoom, keyword search, text-selection highlights (4 colors), per-page notes
- **AI Assistant** — chat sidebar with per-document history; quick Summary and Key Points actions
- **Pluggable AI** — works with any OpenAI-compatible endpoint (Ollama, LM Studio, OpenAI, etc.)
- **Long-term Memory** — on upload, optionally summarise a document into a user-editable memory file
- **Document Categories** — group documents into a graph-RAG-ready node/edge structure
- **Optional Auth** — OpenID Connect login; app is fully functional without it
- **Google Drive Sync** — authenticated users can connect Drive for cloud backup
- **Data Portability** — everything is plain files under `data/`; zip and move at any time
- **Airgap-ready** — Docker images are fully self-contained; no internet access needed at runtime

---

## Architecture Overview

```
Browser
  │
  └─► nginx :80
        ├─► /api/*  ──► backend (Rust/Axum) :3000
        │                  ├── Session middleware (HMAC-signed cookies)
        │                  ├── File storage (data/)
        │                  ├── PDF text extraction (pdftotext)
        │                  └── AI proxy (→ user-configured endpoint)
        │
        └─► /*      ──► frontend (SvelteKit/Node) :3000
                         ├── PDF rendering (pdfjs-dist, worker bundled)
                         ├── Svelte stores (auth, pdf state)
                         └── Tailwind CSS (no CDN, fully local)
```

The three services run as Docker containers. In development, Vite proxies `/api` to the Rust server so you only need one origin.

---

## Project Structure

```
NimbusPDF/
├── docker-compose.yml       # Orchestrates nginx + backend + frontend
├── nginx.conf               # Reverse proxy: /api → backend, / → frontend
├── .env.example             # Copy to .env and fill in secrets
├── .gitignore
├── data/                    # Runtime data (gitignored, created on first run)
│
├── backend/                 # Rust/Axum API server
│   ├── Cargo.toml
│   ├── Cargo.lock
│   ├── Dockerfile
│   ├── config/              # Mounted read-only into the container
│   │   ├── default.toml           # Server, session, AI, storage settings
│   │   ├── ai_system_prompt.md          # Default chat system prompt
│   │   ├── ai_system_prompt.summary.md  # Summary quick-action prompt
│   │   └── ai_system_prompt.keypoints.md
│   └── src/
│       ├── main.rs          # Server bootstrap, AppState, middleware wiring
│       ├── config.rs        # Typed config (TOML + NIMBUS__ env vars)
│       ├── session.rs       # HMAC-signed cookies, disk sessions, Axum middleware
│       ├── pdf_text.rs      # pdftotext subprocess wrapper (text extraction)
│       ├── gdrive.rs        # Google Drive OAuth2 client
│       ├── ai/mod.rs        # AiProxy (OpenAI-compat), load_prompt()
│       ├── auth/mod.rs      # (re-exports, types live in session.rs)
│       ├── storage/
│       │   ├── mod.rs       # Principal enum, module re-exports
│       │   └── local.rs     # LocalStorage — all file I/O
│       └── routes/
│           ├── mod.rs       # Router assembly (no state consumed here)
│           ├── pdfs.rs      # Upload, list, serve (range requests), delete
│           ├── ai.rs        # Chat, summary, keypoints, history, AI config
│           ├── highlights.rs # GET/PUT highlights.json per document
│           ├── notes.rs     # GET/PUT notes.json (per-page, full or single)
│           ├── memory.rs    # Long-term memory markdown + AI append
│           ├── categories.rs # Knowledge graph (nodes + edges) CRUD
│           └── auth.rs      # OIDC login/callback/logout, Google Drive OAuth, /me
│
└── frontend/                # SvelteKit app
    ├── package.json
    ├── svelte.config.js     # adapter-node + path aliases ($api, $components, $stores)
    ├── vite.config.js       # viteStaticCopy (PDF.js worker), /api proxy for dev
    ├── postcss.config.js
    ├── tailwind.config.js
    ├── Dockerfile
    └── src/
        ├── app.html         # HTML shell
        ├── app.css          # Tailwind directives + CSS variables
        ├── lib/
        │   ├── api/
        │   │   └── client.js        # Thin fetch wrapper (get/post/put/delete/upload)
        │   ├── stores/
        │   │   ├── auth.js          # Auth state (user, loading); calls GET /api/auth/me
        │   │   └── pdf.js           # Viewer state (zoom, page, highlights, notes)
        │   └── components/
        │       ├── PDFViewer.svelte     # pdfjs-dist renderer, highlight overlay, color picker
        │       ├── AISidebar.svelte     # Chat UI, history load, Summary/Key Points buttons
        │       ├── Notes.svelte         # Per-page textarea with debounced autosave
        │       ├── MemoryEditor.svelte  # Split markdown editor + marked preview
        │       └── CategoryManager.svelte # Document→category assignment, graph CRUD
        └── routes/
            ├── +layout.svelte           # Auth init on mount
            ├── +page.svelte             # Document library + upload
            ├── viewer/[docId]/
            │   └── +page.svelte         # Viewer + AI sidebar + Notes drawer
            ├── settings/
            │   └── +page.svelte         # AI endpoint config + Google Drive connect
            ├── memory/
            │   └── +page.svelte         # Long-term memory editor (auth required)
            └── categories/
                └── +page.svelte         # Category management (auth required)
```

---

## Quick Start

**Prerequisites:** Docker and Docker Compose.

```bash
# 1. Clone and enter the repo
git clone <repo-url> NimbusPDF && cd NimbusPDF

# 2. Create your environment file
cp .env.example .env
# Edit .env — at minimum set SESSION_SECRET to a random string

# 3. Build and start
docker compose up --build

# 4. Open http://localhost
```

The app is fully functional without filling in any OIDC or AI credentials. You can add a PDF and configure your AI endpoint from the Settings page.

---

## Development Setup

### Backend (Rust)

```bash
cd backend

# First run — downloads and compiles all crates (~2 min)
cargo build

# Run the dev server (reads config from ./config/, data from ./data/)
cargo run

# Run all tests
cargo test

# Run a single test by name
cargo test <test_name>

# Type-check without linking (fast)
cargo check
```

The backend listens on `http://localhost:3000` by default.

**Required system tool:** `poppler-utils` must be installed for PDF text extraction.
```bash
# macOS
brew install poppler

# Debian/Ubuntu
sudo apt-get install poppler-utils
```

### Frontend (Node)

```bash
cd frontend

npm install

# Dev server with hot reload — proxies /api to localhost:3000
npm run dev          # http://localhost:5173

# Type-check
npm run check

# Lint
npm run lint

# Production build (output to build/)
npm run build
```

### Running both together in development

Open two terminals:
```bash
# Terminal 1
cd backend && cargo run

# Terminal 2
cd frontend && npm run dev
```

Navigate to `http://localhost:5173`. All `/api` calls are proxied to the Rust server.

---

## Configuration Reference

### `backend/config/default.toml`

| Key | Default | Description |
|---|---|---|
| `server.host` | `0.0.0.0` | Bind address |
| `server.port` | `3000` | Listen port |
| `server.data_dir` | `./data` | Root for all user data |
| `server.config_dir` | `./config` | Directory containing prompt files and this TOML |
| `session.cookie_name` | `nimbus_session` | Cookie name |
| `session.anonymous_ttl` | `86400` | Anonymous session lifetime in seconds (24 h) |
| `ai.system_prompt_file` | `ai_system_prompt.md` | Chat prompt template filename |
| `ai.summary_prompt_file` | `ai_system_prompt.summary.md` | Summary prompt filename |
| `ai.keypoints_prompt_file` | `ai_system_prompt.keypoints.md` | Key points prompt filename |
| `ai.max_context_tokens` | `4096` | Max words of PDF text sent to AI |
| `auth.require_auth` | `false` | Set `true` to block unauthenticated access |

Any key can be overridden with an environment variable using the `NIMBUS__` prefix with `__` as separator:
```bash
NIMBUS__SERVER__PORT=8080
NIMBUS__AUTH__REQUIRE_AUTH=true
```

### `.env` file

| Variable | Required | Description |
|---|---|---|
| `SESSION_SECRET` | **Yes** | Random string used to sign session cookies (min 32 chars) |
| `OIDC_ISSUER_URL` | No | OIDC provider base URL. Auth is disabled when unset |
| `OIDC_CLIENT_ID` | If OIDC | Client ID from your OIDC provider |
| `OIDC_CLIENT_SECRET` | If OIDC | Client secret |
| `OIDC_REDIRECT_URI` | If OIDC | Must match a URI registered with your provider |
| `GOOGLE_CLIENT_ID` | No | For Google Drive sync |
| `GOOGLE_CLIENT_SECRET` | No | For Google Drive sync |
| `GDRIVE_REDIRECT_URI` | If Drive | Must be registered in Google Cloud Console |

### AI Prompt Templates

The three files in `backend/config/` are loaded at request time — edit them without restarting the server. Each uses a single placeholder:

```
{document_context}   ← replaced with extracted PDF text at request time
```

---

## Airgapped Deployment

The Docker images contain every dependency. Once built, they need no internet access.

### Build on a machine with internet access

```bash
docker compose build
```

### Export images to a tar archive

```bash
docker save \
  nimbuspdf-backend \
  nimbuspdf-frontend \
  nginx:alpine \
  | gzip > nimbuspdf-images.tar.gz
```

### Transfer and load on the airgapped machine

```bash
# Copy nimbuspdf-images.tar.gz and the project directory to the target machine

# Load images
docker load < nimbuspdf-images.tar.gz

# Run (no build needed)
docker compose up
```

### What makes the images self-contained

| Concern | How it's handled |
|---|---|
| PDF.js worker | Copied into the SvelteKit build output by `vite-plugin-static-copy` at build time |
| Rust crates | All compiled into a static binary at build time |
| Node modules | `npm ci --omit=dev` run inside the builder stage; `build/` is copied into runtime image |
| SSL certificates | `ca-certificates` installed in the backend runtime image (needed for outbound AI/OIDC calls) |
| PDF text extraction | `poppler-utils` installed in the backend runtime image |
| Fonts / CSS | Tailwind generates all CSS at build time; no Google Fonts or CDN links anywhere |
| AI endpoint | Configured by the user — points to their own Ollama/LM Studio/other instance |

> **Note:** The AI endpoint you configure in Settings must be reachable from the backend container. For a local Ollama instance, use `http://host.docker.internal:11434/v1/chat/completions` on Docker Desktop (Mac/Windows) or the host's LAN IP on Linux.

---

## Authentication

Authentication is **optional**. Without it, the app creates an anonymous session (cookie-backed, TTL configurable) and all features except long-term memory and categories are available.

### Enabling OIDC

1. Register a client with your OIDC provider (Keycloak, Auth0, Google, GitHub via an OIDC bridge, etc.)
2. Set the redirect URI to `http://<your-host>/api/auth/callback`
3. Add the four `OIDC_*` vars to `.env`
4. Restart the stack

The login flow uses PKCE + CSRF state, so no client secret is technically required for public clients — set `OIDC_CLIENT_SECRET=` to empty in that case.

### Session storage

Sessions are stored as JSON files in `data/sessions/<id>.json`. They are HMAC-SHA256 signed via `SESSION_SECRET`. Anonymous sessions expire after `session.anonymous_ttl` seconds; authenticated sessions have no TTL until logout.

---

## AI Endpoint Setup

From the **Settings** page (or `PUT /api/ai/config`), configure:

| Field | Example |
|---|---|
| Endpoint URL | `http://localhost:11434/v1/chat/completions` |
| Model | `llama3`, `mistral`, `gpt-4o`, etc. |
| API Key | Leave blank for local models; required for OpenAI/Anthropic |

Any OpenAI-compatible endpoint works. Tested with:
- **Ollama** — `http://host.docker.internal:11434/v1/chat/completions`
- **LM Studio** — `http://host.docker.internal:1234/v1/chat/completions`
- **OpenAI** — `https://api.openai.com/v1/chat/completions`

The API key is stored in `data/[users|anonymous/sessions]/<id>/settings/ai_config.toml`. It is not encrypted at rest in the current version — use filesystem-level encryption for sensitive deployments.

---

## Data Layout

All data lives under `data/` and is human-readable:

```
data/
├── sessions/                   # Server-side session files (all users)
│   └── <uuid>.json
│
├── anonymous/
│   └── sessions/
│       └── <session-id>/
│           └── pdfs/
│               └── <doc-id>/
│                   ├── original.pdf
│                   ├── highlights.json   # Array of highlight objects
│                   ├── notes.json        # { "1": { content, updated_at }, ... }
│                   ├── chat_history.json # Array of { role, content, timestamp }
│                   └── metadata.json     # id, filename, page_count, uploaded_at
│
└── users/
    └── <oidc-subject>/
        ├── pdfs/
        │   └── <doc-id>/          # Same structure as anonymous above
        ├── memory/
        │   └── long_term_memory.md  # User-editable markdown
        ├── categories/
        │   └── graph.json           # { nodes: [...], edges: [...] }
        └── settings/
            ├── ai_config.toml       # endpoint_url, model, api_key
            └── gdrive_token.json    # OAuth2 token (when Drive is connected)
```

To back up or migrate a user: copy their folder under `data/users/<subject>/`.

---

## Debugging Guide

### Backend won't start

```bash
# Check logs
docker compose logs backend

# Common causes:
# - SESSION_SECRET not set in .env
# - config/default.toml missing (must be present in backend/config/)
# - Port 3000 already in use on host
```

### PDF upload fails / 500 error

```bash
# Verify poppler-utils is installed in the container
docker compose exec backend pdftotext -v

# Check the data directory is writable
docker compose exec backend ls -la /app/data

# Look at the backend log for the actual error
docker compose logs -f backend
```

### PDF text not extracted (AI has no document context)

```bash
# Test pdftotext directly
docker compose exec backend pdftotext /app/data/users/<id>/pdfs/<doc>/original.pdf -

# If it outputs nothing: the PDF is image-based (scanned) — pdftotext
# only works on text-layer PDFs. OCR support is a planned future feature.
```

### AI chat returns "AI endpoint not configured"

The user-specific `ai_config.toml` is missing. Go to **Settings** and fill in the endpoint URL and model name.

### AI chat returns "AI request failed"

```bash
# From inside the backend container, test the endpoint directly
docker compose exec backend \
  curl -s -X POST http://host.docker.internal:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3","messages":[{"role":"user","content":"hi"}],"stream":false}'

# Common causes:
# - Wrong endpoint URL (check Settings)
# - Ollama/LM Studio not running or not accessible from the container
# - On Linux Docker: use host LAN IP instead of host.docker.internal
```

### OIDC login redirects to error page

```bash
# Check the discovery endpoint is reachable from the backend container
docker compose exec backend \
  curl -s "$OIDC_ISSUER_URL/.well-known/openid-configuration" | head -20

# Verify OIDC_REDIRECT_URI matches exactly what is registered with the provider
# (including http vs https and trailing slash)
```

### Session not persisting across requests

- Verify `SESSION_SECRET` is set and consistent across restarts (not regenerated)
- Check that the browser is sending the `nimbus_session` cookie (dev tools → Network)
- The cookie is `HttpOnly; SameSite=Lax` — it won't appear in `document.cookie`

### Google Drive sync not working

```bash
# Check token file exists and is valid JSON
docker compose exec backend \
  cat /app/data/users/<subject>/settings/gdrive_token.json

# Token refresh logs appear at INFO level
docker compose logs backend | grep -i gdrive
```

### Frontend build fails (npm)

```bash
cd frontend

# Clear caches
rm -rf node_modules .svelte-kit build
npm install
npm run build

# If PDF.js worker is missing from build output:
ls build/pdf.worker.min.js
# Should exist — produced by vite-plugin-static-copy
# If missing: verify vite.config.js has the viteStaticCopy plugin
```

### Tailwind classes not applied

```bash
# Verify tailwind.config.js content paths cover all Svelte files
cat frontend/tailwind.config.js

# Rebuild
cd frontend && npm run build
```

### Running cargo check / clippy locally

```bash
cd backend

# Fast type check
~/.cargo/bin/cargo check

# Linter
~/.cargo/bin/cargo clippy

# Full build
~/.cargo/bin/cargo build --release
```

---

## Ports Summary

| Service | Internal port | External (docker compose) |
|---|---|---|
| nginx | 80 | **80** (access point) |
| backend | 3000 | not exposed directly |
| frontend | 3000 | not exposed directly |

In development (no Docker):

| Service | Port |
|---|---|
| Rust backend | 3000 |
| Vite dev server | 5173 |
