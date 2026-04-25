use leptos::prelude::*;

pub use ep_core::Tone;

#[component]
pub fn Tag(
    #[prop(default = Tone::None)] tone: Tone,
    #[prop(default = false)] dot: bool,
    children: Children,
) -> impl IntoView {
    let class = format!("tag {}", tone.class());
    view! {
        <span class=class>
            {dot.then(|| view! { <span class="dot"/> })}
            {children()}
        </span>
    }
}
