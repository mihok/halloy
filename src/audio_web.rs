//! Browser audio (web port): play a sound through an <audio> element backed
//! by a Blob URL. Replaces audio.rs (rodio) on wasm32.

use data::audio::Sound;
use wasm_bindgen::JsCast;

/// A playing sound; mirrors the desktop `audio::Handle` (a thread handle).
#[derive(Debug)]
pub struct Handle(Option<web_sys::HtmlAudioElement>);

impl Handle {
    pub fn is_finished(&self) -> bool {
        self.0
            .as_ref()
            .is_none_or(|audio| audio.ended() || audio.paused())
    }
}

pub fn play(sound: Sound) -> Handle {
    match start(&sound) {
        Ok(audio) => Handle(Some(audio)),
        Err(e) => {
            log::error!("Failed to play sound: {e:?}");
            Handle(None)
        }
    }
}

fn start(
    sound: &Sound,
) -> Result<web_sys::HtmlAudioElement, wasm_bindgen::JsValue> {
    let bytes = js_sys::Uint8Array::from(sound.as_ref());
    let parts = js_sys::Array::of1(&bytes);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let audio = web_sys::HtmlAudioElement::new_with_src(&url)?;
    // Free the blob URL once playback ends.
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    audio.set_onended(Some(revoke.unchecked_ref()));
    // Browsers may reject autoplay before the user has interacted with the page.
    let _ = audio.play();
    Ok(audio)
}
