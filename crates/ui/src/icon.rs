use ep_core::IconKind;
use leptos::prelude::*;

#[component]
pub fn Icon(
    #[prop(into)] kind: IconKind,
    #[prop(default = 18)] size: u32,
    #[prop(default = 1.6)] stroke: f32,
) -> impl IntoView {
    let s = size;
    let sw = stroke;
    let path = path_for(kind);
    view! {
        <svg
            width=s
            height=s
            viewBox="0 0 18 18"
            fill="none"
            stroke="currentColor"
            stroke-width=sw
            stroke-linecap="round"
            stroke-linejoin="round"
            class="ep-icon"
            inner_html=path
        ></svg>
    }
}

fn path_for(k: IconKind) -> &'static str {
    match k {
        IconKind::Dashboard => {
            r#"<rect x="2.5" y="2.5" width="5.5" height="6.5" rx="1.2"/><rect x="10" y="2.5" width="5.5" height="4" rx="1.2"/><rect x="2.5" y="11" width="5.5" height="4" rx="1.2"/><rect x="10" y="8" width="5.5" height="7" rx="1.2"/>"#
        }
        IconKind::Today => {
            r#"<rect x="2.5" y="3.5" width="13" height="12" rx="1.5"/><path d="M2.5 7h13"/><path d="M6 2v3M12 2v3"/><circle cx="9" cy="11" r="1.4" fill="currentColor" stroke="none"/>"#
        }
        IconKind::Finance => {
            r#"<rect x="2.5" y="4.5" width="13" height="9.5" rx="1.3"/><path d="M2.5 8h13"/><circle cx="12" cy="11.5" r="1.1"/>"#
        }
        IconKind::Fitness => {
            r#"<path d="M3 9h2M13 9h2"/><rect x="5" y="6.5" width="2.5" height="5" rx="0.6"/><rect x="10.5" y="6.5" width="2.5" height="5" rx="0.6"/><path d="M7.5 9h3"/>"#
        }
        IconKind::Learning => {
            r#"<path d="M2.5 5.5L9 2.5l6.5 3L9 8.5 2.5 5.5z"/><path d="M5 7.2v4.3c0 1.3 1.8 2.5 4 2.5s4-1.2 4-2.5V7.2"/><path d="M15.5 5.5v5"/>"#
        }
        IconKind::Modules => {
            r#"<rect x="2.5" y="2.5" width="5.5" height="5.5" rx="1"/><rect x="10" y="2.5" width="5.5" height="5.5" rx="1"/><rect x="2.5" y="10" width="5.5" height="5.5" rx="1"/><path d="M12.75 10v5.5M10 12.75h5.5"/>"#
        }
        IconKind::Reports => r#"<path d="M3 15V6M7 15V3M11 15V9M15 15v-4"/>"#,
        IconKind::Settings => {
            r#"<circle cx="9" cy="9" r="2.2"/><path d="M9 1.5v1.8M9 14.7v1.8M1.5 9h1.8M14.7 9h1.8M3.7 3.7l1.3 1.3M13 13l1.3 1.3M3.7 14.3L5 13M13 5l1.3-1.3"/>"#
        }
        IconKind::Help => {
            r#"<circle cx="9" cy="9" r="6.5"/><path d="M7 7c0-1.2 1-2 2-2s2 .8 2 2c0 1.2-2 1.5-2 3"/><circle cx="9" cy="13" r="0.5" fill="currentColor"/>"#
        }
        IconKind::Search => r#"<circle cx="8" cy="8" r="5"/><path d="M12 12l3 3"/>"#,
        IconKind::Bell => {
            r#"<path d="M4.5 12.5V8a4.5 4.5 0 1 1 9 0v4.5l1 1.5H3.5l1-1.5z"/><path d="M7 15.5a2 2 0 0 0 4 0"/>"#
        }
        IconKind::Plus => r#"<path d="M9 3v12M3 9h12"/>"#,
        IconKind::Arrow => r#"<path d="M3 9h12M11 5l4 4-4 4"/>"#,
        IconKind::ArrowUp => r#"<path d="M4 11l5-5 5 5"/>"#,
        IconKind::ArrowDown => r#"<path d="M4 7l5 5 5-5"/>"#,
        IconKind::Flat => r#"<path d="M3 9h12"/>"#,
        IconKind::Check => r#"<path d="M3.5 9l3.5 3.5L14 5.5"/>"#,
        IconKind::More => {
            r#"<circle cx="4" cy="9" r="1" fill="currentColor" stroke="none"/><circle cx="9" cy="9" r="1" fill="currentColor" stroke="none"/><circle cx="14" cy="9" r="1" fill="currentColor" stroke="none"/>"#
        }
        IconKind::Menu => r#"<path d="M2.5 5h13M2.5 9h13M2.5 13h13"/>"#,
        IconKind::Chevron => r#"<path d="M7 4l5 5-5 5"/>"#,
        IconKind::Filter => r#"<path d="M2.5 3.5h13l-5 6v5l-3-1.5v-3.5l-5-6z"/>"#,
        IconKind::Sun => {
            r#"<circle cx="9" cy="9" r="3"/><path d="M9 1.5v1.5M9 15v1.5M1.5 9h1.5M15 9h1.5M3.6 3.6l1.1 1.1M13.3 13.3l1.1 1.1M3.6 14.4l1.1-1.1M13.3 4.7l1.1-1.1"/>"#
        }
        IconKind::Moon => r#"<path d="M14 10.5A6 6 0 0 1 7.5 4a6 6 0 1 0 6.5 6.5z"/>"#,
        IconKind::Tag => {
            r#"<path d="M9 2.5H3.5a1 1 0 0 0-1 1V9l7 7 7-7-7-7z"/><circle cx="6" cy="6" r="0.7" fill="currentColor"/>"#
        }
        IconKind::Link => {
            r#"<path d="M7 11l4-4"/><path d="M10 4.5l1-1a3 3 0 1 1 4.2 4.2l-1 1"/><path d="M8 13.5l-1 1a3 3 0 1 1-4.2-4.2l1-1"/>"#
        }
        IconKind::Sparkle => {
            r#"<path d="M9 2v3M9 13v3M2 9h3M13 9h3M4.5 4.5L6.5 6.5M11.5 11.5l2 2M4.5 13.5L6.5 11.5M11.5 6.5l2-2"/>"#
        }
        IconKind::Upload => r#"<path d="M9 12V3M6 6l3-3 3 3"/><path d="M3 12v2.5h12V12"/>"#,
        IconKind::Flame => {
            r#"<path d="M9 2c0 2.5-2 3-2 5.5 0 1 .5 2 1.5 2.5C8 9 7.5 8 8 7c.5 1 2 2 2 4a2.5 2.5 0 1 1-5 0C5 8.5 7 7 9 2z"/>"#
        }
        IconKind::Book => {
            r#"<path d="M3 3h5.5A1.5 1.5 0 0 1 10 4.5V15l-1.5-1H3V3z"/><path d="M15 3h-5.5A1.5 1.5 0 0 0 8 4.5V15l1.5-1H15V3z"/>"#
        }
        IconKind::Dumbbell => {
            r#"<rect x="2" y="7" width="1.5" height="4" rx="0.4"/><rect x="14.5" y="7" width="1.5" height="4" rx="0.4"/><rect x="4" y="6" width="1.5" height="6" rx="0.4"/><rect x="12.5" y="6" width="1.5" height="6" rx="0.4"/><path d="M5.5 9h7"/>"#
        }
        IconKind::Heart => {
            r#"<path d="M9 14.5S2.5 11 2.5 6.5A3 3 0 0 1 9 5.2a3 3 0 0 1 6.5 1.3C15.5 11 9 14.5 9 14.5z"/>"#
        }
        IconKind::Coin => {
            r#"<circle cx="9" cy="9" r="6.5"/><path d="M9 5v8M7 7h3a1.5 1.5 0 0 1 0 3h-3M7 10h3a1.5 1.5 0 0 1 0 3h-3"/>"#
        }
        IconKind::Grid => {
            r#"<rect x="2.5" y="2.5" width="5" height="5" rx="0.8"/><rect x="10.5" y="2.5" width="5" height="5" rx="0.8"/><rect x="2.5" y="10.5" width="5" height="5" rx="0.8"/><rect x="10.5" y="10.5" width="5" height="5" rx="0.8"/>"#
        }
        IconKind::Export => r#"<path d="M9 3v8M6 6l3-3 3 3"/><path d="M3 13v2h12v-2"/>"#,
        IconKind::Cube => {
            r#"<path d="M9 2.5L15 5.5v7L9 15.5 3 12.5v-7z"/><path d="M3 5.5l6 3 6-3M9 8.5v7"/>"#
        }
    }
}
