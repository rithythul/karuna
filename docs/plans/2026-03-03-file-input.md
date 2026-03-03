# File/Context Input Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let users attach files to a task at submission time. Hanuman pre-loads those files into the sandbox before agents run, and mentions them in the planning prompt.

**Architecture:** `POST /api/tasks` gains multipart support (existing JSON path unchanged). Input files are stored as `artifact_type = "input"` rows in the existing `artifacts` table (no schema migration needed). The orchestrator loads input artifacts before planning and writes them into `/workspace/` at the start of each sandbox step. Frontend adds a file attachment zone below the textarea.

**Tech Stack:** Rust (axum multipart feature, base64 already in deps), React 19, TypeScript

---

### Task 1: Enable axum multipart feature

**Files:**
- Modify: `backend/Cargo.toml`

**Step 1: Add multipart to axum features**

Find the axum dependency line:
```toml
axum = { version = "0.8", features = ["ws", "json"] }
```

Change to:
```toml
axum = { version = "0.8", features = ["ws", "json", "multipart"] }
```

**Step 2: Compile check**

```bash
SQLX_OFFLINE=true cargo check 2>&1 | grep -E "^error"
```

Expected: no errors.

**Step 3: Commit**

```bash
git add backend/Cargo.toml
git commit -m "chore: enable axum multipart feature"
```

---

### Task 2: Add `create_input_artifact` to db.rs

**Files:**
- Modify: `backend/src/db.rs`

**Step 1: Add the function**

In `db.rs`, after any existing artifact function (search for `get_task_artifacts`), add:

```rust
/// Store a user-uploaded file as an input artifact before task execution.
/// Content is stored as base64-encoded text in the artifacts.content column.
pub async fn create_input_artifact(
    pool: &PgPool,
    task_id: Uuid,
    name: &str,
    mime_type: &str,
    content_base64: &str,
    size_bytes: i64,
) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO artifacts \
         (id, task_id, name, artifact_type, mime_type, content, size_bytes, created_at) \
         VALUES ($1, $2, $3, 'input', $4, $5, $6, NOW())",
        id,
        task_id,
        name,
        mime_type,
        content_base64,
        size_bytes,
    )
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(id)
}
```

**Step 2: Add `get_input_artifacts` function**

```rust
/// Fetch all input artifacts (uploaded by user) for a task.
pub async fn get_input_artifacts(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Vec<crate::models::Artifact>, AppError> {
    sqlx::query_as!(
        crate::models::Artifact,
        "SELECT id, task_id, step_id, name, artifact_type, mime_type, \
         path_in_sandbox, content, metadata, size_bytes, created_at \
         FROM artifacts \
         WHERE task_id = $1 AND artifact_type = 'input' \
         ORDER BY created_at ASC",
        task_id
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}
```

**Step 3: Compile check**

```bash
SQLX_OFFLINE=true cargo check 2>&1 | grep -E "^error"
```

Note: With `SQLX_OFFLINE=true`, sqlx uses cached query metadata. If you get "query not found in offline cache" errors, use `sqlx::query!` with the literal string instead of `query_as!`, or switch to `query_as::<_, crate::models::Artifact>(...)` with a string arg (not macro). Use the same pattern as existing queries in db.rs.

**Step 4: Commit**

```bash
git add backend/src/db.rs
git commit -m "feat: add create_input_artifact and get_input_artifacts db functions"
```

---

### Task 3: Add multipart task creation handler

**Files:**
- Modify: `backend/src/api.rs`

**Step 1: Add multipart imports**

At the top of `api.rs`, add to the existing `axum` import:
```rust
use axum::extract::Multipart;
use axum::http::Request;
use axum::body::Body;
```

Also add:
```rust
use base64::Engine as _;
```

**Step 2: Replace `create_task` with a dispatcher**

The current `create_task` handler accepts `Json(req)`. Replace it with one that checks `Content-Type` and dispatches:

