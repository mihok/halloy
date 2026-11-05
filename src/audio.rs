use std::io::Cursor;
use std::sync::Arc;
use std::thread;

use data::audio::Sound;
use rodio::{Decoder, DeviceSinkBuilder, Player};

pub type Handle = thread::JoinHandle<()>;

pub fn play(sound: Sound) -> Handle {
    thread::spawn(move || {
        if let Err(e) = _play(sound) {
            log::error!("Failed to play sound: {e}");
        }
    })
}

fn _play(sound: Sound) -> Result<(), PlayError> {
    let mut sink_handle = DeviceSinkBuilder::open_default_sink()?;
    sink_handle.log_on_drop(false);
    let player = Player::connect_new(sink_handle.mixer());

    let source = Decoder::new(Cursor::new(sound))?;

    player.append(source);

    player.sleep_until_end();

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum PlayError {
    #[error(transparent)]
    Decoding(Arc<rodio::decoder::DecoderError>),
    #[error(transparent)]
    Playing(Arc<rodio::PlayError>),
    #[error(transparent)]
    SinkInitialization(Arc<rodio::DeviceSinkError>),
}

impl From<rodio::decoder::DecoderError> for PlayError {
    fn from(error: rodio::decoder::DecoderError) -> Self {
        Self::Decoding(Arc::new(error))
    }
}

impl From<rodio::PlayError> for PlayError {
    fn from(error: rodio::PlayError) -> Self {
        Self::Playing(Arc::new(error))
    }
}

impl From<rodio::DeviceSinkError> for PlayError {
    fn from(error: rodio::DeviceSinkError) -> Self {
        Self::SinkInitialization(Arc::new(error))
    }
}
