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