```rust
async fn create_task(
    auth: AuthUser,
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<Json<CreateTaskResponse>, AppError> {
    let content_type = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.contains("multipart/form-data") {
        create_task_with_files(auth, state, request).await
    } else {
        // JSON path — extract body and deserialize
        let bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024)
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read body: {e}")))?;
        let req: CreateTaskRequest = serde_json::from_slice(&bytes)
            .map_err(|e| AppError::BadRequest(format!("Invalid JSON: {e}")))?;
        let task = db::create_task(&state.db, &auth.0.id, &req.goal).await?;
        state.orchestrator.enqueue(task.id).await?;
        Ok(Json(CreateTaskResponse { task_id: task.id }))
    }
}

async fn create_task_with_files(
    auth: AuthUser,
    state: AppState,
    request: Request<Body>,
) -> Result<Json<CreateTaskResponse>, AppError> {
    let mut multipart = Multipart::from_request(request, &state)
        .await
        .map_err(|e| AppError::BadRequest(format!("Invalid multipart: {e}")))?;

    let mut goal: Option<String> = None;
    // (filename, mime_type, raw_bytes)
    let mut uploads: Vec<(String, String, Vec<u8>)> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "goal" {
            goal = Some(
                field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read goal: {e}")))?,
            );
        } else {
            // Any other field is treated as a file attachment
            let filename = field
                .file_name()
                .unwrap_or("attachment")
                .to_string();
            let mime = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to read file {filename}: {e}")))?;

            if bytes.len() > 10 * 1024 * 1024 {
                return Err(AppError::BadRequest(format!(
                    "File '{}' exceeds 10 MB limit",
                    filename
                )));
            }
            uploads.push((filename, mime, bytes.to_vec()));
        }
    }

    if uploads.len() > 5 {
        return Err(AppError::BadRequest(
            "Maximum 5 files per task".to_string(),
        ));
    }

    let goal = goal.ok_or_else(|| AppError::BadRequest("Missing 'goal' field".into()))?;
    if goal.trim().is_empty() {
        return Err(AppError::BadRequest("Goal cannot be empty".into()));
    }

    let task = db::create_task(&state.db, &auth.0.id, &goal).await?;

    for (filename, mime, bytes) in uploads {
        let size = bytes.len() as i64;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        db::create_input_artifact(&state.db, task.id, &filename, &mime, &encoded, size).await?;
    }

    state.orchestrator.enqueue(task.id).await?;
    Ok(Json(CreateTaskResponse { task_id: task.id }))
}
```

**Step 3: Compile check**

```bash
SQLX_OFFLINE=true cargo check 2>&1 | grep -E "^error"
```

Note: If `axum::body::to_bytes` is not available directly, use `axum::body::Body` conversions or `hyper::body::to_bytes`. Check what axum 0.8 exposes and adjust — it may be `axum::body::to_bytes(body, limit)`.

**Step 4: Commit**

```bash
git add backend/src/api.rs
git commit -m "feat: add multipart task creation with file upload support"
```

---

### Task 4: Inject input files into orchestrator planning and sandbox

**Files:**
- Modify: `backend/src/orchestrator.rs`

**Step 1: Add `write_file_to_sandbox` helper to SandboxHandle**

Read `backend/src/tools/mod.rs` to find where `SandboxHandle` is defined. Add a method:

