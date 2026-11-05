use data::appearance;
#[cfg(not(target_arch = "wasm32"))]
use futures::StreamExt;
use iced::advanced::graphics::futures::{BoxStream, subscription};
use iced::advanced::subscription::Hasher;
use iced::{Subscription, futures};
#[cfg(not(target_arch = "wasm32"))]
use mundy::{ColorScheme, Interest, Preferences};
pub use theme::Theme;

pub mod theme;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Mode {
    Dark,
    Light,
    Unspecified,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<ColorScheme> for Mode {
    fn from(mode: ColorScheme) -> Self {
        match mode {
            ColorScheme::Dark => Mode::Dark,
            ColorScheme::Light => Mode::Light,
            ColorScheme::NoPreference => Mode::Unspecified,
        }
    }
}

impl Mode {
    pub fn theme(
        &self,
        selected: &data::appearance::Selected,
    ) -> data::appearance::Theme {
        match &selected {
            appearance::Selected::Static(theme) => theme.clone(),
            appearance::Selected::Dynamic { light, dark } => match self {
                Self::Dark => dark.clone(),
                Self::Light => light.clone(),
                // We map `Unspecified` to `Light`.
                // This is because Gnome never specifies `Light` and only sends `Unspecified`.
                Self::Unspecified => light.clone(),
            },
        }
    }
}

struct Appearance;

impl subscription::Recipe for Appearance {
    type Output = Mode;

    fn hash(&self, state: &mut Hasher) {
        use std::hash::Hash;
        struct Marker;
        std::any::TypeId::of::<Marker>().hash(state);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn stream(
        self: Box<Self>,
        _input: subscription::EventStream,
    ) -> BoxStream<Mode> {
        Preferences::stream(Interest::ColorScheme)
            .map(|preference| Mode::from(preference.color_scheme))
            .boxed()
    }

    /// Browser: follow `prefers-color-scheme` via matchMedia.
    #[cfg(target_arch = "wasm32")]
    fn stream(
        self: Box<Self>,
        _input: subscription::EventStream,
    ) -> BoxStream<Mode> {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;

        let query = web_sys::window().and_then(|window| {
            window
                .match_media("(prefers-color-scheme: dark)")
                .ok()
                .flatten()
        });
        let Some(query) = query else {
            return iced::advanced::graphics::futures::boxed_stream(
                futures::stream::once(async { Mode::Unspecified }),
            );
        };

        let (sender, receiver) = futures::channel::mpsc::unbounded();
        let mode = |dark: bool| if dark { Mode::Dark } else { Mode::Light };
        let _ = sender.unbounded_send(mode(query.matches()));

        let on_change = Closure::<dyn FnMut(web_sys::MediaQueryListEvent)>::new(
            move |event: web_sys::MediaQueryListEvent| {
                let _ = sender.unbounded_send(mode(event.matches()));
            },
        );
        query.set_onchange(Some(on_change.as_ref().unchecked_ref()));
        // Lives for the page's lifetime, like the subscription itself.
        on_change.forget();

        iced::advanced::graphics::futures::boxed_stream(receiver)
    }
}

pub fn subscription() -> Subscription<Mode> {
    subscription::from_recipe(Appearance)
}
