#[cfg(not(target_arch = "wasm32"))]
use iced::advanced::graphics::futures::BoxStream;
#[cfg(not(target_arch = "wasm32"))]
use iced::advanced::subscription::{self, Hasher};
use iced::{self, Subscription};

#[cfg(target_os = "macos")]
pub fn listen() -> Subscription<String> {
    use futures::stream::StreamExt;
    use iced::advanced::graphics::futures::subscription::{
        Event, MacOS, PlatformSpecific,
    };

    struct OnUrl;

    impl subscription::Recipe for OnUrl {
        type Output = String;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash;

            struct Marker;
            std::any::TypeId::of::<Marker>().hash(state);
        }

        fn stream(
            self: Box<Self>,
            input: subscription::EventStream,
        ) -> BoxStream<Self::Output> {
            input
                .filter_map(move |event| {
                    if let Event::Interaction { status, .. } = &event
                        && *status == iced::event::Status::Captured
                    {
                        return futures::future::ready(None);
                    }

                    let result = match event {
                        Event::PlatformSpecific(event) => match event {
                            PlatformSpecific::MacOS(macos) => match macos {
                                MacOS::ReceivedUrl(url) => Some(url),
                            },
                        },
                        _ => None,
                    };

                    futures::future::ready(result)
                })
                .boxed()
        }
    }

    subscription::from_recipe(OnUrl)
}

/// Browser: no OS-level URL handler; deep links arrive with the page.
#[cfg(target_arch = "wasm32")]
pub fn listen() -> Subscription<String> {
    Subscription::none()
}

#[cfg(all(not(target_os = "macos"), not(target_arch = "wasm32")))]
pub fn listen() -> Subscription<String> {
    struct Listener;

    impl subscription::Recipe for Listener {
        type Output = String;

        fn hash(&self, state: &mut Hasher) {
            use std::hash::Hash;

            struct Marker;
            std::any::TypeId::of::<Marker>().hash(state);
        }

        fn stream(
            self: Box<Self>,
            _input: subscription::EventStream,
        ) -> BoxStream<Self::Output> {
            ipc::listen()
        }
    }

    subscription::from_recipe(Listener)
}
