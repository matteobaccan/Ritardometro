//! Implementazione Windows: cattura con `xcap` + OCR con `Windows.Media.Ocr`.
//! L'OCR è integrato in Windows 10/11: niente Tesseract da installare.

use super::{interpreta, Messaggio};
use crate::config::OcrConfig;
use image::{imageops::FilterType, DynamicImage, GrayImage};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use xcap::Monitor;

/// Lato massimo dell'immagine passata all'OCR (Windows accetta fino a ~2600 px).
const LATO_MASSIMO: u32 = 2000;

pub fn avvia(cfg: OcrConfig, ctx: eframe::egui::Context) -> Result<Receiver<Messaggio>, String> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("ritardometro-ocr".into())
        .spawn(move || ciclo(cfg, tx, ctx))
        .map_err(|e| format!("Non riesco ad avviare il thread OCR: {e}"))?;
    Ok(rx)
}

fn ciclo(cfg: OcrConfig, tx: Sender<Messaggio>, ctx: eframe::egui::Context) {
    // Il thread deve entrare in un apartment WinRT prima di usare l'OCR.
    // SAFETY: chiamata una sola volta, all'inizio del thread.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };

    let motore = match OcrEngine::TryCreateFromUserProfileLanguages() {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(Messaggio::Errore(format!(
                "OCR di Windows non disponibile ({e}). Installa una lingua con il riconoscimento ottico in Impostazioni > Lingua"
            )));
            ctx.request_repaint();
            return;
        }
    };

    let percorso_debug = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ritardometro-debug.png")));

    loop {
        let msg = match leggi(&motore, &cfg, percorso_debug.as_deref()) {
            Ok(testo) => match interpreta(&testo) {
                Some(l) => Messaggio::Lettura(l),
                None => Messaggio::Niente(testo),
            },
            Err(e) => Messaggio::Errore(e),
        };
        if tx.send(msg).is_err() {
            return; // l'interfaccia è stata chiusa
        }
        ctx.request_repaint();
        std::thread::sleep(Duration::from_millis(cfg.intervallo_ms));
    }
}

fn leggi(
    motore: &OcrEngine,
    cfg: &OcrConfig,
    debug: Option<&std::path::Path>,
) -> Result<String, String> {
    let immagine = cattura(cfg)?;
    let immagine = prepara(immagine, cfg.ingrandimento);
    if cfg.salva_debug {
        if let Some(p) = debug {
            let _ = immagine.save(p);
        }
    }
    riconosci(motore, &immagine)
}

fn scegli_monitor(indice: usize) -> Result<Monitor, String> {
    let monitor = Monitor::all().map_err(|e| format!("Monitor non trovati: {e}"))?;
    if indice == 0 {
        monitor
            .iter()
            .find(|m| m.is_primary().unwrap_or(false))
            .or_else(|| monitor.first())
            .cloned()
            .ok_or_else(|| "Nessun monitor trovato".into())
    } else {
        let n = monitor.len();
        monitor
            .into_iter()
            .nth(indice - 1)
            .ok_or_else(|| format!("Monitor {indice} non esiste (ce ne sono {n})"))
    }
}

fn cattura(cfg: &OcrConfig) -> Result<image::RgbaImage, String> {
    let m = scegli_monitor(cfg.monitor)?;
    let w = m.width().map_err(|e| e.to_string())?;
    let h = m.height().map_err(|e| e.to_string())?;
    let x = ((cfg.x * w as f32) as u32).min(w.saturating_sub(1));
    let y = ((cfg.y * h as f32) as u32).min(h.saturating_sub(1));
    let rw = ((cfg.larghezza * w as f32) as u32).clamp(1, w - x);
    let rh = ((cfg.altezza * h as f32) as u32).clamp(1, h - y);
    m.capture_region(x, y, rw, rh)
        .map_err(|e| format!("Cattura schermo fallita: {e}"))
}

/// Scala di grigi + ingrandimento: l'OCR legge molto meglio i numeri piccoli.
fn prepara(img: image::RgbaImage, ingrandimento: u32) -> GrayImage {
    let grigio = DynamicImage::ImageRgba8(img).into_luma8();
    let (w, h) = grigio.dimensions();
    let lato = w.max(h).max(1);
    let fattore = ingrandimento.max(1).min((LATO_MASSIMO / lato).max(1));
    if fattore <= 1 {
        return grigio;
    }
    image::imageops::resize(&grigio, w * fattore, h * fattore, FilterType::CatmullRom)
}

fn riconosci(motore: &OcrEngine, img: &GrayImage) -> Result<String, String> {
    let (w, h) = img.dimensions();
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for p in img.pixels() {
        let v = p.0[0];
        bgra.extend_from_slice(&[v, v, v, 255]);
    }

    let ocr = || -> windows::core::Result<String> {
        let writer = DataWriter::new()?;
        writer.WriteBytes(&bgra)?;
        let buffer = writer.DetachBuffer()?;
        let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            w as i32,
            h as i32,
        )?;
        let risultato = motore.RecognizeAsync(&bitmap)?.join()?;
        Ok(risultato.Text()?.to_string_lossy())
    };
    ocr().map_err(|e| format!("OCR fallito: {e}"))
}
