# Resolved follow-up archive

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

### Chosen Fix

The shipped fix is the stable-wrapper pattern: wrap each conditional
`{move || option.map(view!{…})}` sibling in an element that exists in both
SSR and hydrate trees:

   ```rust
   <span class="form-error-slot">
       {move || create.value().get().and_then(|r| r.err()).map(|e| view! { <span>…</span> })}
   </span>
   ```

This is implemented in `app/src/views/settings/{notifications,security}.rs`
using `error-slot`, `test-notice-slot`, and `new-token-slot`.

### Reproduction

```bash
EP_ADMIN_PASSWORD=dev-pw EP_SECRET="$(openssl rand -hex 64)" \
LEPTOS_OUTPUT_NAME=eigenpulse LEPTOS_SITE_ROOT=target/site \
LEPTOS_SITE_PKG_DIR=pkg LEPTOS_SITE_ADDR=127.0.0.1:3041 \
DATABASE_URL=sqlite:///tmp/x.db?mode=rwc \
./target/release/eigenpulse &
# Then in a real browser, login → /settings/security → DevTools console.
```
