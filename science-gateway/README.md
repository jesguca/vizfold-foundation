# VizFold Gateway

Prototype for the VizFold Science Gateway.

## Structure

- `apps/workbench`: browser-based VizFold interface, runnable locally and deployable later as a science gateway frontend.
- `apps/executor`: Rust backend executor service currently exposing an Axum HTTP adapter.
- `packages/schemas`: shared data contracts.
- `packages/adapters`: model adapter interfaces and implementations.
- `examples`: sample inputs and outputs.
- `docs`: architecture and implementation notes.

## Local Development

### Prerequisites

- Node.js 20 LTS or later
- npm
- Git
- Rust toolchain (`cargo`, `rustc`)

> **Recommended:** Use `nvm` (Linux/macOS/WSL) or `nvm-windows` (Windows) to manage Node.js versions.

Verify your installation:

```bash
node -v
npm -v
cargo -V
```

---

### Clone the repository

```bash
git clone ...
```

---

### Install dependencies

```bash
cd science-gateway/apps/workbench
npm install
```

---

### Configure environment variables

Create a local `.env` file from the example:

```bash
cp .env.example .env
```

On Windows PowerShell:

```powershell
Copy-Item .env.example .env
```

or simply duplicate the file manually.

---

### Configure Prisma

```bash
npx prisma generate
npx prisma migrate dev
```

---

### Start the application

```bash
npm run dev
```

The application will be available at:

```
http://localhost:3000
```

## Executor Development

Run the Axum API locally from the gateway root:

```bash
cd apps/executor
cargo run
```

The health check will be available at:

```
http://127.0.0.1:3001/health
```

By default, the executor uses SQLite at `apps/executor/data/vizfold.db` and applies SeaORM migrations automatically on startup. To override the location, set `DATABASE_URL`, for example:

```powershell
$env:DATABASE_URL = "sqlite://data/vizfold.db?mode=rwc"
cargo run
```
