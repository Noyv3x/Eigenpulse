use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct UnreadSignal(pub RwSignal<u32>);

pub fn provide_unread_signal(initial: u32) -> RwSignal<u32> {
    let s = RwSignal::new(initial);
    provide_context(UnreadSignal(s));
    s
}

pub fn use_unread_signal() -> RwSignal<u32> {
    use_context::<UnreadSignal>()
        .map(|u| u.0)
        .unwrap_or_else(|| RwSignal::new(0))
}

#[component]
pub fn NotificationsBellPopover() -> impl IntoView {
    // MVP: the bell itself lives in `Topbar`; this popover is attached to it via CSS in a later iteration.
    // Render an empty placeholder for now so hydration matches.
    view! { <div class="hidden" data-component="bell-popover" style="display:none"></div> }
}
