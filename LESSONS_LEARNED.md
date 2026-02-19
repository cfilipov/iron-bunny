# Lessons Learned

Known pitfalls and solutions from debugging sessions. Read this at the start of every task.

## CSP blocks SvelteKit inline scripts

**Problem:** The existing brunnylol security middleware (`src/security.rs`) sets a Content-Security-Policy header with `script-src 'self' https://unpkg.com 'unsafe-eval'` for HTMX. SvelteKit generates inline `<script>` tags which are blocked by this CSP.

**Solution:** The CSP middleware checks `request.uri().path().starts_with("/dashboard")` and applies a different policy for dashboard routes: `script-src 'self' 'unsafe-inline'` instead of the HTMX-oriented policy. Both policies coexist.

## Trailing slash on /dashboard/ returns 404

**Problem:** Axum route `/dashboard` matches exactly, and `/dashboard/{*path}` requires at least one character after the slash. Browsers and reverse proxies often append trailing slashes, so `/dashboard/` was a 404.

**Solution:** Register three routes:
```rust
.route("/dashboard", get(serve_frontend))
.route("/dashboard/", get(serve_frontend))
.route("/dashboard/{*path}", get(serve_frontend))
```

## Duplicate Svelte each key crashes rendering

**Problem:** Using `{#each commands as cmd (cmd.alias)}` crashes with `each_key_duplicate` when there are duplicate aliases (e.g., error commands from conflicting Docker labels). Svelte 5 throws a runtime error and the entire component fails to render — showing zero commands.

**Solution:** Use a composite key that includes the index: `{#each commands as cmd, i (cmd.alias + ':' + i)}`.

## Backend must bind 0.0.0.0 for external access

**Problem:** Binding to `127.0.0.1:3001` makes the backend inaccessible through reverse proxies (e.g., Coder dev environments). The port appears to work intermittently because the proxy can't connect.

**Solution:** Always use `IRON_BUNNY_LISTEN_ADDR=0.0.0.0:3001` in dev mode. The CLAUDE.md and dev instructions have been updated to reflect this.

## create_router() must return AppState for startup rebuild

**Problem:** The original `create_router()` returned only a `Router`, making `AppState` inaccessible in `main()`. Without access to the state, `full_rebuild()` couldn't be called on startup, so Docker labels and config file commands were never loaded. The API returned empty commands.

**Solution:** Changed `create_router()` to return `(Router, Arc<AppState>)`. In `main()`, the returned state is used to:
1. Store the `rebuild_tx` channel sender
2. Run `full_rebuild()` before starting the server
3. Spawn the rebuild coordinator and file/Docker watchers

## Duplicate Docker labels must remove BOTH entries

**Problem:** When `parse_all_containers()` detects a duplicate alias across containers, it originally kept the first container's command in the commands list and only added an error. This caused two entries with the same alias in the registry — one normal, one error — leading to the Svelte duplicate key crash and inconsistent state.

**Solution:** Track duplicate aliases in a `HashSet` and call `all_commands.retain(|cmd| !duplicate_aliases.contains(&cmd.alias))` to remove the first entry too. Neither conflicting container wins — both are removed and only the error command (from `build_registry`) is created.

## OpenSSL build dependency

**Problem:** `cargo build` fails with "Could not find openssl via pkg-config" on fresh systems.

**Solution:** Install build dependencies: `apt-get install -y pkg-config libssl-dev`. The Dockerfile includes this in the builder stage.

## Template API: TemplateParser::parse(), not Template::parse()

**Problem:** The domain template system uses `TemplateParser::parse(template_str)` to create `Template` objects, not `Template::parse()`. To create a literal-only template, use `Template::new(vec![TemplatePart::Literal(s)])`.

**Solution:** Import both `TemplateParser` and `Template`/`TemplatePart` from `crate::domain::template`. Check `src/domain/template/parser.rs` for the actual API.