```rust
/// Write base64-encoded content into a file at /workspace/{filename} inside the sandbox.
pub async fn write_file_from_base64(
    &self,
    filename: &str,
    base64_content: &str,
) -> Result<(), AppError> {
    // Write in chunks to avoid shell argument length limits
    // First write base64 to a temp file, then decode it
    let safe_name = filename.replace('\'', "_").replace('/', "_");
    let tmp = format!("/tmp/.input_{safe_name}.b64");
    let dest = format!("/workspace/{safe_name}");

    // Write base64 to temp file using printf
    let write_cmd = format!(
        "printf '%s' '{}' > {}",
        base64_content.replace('\'', "'\\''"),
        tmp
    );
    let decode_cmd = format!("base64 -d {} > {} && rm {}", tmp, dest, tmp);

    let r1 = self.run_shell_cmd(&write_cmd).await?;
    if r1.exit_code != 0 {
        tracing::warn!("Failed to write temp file for {filename}: {}", r1.stderr);
        return Ok(());
    }
    let r2 = self.run_shell_cmd(&decode_cmd).await?;
    if r2.exit_code != 0 {
        tracing::warn!("Failed to decode {filename} into sandbox: {}", r2.stderr);
    }
    Ok(())
}

/// Run a shell command in the sandbox (internal helper).
async fn run_shell_cmd(&self, cmd: &str) -> Result<crate::sandbox::ExecResult, AppError> {
    self.run_in_sandbox(&["bash", "-c", cmd]).await
}
```

Note: Check the existing method names on `SandboxHandle`. It likely exposes a method to run commands — use whatever name already exists (e.g., `exec`, `run_in_sandbox`). Adapt accordingly.

**Step 2: Load input artifacts in execute_task_inner**

In `orchestrator.rs`, find `execute_task_inner`. After the `user_memories` loading block and before the planning phase, add:

```rust
// Load user-uploaded input files
let input_artifacts = db::get_input_artifacts(&self.pool, task_id)
    .await
    .unwrap_or_default();

let file_context = if input_artifacts.is_empty() {
    String::new()
} else {
    let file_list = input_artifacts
        .iter()
        .map(|a| format!(
            "- {} ({}, {} KB)",
            a.name,
            a.mime_type.as_deref().unwrap_or("unknown"),
            a.size_bytes.unwrap_or(0) / 1024,
        ))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "\n\nUser-provided files (available at /workspace/ in the sandbox):\n{}",
        file_list
    )
};
```

**Step 3: Inject file context into planning prompt**

In `create_plan`, add `file_context: &str` as a parameter. Change the call:

```rust
let plan = self.create_plan(&task.goal, user_context.as_deref(), &file_context).await?;
```

In `create_plan`, change the plan prompt's `Goal:` line:

```rust
let plan_prompt = format!(
    "Goal: {goal}{file_context}\n\nAvailable agents:\n...",
    // rest unchanged
);
```

**Step 4: Write input files to sandbox before each step**

In `execute_single_step` (or wherever each step acquires a sandbox), after the sandbox is acquired and before the agent loop runs, add:

```rust
// Pre-load user input files into the sandbox
for artifact in &input_artifacts {
    if let Some(content) = &artifact.content {
        if let Err(e) = sandbox_handle
            .write_file_from_base64(&artifact.name, content)
            .await
        {
            tracing::warn!("Failed to pre-load file {} into sandbox: {e}", artifact.name);
        }
    }
}
```

Pass `input_artifacts` as a parameter to `execute_single_step` (add it to the signature).

**Step 5: Compile check**

```bash
SQLX_OFFLINE=true cargo check 2>&1 | grep -E "^error"
```

Fix any type/signature mismatches. The orchestrator is the most complex file — read it carefully before editing.

**Step 6: Run all tests**

```bash
SQLX_OFFLINE=true cargo test 2>&1 | tail -20
```

Expected: all tests pass.

**Step 7: Commit**

```bash
git add backend/src/orchestrator.rs backend/src/tools/mod.rs
git commit -m "feat: pre-load input artifacts into sandbox and inject into planning prompt"
```

---

### Task 5: Frontend file attachment UI

**Files:**
- Modify: `frontend/src/app/page.tsx`

**Step 1: Add file state and refs**

In the component body, after existing `useState` calls, add:

```tsx
const [attachments, setAttachments] = useState<File[]>([]);
const fileInputRef = useRef<HTMLInputElement>(null);
```

**Step 2: Add file handlers**

