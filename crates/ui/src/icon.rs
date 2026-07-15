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
        // Icons are decorative: every call site that conveys meaning carries
        // its own accessible name (button/link text or `aria-label`). Hide the
        // SVG from the a11y tree so screen readers don't announce a nameless
        // graphic or include it in the tab order.
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
            aria-hidden="true"
            focusable="false"
            inner_html=path
        ></svg>
    }
}

fn path_for(k: IconKind) -> &'static str {
    match k {
        IconKind::Dashboard => {
            r#"<rect x="2.5" y="2.5" width="5.5" height="6.5" rx="1.2"/><rect x="10" y="2.5" width="5.5" height="4" rx="1.2"/><rect x="2.5" y="11" width="5.5" height="4" rx="1.2"/><rect x="10" y="8" width="5.5" height="7" rx="1.2"/>"#
        }
        IconKind::Finance => {
            r#"<rect x="2.5" y="4.5" width="13" height="9.5" rx="1.3"/><path d="M2.5 8h13"/><circle cx="12" cy="11.5" r="1.1"/>"#
        }
        IconKind::Fitness => {
            r#"<path d="M3 9h2M13 9h2"/><rect x="5" y="6.5" width="2.5" height="5" rx="0.6"/><rect x="10.5" y="6.5" width="2.5" height="5" rx="0.6"/><path d="M7.5 9h3"/>"#
        }
        IconKind::Journal => {
            r#"<path d="M4 2.5h9.5a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-11a1 1 0 0 1 1-1z"/><path d="M6 2.5v13M8.5 6h3.5M8.5 9h3.5M8.5 12h2"/>"#
        }
        IconKind::Settings => {
            r#"<circle cx="9" cy="9" r="2.2"/><path d="M9 1.5v1.8M9 14.7v1.8M1.5 9h1.8M14.7 9h1.8M3.7 3.7l1.3 1.3M13 13l1.3 1.3M3.7 14.3L5 13M13 5l1.3-1.3"/>"#
        }
        IconKind::Bell => {
            r#"<path d="M4.5 12.5V8a4.5 4.5 0 1 1 9 0v4.5l1 1.5H3.5l1-1.5z"/><path d="M7 15.5a2 2 0 0 0 4 0"/>"#
        }
        IconKind::Plus => r#"<path d="M9 3v12M3 9h12"/>"#,
        IconKind::Arrow => r#"<path d="M3 9h12M11 5l4 4-4 4"/>"#,
        IconKind::ArrowUp => r#"<path d="M4 11l5-5 5 5"/>"#,
        IconKind::ArrowDown => r#"<path d="M4 7l5 5 5-5"/>"#,
        IconKind::Flat => r#"<path d="M3 9h12"/>"#,
        IconKind::Check => r#"<path d="M3.5 9l3.5 3.5L14 5.5"/>"#,
        IconKind::Menu => r#"<path d="M2.5 5h13M2.5 9h13M2.5 13h13"/>"#,
        IconKind::Sun => {
            r#"<circle cx="9" cy="9" r="3"/><path d="M9 1.5v1.5M9 15v1.5M1.5 9h1.5M15 9h1.5M3.6 3.6l1.1 1.1M13.3 13.3l1.1 1.1M3.6 14.4l1.1-1.1M13.3 4.7l1.1-1.1"/>"#
        }
        IconKind::Moon => r#"<path d="M14 10.5A6 6 0 0 1 7.5 4a6 6 0 1 0 6.5 6.5z"/>"#,
        IconKind::Upload => r#"<path d="M9 12V3M6 6l3-3 3 3"/><path d="M3 12v2.5h12V12"/>"#,
        IconKind::Export => r#"<path d="M9 3v8M6 6l3-3 3 3"/><path d="M3 13v2h12v-2"/>"#,
        IconKind::Logout => {
            r#"<path d="M9 3H4a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h5"/><path d="M12 6l3 3-3 3"/><path d="M7 9h8"/>"#
        }
        IconKind::Close => r#"<path d="M4 4l10 10M14 4L4 14"/>"#,
        IconKind::Empty => {
            r#"<rect x="3" y="4" width="12" height="11" rx="1.5"/><path d="M6 8h6M6 11h4"/>"#
        }
    }
}
