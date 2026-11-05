//! Browser network state (web port): follows the window's online/offline
//! events. Replaces network.rs (netwatcher) on wasm32.

use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use hashbrown::HashSet;
use iced::Subscription;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use super::Event;

pub fn events() -> Subscription<Event> {
    Subscription::run(|| {
        iced::stream::channel(10, async |mut output| {
            let Some(window) = web_sys::window() else {
                return;
            };
            let (sender, mut receiver) =
                futures::channel::mpsc::unbounded::<bool>();

            let on_online = {
                let sender = sender.clone();
                Closure::<dyn FnMut()>::new(move || {
                    let _ = sender.unbounded_send(true);
                })
            };
            let on_offline = Closure::<dyn FnMut()>::new(move || {
                let _ = sender.unbounded_send(false);
            });
            window.set_ononline(Some(on_online.as_ref().unchecked_ref()));
            window.set_onoffline(Some(on_offline.as_ref().unchecked_ref()));

            // Streams assume a routable network until told otherwise.
            if !window.navigator().on_line() {
                let _ = output
                    .send(Event::NetworkChanged {
                        routable: false,
                        addresses: Arc::new(HashSet::new()),
                    })
                    .await;
            }

            while let Some(online) = receiver.next().await {
                log::info!(
                    "network {}",
                    if online { "online" } else { "offline" }
                );
                let event = Event::NetworkChanged {
                    routable: online,
                    addresses: Arc::new(HashSet::new()),
                };
                if output.send(event).await.is_err() {
                    break;
                }
            }

            window.set_ononline(None);
            window.set_onoffline(None);
            drop((on_online, on_offline));
        })
    })
}