```tsx
const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
  const selected = Array.from(e.target.files ?? []);
  const valid = selected
    .filter((f) => f.size <= 10 * 1024 * 1024)  // 10MB limit
    .slice(0, 5 - attachments.length);           // max 5 total
  setAttachments((prev) => [...prev, ...valid].slice(0, 5));
  e.target.value = ""; // reset so same file can be re-added after removal
};

const removeAttachment = (index: number) => {
  setAttachments((prev) => prev.filter((_, i) => i !== index));
};
```

**Step 3: Update submitGoal to use FormData when files are attached**

Find the existing `submitGoal` function. Replace the `fetch` call with:

```tsx
let res: Response;
if (attachments.length > 0) {
  const form = new FormData();
  form.append("goal", trimmed);
  attachments.forEach((f) => form.append("file", f, f.name));
  res = await fetch("/api/tasks", { method: "POST", body: form });
} else {
  res = await fetch("/api/tasks", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ goal: trimmed }),
  });
}
```

Also clear attachments on successful submit:
```tsx
setAttachments([]);
```

**Step 4: Add attachment UI below the textarea**

Below the textarea `<div>` (outside the form's textarea container but inside the form), add:

```tsx
{/* File attachments */}
<div className="mt-2 flex flex-wrap items-center gap-2">
  <button
    type="button"
    onClick={() => fileInputRef.current?.click()}
    disabled={isLoading || attachments.length >= 5}
    className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[12px] font-medium transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
    style={{
      background: "var(--bg-elevated)",
      color: "var(--text-secondary)",
      border: "1px solid var(--border-subtle)",
    }}
  >
    <svg width="12" height="12" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
      <path strokeLinecap="round" strokeLinejoin="round" d="M18.375 12.739l-7.693 7.693a4.5 4.5 0 01-6.364-6.364l10.94-10.94A3 3 0 1119.5 7.372L8.552 18.32m.009-.01l-.01.01m5.699-9.941l-7.81 7.81a1.5 1.5 0 002.112 2.13" />
    </svg>
    Attach files
  </button>

  {attachments.map((file, i) => (
    <div
      key={i}
      className="flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] max-w-[160px]"
      style={{
        background: "var(--bg-elevated)",
        color: "var(--text-secondary)",
        border: "1px solid var(--border-subtle)",
      }}
    >
      <span className="truncate">{file.name}</span>
      <button
        type="button"
        onClick={() => removeAttachment(i)}
        className="flex-shrink-0 cursor-pointer"
        style={{ color: "var(--text-tertiary)" }}
      >
        ×
      </button>
    </div>
  ))}

  <input
    ref={fileInputRef}
    type="file"
    multiple
    accept=".csv,.json,.txt,.pdf,.xlsx,.py,.js,.ts,.md,.png,.jpg,.jpeg,.gif,.webp"
    onChange={handleFileChange}
    className="hidden"
  />
</div>
```

**Step 5: Build check**

```bash
cd frontend && bun run build 2>&1 | tail -20
```

Expected: clean build.

**Step 6: Manual smoke test**

1. Start the app (`make dev`)
2. On the home page, click "Attach files" and select a CSV or text file
3. Verify the file chip appears with name and × button
4. Submit a task — verify no console errors, task is created and navigates to the task page
5. If backend is running, verify the file appears in the planning prompt (check reasoning traces)

**Step 7: Commit**

```bash
git add frontend/src/app/page.tsx
git commit -m "feat: add file attachment UI to task submission form"
```

---

### Summary

All four files modified:

| File | Change |
|------|--------|
| `backend/Cargo.toml` | Add `multipart` to axum features |
| `backend/src/db.rs` | Add `create_input_artifact`, `get_input_artifacts` |
| `backend/src/api.rs` | Multipart dispatcher + `create_task_with_files` |
| `backend/src/orchestrator.rs` | Load input artifacts, inject into plan, write to sandbox |
| `backend/src/tools/mod.rs` | Add `write_file_from_base64` to SandboxHandle |
| `frontend/src/app/page.tsx` | File attachment state + UI + FormData submit |
