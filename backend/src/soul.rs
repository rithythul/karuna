use std::sync::OnceLock;

static SOUL: OnceLock<String> = OnceLock::new();

const MAX_BYTES: usize = 2000;

/// Load the soul document from disk. Call once at startup.
pub fn load(path: &str) {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => {
            let truncated = if s.len() > MAX_BYTES {
                tracing::warn!("soul.md exceeds {MAX_BYTES} bytes, truncating");
                s[..MAX_BYTES].to_string()
            } else {
                s
            };
            tracing::info!("Loaded soul document ({} bytes)", truncated.len());
            truncated
        }
        Err(e) => {
            tracing::warn!("Could not load soul document from {path}: {e}");
            String::new()
        }
    };
    let _ = SOUL.set(content);
}

/// Returns the raw soul preamble text, or empty string if not loaded.
pub fn preamble() -> &'static str {
    SOUL.get().map(|s| s.as_str()).unwrap_or("")
}

/// Build a complete system prompt by combining the soul preamble,
/// role-specific instructions, and optional user context.
pub fn system_prompt(role_instructions: &str, user_context: Option<&str>) -> String {
    let soul = preamble();
    let mut prompt = String::with_capacity(soul.len() + role_instructions.len() + 256);

    if !soul.is_empty() {
        prompt.push_str(soul);
        prompt.push_str("\n\n---\n\n");
    }

    prompt.push_str(role_instructions);

    if let Some(ctx) = user_context {
        if !ctx.is_empty() {
            prompt.push_str("\n\n## What you know about this user\n\n");
            prompt.push_str(ctx);
        }
    }

    prompt
}
