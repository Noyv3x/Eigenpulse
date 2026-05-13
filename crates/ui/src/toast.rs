use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Danger,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub kind: ToastKind,
}

#[derive(Clone, Copy)]
pub struct ToastStack {
    pub items: RwSignal<Vec<Toast>>,
    next_id: RwSignal<u64>,
}

impl ToastStack {
    pub fn push(&self, message: impl Into<String>, kind: ToastKind) {
        let id = {
            let mut next = self.next_id.get_untracked();
            next = next.wrapping_add(1);
            self.next_id.set(next);
            next
        };
        let message = message.into();
        self.items.update(|v| v.push(Toast { id, message, kind }));
        #[cfg(feature = "hydrate")]
        {
            // The closure owns itself via an `Rc<RefCell<Option<…>>>`: the
            // setTimeout fires once, the body retains the matching toast id
            // *and* drops the Closure handle, freeing the `Box<dyn FnMut>`.
            // Without this self-drop the previous `cb.forget()` leaked the
            // boxed callback permanently for every toast pushed.
            use std::cell::RefCell;
            use std::rc::Rc;
            use wasm_bindgen::closure::Closure;
            use wasm_bindgen::JsCast;
            type Slot = Rc<RefCell<Option<Closure<dyn FnMut()>>>>;
            let items = self.items;
            let holder: Slot = Rc::new(RefCell::new(None));
            let holder_clone = holder.clone();
            let cb = Closure::wrap(Box::new(move || {
                items.update(|v| v.retain(|t| t.id != id));
                holder_clone.borrow_mut().take();
            }) as Box<dyn FnMut()>);
            if let Some(win) = web_sys::window() {
                let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.as_ref().unchecked_ref(),
                    4_000,
                );
            }
            *holder.borrow_mut() = Some(cb);
        }
    }

    pub fn info(&self, message: impl Into<String>) {
        self.push(message, ToastKind::Info);
    }
    pub fn success(&self, message: impl Into<String>) {
        self.push(message, ToastKind::Success);
    }
    pub fn danger(&self, message: impl Into<String>) {
        self.push(message, ToastKind::Danger);
    }
    pub fn dismiss(&self, id: u64) {
        self.items.update(|v| v.retain(|t| t.id != id));
    }
}

pub fn provide_toast_stack() -> ToastStack {
    let stack = ToastStack {
        items: RwSignal::new(Vec::new()),
        next_id: RwSignal::new(0),
    };
    provide_context(stack);
    stack
}

pub fn use_toast() -> ToastStack {
    use_context::<ToastStack>().unwrap_or_else(|| ToastStack {
        items: RwSignal::new(Vec::new()),
        next_id: RwSignal::new(0),
    })
}

#[component]
pub fn ToastViewport() -> impl IntoView {
    let stack = use_toast();
    let items = stack.items;
    view! {
        <div class="toast-stack" aria-live="polite">
            <For
                each=move || items.get()
                key=|t: &Toast| t.id
                children=move |t: Toast| {
                    let cls = match t.kind {
                        ToastKind::Success => "toast success",
                        ToastKind::Danger => "toast danger",
                        ToastKind::Info => "toast",
                    };
                    let id = t.id;
                    view! {
                        <div class=cls role="status">
                            <span>{t.message}</span>
                            <button class="toast-close" type="button"
                                    aria-label="dismiss"
                                    on:click=move |_| stack.dismiss(id)>"×"</button>
                        </div>
                    }
                }
            />
        </div>
    }
}
