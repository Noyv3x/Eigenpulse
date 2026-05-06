# Open follow-ups

## #26 — settings/* hydration panic (`tachys::hydration::failed_to_cast_text_node`) — resolved

**Status.** Resolved in `app/src/views/settings/{notifications,security}.rs` by
anchoring conditional action feedback in stable wrapper elements
(`error-slot`, `test-notice-slot`, `new-token-slot`). Keep this note as the
regression pattern for future settings forms.

**Symptom.** Navigating in a real browser to `/settings/notifications` or
`/settings/security` after authenticating produces:

```
panicked at tachys-0.1.9/src/hydration.rs:192:9: internal error: entered unreachable code
…  Module.hydrate (eigenpulse.js:1:1705)
```

The page still renders (SSR HTML survives) but every WASM-driven feature
on those pages dies: Tweaks toggle, ActionForm refetch, SSE counter.

**Reproduces on**: `/settings/notifications`, `/settings/security`.
**Does NOT reproduce on**: `/`, `/settings`, `/login` — i.e. routes
*without* `<ActionForm>` + inline conditional `{move || …map(…)}` views.

**Root cause.** `tachys::hydration::failed_to_cast_text_node` (tachys
0.1.9, src/hydration.rs:189-205) fires when the framework expected to
walk into a text node but found an element instead. The two settings
views contain several inline-conditional fragments of the shape

```rust
{move || create.value().get().and_then(|r| r.err()).map(|e| view! {
    <span class="tag rose">{e.to_string()}</span>
})}
```

`Option<HtmlElement<…>>` collapses to *no node* on `None` and an
*element* on `Some`; the SSR placeholder (a comment `<!---->`) and the
hydration walker disagree about which kind of DOM neighbour to find,
typically because some sibling `<form>` produced by `<ActionForm>` is
inserted differently in SSR vs. hydrate.

**Verified non-causes**:
- WASM filename mismatch (separate bug, fixed in commit `6033891`).
- `time::OffsetDateTime::now_utc()` in view code (already moved
  server-side via `is_expired`/`is_revoked` on `PatDto`).
- `ChannelDto` config_json leak (separate bug, fixed in `f3a31b9`).

### Candidate fixes (ranked by cost)

1. **Replace `Option<view!>` with `<Show when=…>` + always-present
   wrapper.** Wrap each `{move || option.map(view!{…})}` in a stable
   element that exists in both SSR and hydrate trees:

   ```rust
   <span class="form-error-slot">
       {move || create.value().get().and_then(|r| r.err()).map(|e| view! { <span>…</span> })}
   </span>
   ```

   Affects ~3 sites in `notifications.rs` + ~2 in `security.rs`.

2. **Switch the matched-arm view returns from `.into_any()` to
   `EitherOf3`/`EitherOf4`.** `.into_any()` type-erases each branch
   independently; the `Either*` enums tell the framework the branches
   share a slot.

3. **Switch the entire two settings views from `<ActionForm>` to plain
   `<form method="post" action="/api/_internal/cfg/{op}">` + server fns
   that issue 303 redirects on success.** No reactive layer, no
   hydration-time interaction with these pages — the form is purely
   server-rendered. This loses optimistic UI + inline error display
   but is bulletproof. Cheapest engineering, regressing functionality.

4. **Upgrade Leptos.** `leptos = 0.7.8` is current latest patch on
   crates.io. Leptos 0.8 likely improves hydration but is a breaking
   bump (pre-0.8 → 0.8 needs `view!` updates and possibly Resource API
   changes). Out of scope for a maintenance fix.

### Recommended path

Option 1 first (cheapest, surgical), then Option 2 if 1 doesn't fully
silence the panic. Each iteration takes ~60s rebuild + Playwright test.

### Reproduction

```bash
EP_ADMIN_PASSWORD=dev-pw EP_SECRET="$(openssl rand -hex 64)" \
LEPTOS_OUTPUT_NAME=eigenpulse LEPTOS_SITE_ROOT=target/site \
LEPTOS_SITE_PKG_DIR=pkg LEPTOS_SITE_ADDR=127.0.0.1:3041 \
DATABASE_URL=sqlite:///tmp/x.db?mode=rwc \
./target/release/eigenpulse &
# Then in a real browser, login → /settings/security → DevTools console.
```
