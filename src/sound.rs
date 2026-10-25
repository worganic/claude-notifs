//! Son d'alerte : le son système « Notification » de Windows (réglable dans le panneau Sons).

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ALIAS, SND_ASYNC, SND_NOWAIT};

pub fn play_alert() {
    let alias: Vec<u16> = "Notification.Default".encode_utf16().chain(Some(0)).collect();
    // Sans SND_NODEFAULT : si l'alias n'existe pas, Windows joue le son par défaut.
    unsafe {
        PlaySoundW(alias.as_ptr(), std::ptr::null_mut(), SND_ALIAS | SND_ASYNC | SND_NOWAIT);
    }
}
