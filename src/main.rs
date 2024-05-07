use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use rodio::{
    cpal::{traits::HostTrait, FromSample},
    source::SineWave,
    Decoder, DeviceTrait, OutputStream, OutputStreamHandle, Sample, Sink, Source,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Read},
    marker::PhantomData,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

#[derive(Deserialize, Serialize, Debug)]
struct Config {
    sounds: Vec<SoundConfig>,
    output_devices: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug)]
struct SoundConfig {
    path: PathBuf,
    keybind: KeybindConfig,
}

#[derive(Deserialize, Serialize, Debug)]
struct KeybindConfig {
    modifiers: Modifiers,
    key: Code,
}

pub struct Sound(Arc<Vec<u8>>);

impl AsRef<[u8]> for Sound {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Sound {
    pub fn load(filename: &str) -> std::io::Result<Sound> {
        use std::fs::File;
        let mut buf = Vec::new();
        let mut file = File::open(filename)?;
        file.read_to_end(&mut buf)?;
        Ok(Sound(Arc::new(buf)))
    }
    pub fn cursor(&self) -> Cursor<Sound> {
        Cursor::new(Sound(self.0.clone()))
    }
    pub fn decoder(&self) -> rodio::Decoder<Cursor<Sound>> {
        rodio::Decoder::new_mp3(self.cursor()).unwrap()
    }
}

fn main() {
    let config_dir = dirs::config_dir().unwrap().join("soundboard");

    let config = fs::read_to_string(config_dir.join("config.toml"))
        .expect("Error reading config file, does not exist?");
    let config: Config = toml::from_str(&config).expect("Error parsing config file");

    let host = rodio::cpal::default_host();
    let devices = host
        .output_devices()
        .unwrap()
        .filter(|device| config.output_devices.contains(&device.name().unwrap()));
    let streams = devices.map(|device| OutputStream::try_from_device(&device).unwrap());
    let sinks: Vec<(Sink, OutputStream)> = streams
        .map(|stream| (Sink::try_new(&stream.1).unwrap(), stream.0))
        .collect();

    let hotkeys: Vec<_> = config
        .sounds
        .iter()
        .map(|sound| {
            let file = config_dir.join(&sound.path);
            let source = Sound::load(file.to_str().unwrap()).expect("Error creating sound");
            (
                HotKey::new(Some(sound.keybind.modifiers), sound.keybind.key),
                source,
            )
        })
        .collect();

    hotkey_loop(&hotkeys, &sinks);
}

fn play_sound(sinks: &Vec<(Sink, OutputStream)>, source: &Sound) {
    for (sink, _stream) in sinks {
        sink.stop();
        sink.append(source.decoder());
        sink.play();
    }
}

fn hotkey_loop(hotkeys: &Vec<(HotKey, Sound)>, sinks: &Vec<(Sink, OutputStream)>) -> ! {
    let manager = GlobalHotKeyManager::new().unwrap();

    for hotkey in hotkeys {
        manager.register(hotkey.0).unwrap();
    }

    let sources = hotkeys.iter().map(|(hotkey, source)| (hotkey.id(), source));

    let handler_map = HashMap::<u32, &Sound>::from_iter(sources);

    loop {
        if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state() == HotKeyState::Released {
                continue;
            }

            if let Some(source) = handler_map.get(&event.id) {
                play_sound(sinks, source);
            }
        }
    }
}
