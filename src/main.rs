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
            "-m" | "--minuti" => {
                let v = it.next().ok_or("manca il valore dopo --minuti")?;
                a.minuti = Some(
                    v.replace(',', ".")
                        .parse()
                        .map_err(|_| format!("minuti non validi: {v}"))?,
                );
            }
            "-s" | "--slide" => {
                let v = it.next().ok_or("manca il valore dopo --slide")?;
                a.slide = Some(
                    v.parse()
                        .map_err(|_| format!("numero di slide non valido: {v}"))?,
                );
            }
            "--demo" => a.demo = true,
            "-h" | "--help" => {
                return Err(
                    "uso: ritardometro [config.toml] [--minuti N] [--slide N] [--demo]".into(),
                );
            }
            altro if !altro.starts_with('-') => a.config = Some(PathBuf::from(altro)),
            altro => return Err(format!("opzione sconosciuta: {altro}")),
        }
    }
    Ok(a)
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
        caricata.config.durata_minuti = m.max(1.0);
    }
    if let Some(s) = args.slide {
        caricata.config.totale_slide = s;
    }
    if let Some(e) = errore_args {
        caricata.avviso = Some(e);
    }

    let demo = args.demo;
    let scala = caricata.config.scala;
    let opzioni = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Ritardometro")
            .with_inner_size([app::LARGHEZZA * scala, app::ALTEZZA * scala])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(true),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Ritardometro",
        opzioni,
        Box::new(move |cc| Ok(Box::new(app::Ritardometro::new(cc, caricata, demo)))),
    )
}
