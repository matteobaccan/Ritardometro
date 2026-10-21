//! Ritardometro: il timer per talk che ti dice se sei in anticipo o in ritardo.
//!
//! Uso:
//!   ritardometro.exe                       usa ritardometro.toml accanto all'exe
//!   ritardometro.exe mio-talk.toml         usa un file di configurazione specifico
//!   ritardometro.exe --minuti 25 --slide 28   sovrascrive durata e numero di slide
//!   ritardometro.exe --demo                simula un talk a velocità 30x

// Niente finestra della console su Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod avatar;
mod config;
mod ocr;
mod pacing;
mod schermo;

use eframe::egui;
use std::path::PathBuf;

struct Argomenti {
    config: Option<PathBuf>,
    minuti: Option<f32>,
    slide: Option<u32>,
    demo: bool,
}

fn argomenti() -> Result<Argomenti, String> {
    let mut a = Argomenti {
        config: None,
        minuti: None,
        slide: None,
        demo: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-m" | "--minuti" | "--minutes" => {
                let v = it.next().ok_or("missing value after --minutes")?;
                a.minuti = Some(
                    v.replace(',', ".")
                        .parse()
                        .map_err(|_| format!("invalid minutes: {v}"))?,
                );
            }
            "-s" | "--slide" | "--slides" => {
                let v = it.next().ok_or("missing value after --slides")?;
                a.slide = Some(
                    v.parse()
                        .map_err(|_| format!("invalid slide number: {v}"))?,
                );
            }
            "--demo" => a.demo = true,
            "-h" | "--help" => {
                return Err(
                    "usage: ritardometro [config.toml] [--minutes N] [--slides N] [--demo]".into(),
                );
            }
            altro if !altro.starts_with('-') => a.config = Some(PathBuf::from(altro)),
            altro => return Err(format!("unknown option: {altro}")),
        }
    }
    Ok(a)
}

/// Icona della finestra: la testa della caricatura.
///
/// Si ritaglia dall'immagine che l'avatar usa già, così non c'è un secondo
/// file da tenere allineato. Le frazioni inquadrano la testa con un filo
/// d'aria sopra: a 16 pixel nella barra delle applicazioni si riconosce
/// ancora, cosa che un mezzobusto intero non farebbe.
fn icona_finestra() -> Option<egui::IconData> {
    /// Centro del ritaglio e suo lato, in frazioni dell'immagine.
    const CENTRO_X: f32 = 0.504;
    const CENTRO_Y: f32 = 0.295;
    const LATO: f32 = 0.80;
    const PIXEL: u32 = 256;

    let immagine = image::load_from_memory(avatar::PNG_CARICATURA)
        .ok()?
        .into_rgba8();
    let (larghezza, altezza) = immagine.dimensions();
    let lato = ((LATO * larghezza as f32) as u32)
        .min(larghezza)
        .min(altezza)
        .max(1);
    let x = (((CENTRO_X * larghezza as f32) as i64) - lato as i64 / 2)
        .clamp(0, (larghezza - lato) as i64) as u32;
    let y = (((CENTRO_Y * altezza as f32) as i64) - lato as i64 / 2)
        .clamp(0, (altezza - lato) as i64) as u32;

    let testa = image::imageops::crop_imm(&immagine, x, y, lato, lato).to_image();
    let icona = image::imageops::resize(
        &testa,
        PIXEL,
        PIXEL,
        image::imageops::FilterType::CatmullRom,
    );
    Some(egui::IconData {
        rgba: icona.into_raw(),
        width: PIXEL,
        height: PIXEL,
    })
}

fn main() -> eframe::Result {
    let (args, errore_args) = match argomenti() {
        Ok(a) => (a, None),
        Err(e) => (
            Argomenti {
                config: None,
                minuti: None,
                slide: None,
                demo: false,
            },
            Some(e),
        ),
    };

    let percorso = args.config.clone().unwrap_or_else(config::percorso_default);
    let mut caricata = config::carica(&percorso);
    if let Some(m) = args.minuti {
        caricata.config.duration_minutes = m.max(1.0);
    }
    if let Some(s) = args.slide {
        caricata.config.total_slides = s;
    }
    if let Some(e) = errore_args {
        caricata.avviso = Some(e);
    }

    let demo = args.demo;
    let scala = caricata.config.scale;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Ritardometro")
        .with_inner_size([app::LARGHEZZA * scala, app::ALTEZZA * scala])
        .with_resizable(false)
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_taskbar(true);
    if let Some(icona) = icona_finestra() {
        viewport = viewport.with_icon(icona);
    }

    let opzioni = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Ritardometro",
        opzioni,
        Box::new(move |cc| Ok(Box::new(app::Ritardometro::new(cc, caricata, demo)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_icona_esce_quadrata_e_piena() {
        let icona = icona_finestra().expect("la caricatura deve essere decodificabile");
        assert_eq!((icona.width, icona.height), (256, 256));
        assert_eq!(icona.rgba.len(), 256 * 256 * 4);
        // il ritaglio prende la testa, non un angolo vuoto: in mezzo c'è roba opaca
        let centro = (256 * 128 + 128) * 4;
        assert_eq!(icona.rgba[centro + 3], 255, "il centro dell'icona è trasparente");
    }
}
