use leptos::{html::Div, prelude::*};
use leptos_router::{
    hooks::{use_navigate, use_params},
    params::Params,
};
use shared::{ClientEvent, MeResponse, ServerEvent};
use std::{cell::Cell, rc::Rc};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{ErrorEvent, Event, MessageEvent, RequestCredentials, WebSocket};

use crate::app::{api_base_url, websocket_base_url};

#[derive(Clone, Params, PartialEq)]
struct ChatParams {
    room: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChatEntry {
    id: String,
    kind: ChatEntryKind,
    author: Option<String>,
    body: String,
    meta: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedChatEntry {
    entry: ChatEntry,
    show_header: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChatEntryKind {
    Info,
    Message,
}

#[component]
pub fn ChatPage() -> impl IntoView {
    let navigate = use_navigate();
    let navigate_for_back = navigate.clone();
    let params = use_params::<ChatParams>();
    let room_id = Memo::new(move |_| {
        params
            .read()
            .as_ref()
            .ok()
            .and_then(|params| params.room.clone())
            .filter(|room| !room.trim().is_empty())
            .unwrap_or_else(|| "lobby".to_string())
    });

    let username = RwSignal::new(Option::<String>::None);
    let session_loading = RwSignal::new(true);
    let error_msg = RwSignal::new(Option::<String>::None);
    let messages = RwSignal::new(Vec::<ChatEntry>::new());
    let draft = RwSignal::new(String::new());
    let socket = RwSignal::new(Option::<WebSocket>::None);
    let messages_container = NodeRef::<Div>::new();
    let rendered_messages = Memo::new(move |_| collapse_messages(&messages.get()));
    let next_local_entry_id = Rc::new(Cell::new(0_u64));

    let navigate_for_auth = navigate.clone();
    Effect::new(move |_| {
        let navigate = navigate_for_auth.clone();

        leptos::task::spawn_local(async move {
            let me_url = format!("{}/me", api_base_url());
            match gloo_net::http::Request::get(&me_url)
                .credentials(RequestCredentials::Include)
                .send()
                .await
            {
                Ok(resp) if resp.status() == 200 => match resp.json::<MeResponse>().await {
                    Ok(data) => {
                        username.set(Some(data.username));
                        error_msg.set(None);
                    }
                    Err(_) => {
                        error_msg.set(Some("Failed to read account details.".into()));
                    }
                },
                Ok(resp) if resp.status() == 401 => {
                    let _ = navigate("/login", Default::default());
                }
                Ok(_) => {
                    error_msg.set(Some("Unexpected session response.".into()));
                }
                Err(_) => {
                    error_msg.set(Some("Network error. Failed to reach server.".into()));
                }
            }

            session_loading.set(false);
        });
    });

    let navigate_for_socket = navigate.clone();
    Effect::new(move |_| {
        if session_loading.get() {
            return;
        }

        let room = room_id.get();
        let Some(_current_user) = username.get() else {
            return;
        };

        messages.update(|items| items.clear());

        let ws_url = websocket_url(&room);
        let ws = match WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(_) => {
                error_msg.set(Some("Failed to create websocket.".into()));
                return;
            }
        };

        let on_open = Closure::<dyn FnMut(Event)>::new(move |_: Event| {
            error_msg.set(None);
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();

        let next_local_entry_id = next_local_entry_id.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            if let Some(text) = event.data().as_string() {
                match serde_json::from_str::<ServerEvent>(&text) {
                    Ok(ServerEvent::NewMessage { message }) => {
                        messages.update(|items| {
                            items.push(ChatEntry {
                                id: message.message_id,
                                kind: ChatEntryKind::Message,
                                author: Some(message.username),
                                body: message.text,
                                meta: Some(format_chat_time(&message.timestamp)),
                            });
                        });
                    }
                    Ok(ServerEvent::UserJoined { username, .. }) => {
                        messages.update(|items| {
                            items.push(ChatEntry {
                                id: next_chat_entry_id(&next_local_entry_id, "join"),
                                kind: ChatEntryKind::Info,
                                author: None,
                                meta: None,
                                body: format!("--- {username} joined ---"),
                            });
                        });
                    }
                    Ok(ServerEvent::UserLeft { username, .. }) => {
                        messages.update(|items| {
                            items.push(ChatEntry {
                                id: next_chat_entry_id(&next_local_entry_id, "leave"),
                                kind: ChatEntryKind::Info,
                                author: None,
                                meta: None,
                                body: format!("{username} left"),
                            });
                        });
                    }
                    Err(_) => {
                        error_msg.set(Some("Received an invalid server event.".into()));
                    }
                }
            }
        });
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        let on_error = Closure::<dyn FnMut(ErrorEvent)>::new(move |_: ErrorEvent| {
            error_msg.set(Some("Realtime connection failed.".into()));
        });
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();

        let navigate_on_close = navigate_for_socket.clone();
        let on_close = Closure::<dyn FnMut(Event)>::new(move |_: Event| {
            socket.set(None);
            if username.get().is_none() {
                let _ = navigate_on_close("/login", Default::default());
            }
        });
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();

        socket.set(Some(ws));
    });

    Effect::new(move |_| {
        let _ = messages.get().len();
        if let Some(container) = messages_container.get() {
            container.set_scroll_top(container.scroll_height());
        }
    });

    let send_message = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();

        let text = draft.get().trim().to_string();
        if text.is_empty() {
            return;
        }

        let payload = match serde_json::to_string(&ClientEvent::SendMessage { text }) {
            Ok(payload) => payload,
            Err(_) => {
                error_msg.set(Some("Failed to prepare message payload.".into()));
                return;
            }
        };

        match socket.get() {
            Some(ws) => {
                if ws.send_with_str(&payload).is_ok() {
                    draft.set(String::new());
                } else {
                    error_msg.set(Some("Failed to send message.".into()));
                }
            }
            None => error_msg.set(Some("Socket is not connected yet.".into())),
        }
    };

    let navigate_for_logout = navigate.clone();
    let logout = move |_| {
        let navigate = navigate_for_logout.clone();
        leptos::task::spawn_local(async move {
            let logout_url = format!("{}/logout", api_base_url());
            let response = gloo_net::http::Request::post(&logout_url)
                .credentials(RequestCredentials::Include)
                .send()
                .await;

            if let Ok(resp) = response {
                if resp.status() == 204 {
                    if let Some(ws) = socket.get() {
                        let _ = ws.close();
                    }
                    let _ = navigate("/login", Default::default());
                    return;
                }
            }

            error_msg.set(Some("Logout failed. Try again.".into()));
        });
    };

    view! {
        <section class="w-full max-w-5xl border border-orange-500/40 bg-surface shadow-2xl">
            <div class="flex flex-col gap-4 border-b border-orange-950 px-5 py-5 sm:flex-row sm:items-center sm:justify-between sm:px-6">
                <h1 class="text-2xl font-semibold uppercase tracking-wide text-orange-50">
                    {move || format!("#{}", room_id.get())}
                </h1>

                <div class="flex items-center gap-3">
                    <p class="hidden text-xs uppercase tracking-wider text-muted sm:block">
                        {move || username.get().unwrap_or_default()}
                    </p>
                    <button
                        class="border border-orange-700 bg-transparent px-3 py-2.5 text-sm font-semibold uppercase tracking-wider text-orange-200 transition hover:border-orange-500 hover:bg-orange-950"
                        on:click=move |_| {
                            let _ = navigate_for_back("/me", Default::default());
                        }
                    >
                        "Account"
                    </button>
                    <button
                        class="border border-orange-500 bg-orange-500 px-3 py-2.5 text-sm font-semibold uppercase tracking-wider text-black transition hover:bg-orange-400"
                        on:click=logout
                    >
                        "Logout"
                    </button>
                </div>
            </div>

            {move || {
                error_msg
                    .get()
                    .map(|message| {
                        view! {
                            <p class="mx-5 mt-5 border border-red-600/50 bg-red-950/40 px-3 py-2 text-sm text-red-200 sm:mx-6">
                                {message}
                            </p>
                        }
                    })
            }}

            <div class="px-5 py-5 sm:px-6">
                <div
                    class="h-96 overflow-y-auto border border-orange-950 bg-surface-strong p-4"
                    node_ref=messages_container
                >
                    <div class="space-y-1">
                        <For
                            each=move || rendered_messages.get()
                            key=|item| item.entry.id.clone()
                            children=move |item| {
                                let class = match item.entry.kind {
                                    ChatEntryKind::Info => {
                                        "flex min-h-8 items-center justify-center text-orange-100/50 text-center"
                                    }
                                    ChatEntryKind::Message => "text-stone-100",
                                };

                                view! {
                                    <article class=format!(
                                        "hover:bg-white/20 transition-color duration-200 px-3 {} ",
                                        class,
                                    )>
                                        {if item.show_header {
                                            view! {
                                                <div class="flex items-baseline justify-between gap-3 mt-2">
                                                    <p class="text-sm font-semibold text-orange-500">
                                                        {item.entry.author.clone().unwrap_or_default()}
                                                    </p>
                                                    <p class="text-xs uppercase tracking-wider text-orange-300/80">
                                                        {item.entry.meta.clone().unwrap_or_default()}
                                                    </p>
                                                </div>
                                            }
                                                .into_any()
                                        } else {
                                            view! {}.into_any()
                                        }} <p class="text-sm">{item.entry.body}</p>
                                    </article>
                                }
                            }
                        />
                    </div>
                </div>

                <form class="mt-5 flex flex-col gap-3 sm:flex-row" on:submit=send_message>
                    <input
                        autofocus
                        class="min-w-0 flex-1 border border-orange-950 bg-surface-strong px-4 py-3 text-sm text-foreground outline-none transition placeholder:text-muted focus:border-orange-400"
                        type="text"
                        placeholder="Send a message to the room"
                        on:input=move |ev| draft.set(event_target_value(&ev))
                        prop:value=draft
                    />
                    <button
                        class="border border-orange-500 bg-orange-500 px-5 py-3 text-sm font-semibold uppercase tracking-wider text-black transition hover:bg-orange-400 disabled:cursor-not-allowed disabled:opacity-60"
                        type="submit"
                        disabled=move || session_loading.get() || socket.get().is_none()
                    >
                        "Send"
                    </button>
                </form>
            </div>
        </section>
    }
}

fn websocket_url(room: &str) -> String {
    format!("{}/{room}", websocket_base_url())
}

fn collapse_messages(entries: &[ChatEntry]) -> Vec<RenderedChatEntry> {
    let mut rendered = Vec::with_capacity(entries.len());
    let mut last_author: Option<String> = None;
    let mut last_meta: Option<String> = None;

    for entry in entries.iter().cloned() {
        let show_header = match entry.kind {
            ChatEntryKind::Info => true,
            ChatEntryKind::Message => {
                let author = entry.author.clone();
                let meta = entry.meta.clone();
                let repeated = author.is_some() && author == last_author && meta == last_meta;
                !repeated
            }
        };

        if entry.kind == ChatEntryKind::Message {
            last_author = entry.author.clone();
            last_meta = entry.meta.clone();
        } else {
            last_author = None;
            last_meta = None;
        }

        rendered.push(RenderedChatEntry { entry, show_header });
    }

    rendered
}

fn format_chat_time(timestamp: &str) -> String {
    timestamp
        .split('T')
        .nth(1)
        .and_then(|time| time.get(..5))
        .map(str::to_string)
        .unwrap_or_else(|| "--:--".to_string())
}

fn next_chat_entry_id(counter: &Cell<u64>, prefix: &str) -> String {
    let id = counter.get();
    counter.set(id + 1);
    format!("{prefix}-{id}")
}
