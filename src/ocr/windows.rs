//! Implementazione Windows: cattura con `xcap` + OCR con `Windows.Media.Ocr`.
//! L'OCR è integrato in Windows 10/11: niente Tesseract da installare.

use super::{con_zona, interpreta, Esito, Messaggio, Scansione, StatoOcrCondiviso, Zona};
use crate::config::OcrConfig;
use crate::schermo::scegli_monitor;
use xcap::Monitor;
use image::{imageops::FilterType, DynamicImage, GrayImage, Luma};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

/// Lato massimo dell'immagine passata all'OCR: oltre ~900 px di larghezza su un ritaglio
/// del 15% (es. su monitor 4K/HiDPI con ingrandimento 3x -> 1725 px), i caratteri diventano
/// troppo grandi (>80 px) e `Windows.Media.Ocr` smette di riconoscerli restituendo stringa vuota.
const LATO_MASSIMO: u32 = 900;

pub fn avvia(
    condiviso: Arc<Mutex<StatoOcrCondiviso>>,
    ctx: eframe::egui::Context,
) -> Result<Receiver<Messaggio>, String> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("ritardometro-ocr".into())
        .spawn(move || ciclo(condiviso, tx, ctx))
        .map_err(|e| format!("Non riesco ad avviare il thread OCR: {e}"))?;
    Ok(rx)
}

fn ciclo(
    condiviso: Arc<Mutex<StatoOcrCondiviso>>,
    tx: Sender<Messaggio>,
    ctx: eframe::egui::Context,
) {
    // Il thread deve entrare in un apartment WinRT prima di usare l'OCR.
    // SAFETY: chiamata una sola volta, all'inizio del thread.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };

    let motore = match OcrEngine::TryCreateFromUserProfileLanguages() {
        Ok(m) => m,
        Err(e) => {
            let _ = tx.send(Messaggio::Errore(format!(
                "Windows OCR not available ({e}). Please install a language with OCR in Settings > Time & Language > Language"
            )));
            ctx.request_repaint();
            return;
        }
    };

    let percorso_debug = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ritardometro-debug.png")));

    let leggi_stato = || {
        condiviso.lock().map(|s| s.clone()).unwrap_or(StatoOcrCondiviso {
            cfg: OcrConfig::default(),
            centro_finestra: None,
            rect_finestra: None,
            in_pausa: false,
        })
    };
    let mut scansione = Scansione::new(&leggi_stato().cfg);

    loop {
        let stato = leggi_stato();
        let intervallo = stato.cfg.interval_ms;

        if stato.in_pausa || !stato.cfg.enabled {
            std::thread::sleep(Duration::from_millis(150));
            continue;
        }

        // Recepisce le modifiche fatte dal menu mentre il talk è in corso.
        if scansione.aggiorna_config(&stato.cfg)
            && tx.send(Messaggio::ZonaCambiata(scansione.zona())).is_err()
        {
            return;
        }

        scansione.imposta_overlay(stato.rect_finestra.and_then(|r| {
            zona_overlay(&stato.cfg, stato.centro_finestra, r)
        }));

        let cfg_zona = con_zona(&stato.cfg, scansione.zona());
        let msg = match leggi(
            &motore,
            &cfg_zona,
            stato.centro_finestra,
            percorso_debug.as_deref(),
        ) {
            Ok(testo) => match scansione.registra(interpreta(&testo)) {
                Esito::Accetta(l) => Some(Messaggio::Lettura(l)),
                // Numero da confermare: meglio un frame di silenzio che una slide sbagliata.
                Esito::Verifica => None,
                Esito::Ruota(z) => Some(Messaggio::ZonaCambiata(z)),
                Esito::Niente => Some(Messaggio::Niente(testo)),
            },
            Err(e) => Some(Messaggio::Errore(e)),
        };
        if let Some(msg) = msg {
            if tx.send(msg).is_err() {
                return; // l'interfaccia è stata chiusa
            }
            ctx.request_repaint();
        }
        std::thread::sleep(Duration::from_millis(intervallo));
    }
}

/// Posizione della finestra del Ritardometro sul monitor catturato, in frazioni
/// di schermo: le zone che la coprono vanno evitate, altrimenti l'OCR legge il
/// numero di slide che l'overlay stesso sta mostrando.
fn zona_overlay(
    cfg: &OcrConfig,
    centro_finestra: Option<(i32, i32)>,
    rect: (i32, i32, i32, i32),
) -> Option<Zona> {
    let m = scegli_monitor(cfg.monitor, centro_finestra).ok()?;
    let mx = m.x().ok()?;
    let my = m.y().ok()?;
    let w = m.width().ok()? as f32;
    let h = m.height().ok()? as f32;
    let (sinistra, alto, destra, basso) = rect;
    if w <= 0.0 || h <= 0.0 || destra <= sinistra || basso <= alto {
        return None;
    }
    Some(Zona {
        x: (sinistra - mx) as f32 / w,
        y: (alto - my) as f32 / h,
        width: (destra - sinistra) as f32 / w,
        height: (basso - alto) as f32 / h,
    })
}

fn leggi(
    motore: &OcrEngine,
    cfg: &OcrConfig,
    centro_finestra: Option<(i32, i32)>,
    debug: Option<&std::path::Path>,
) -> Result<String, String> {
    let grezza = cattura(cfg, centro_finestra)?;
    let immagine = prepara(grezza, cfg.magnification);
    if cfg.save_debug {
        if let Some(p) = debug {
            let _ = immagine.save(p);
        }
    }
    let testo = riconosci(motore, &immagine)?;
    let l1 = interpreta(&testo);
    if l1.is_some_and(|l| l.totale.is_some()) {
        return Ok(testo);
    }

    // Se il primo passaggio non trova numeri o trova un solo numero (es. quando la slide
    // corrente ha un colore/contrasto diverso dal "/ totale"), prova anche con contrasto
    // binarizzato normale e invertito: se uno dei due legge sia slide che totale, usa quello.
    let bin = binarizza_per_ocr(&immagine, false);
    let t2 = riconosci(motore, &bin).ok();
    let l2 = t2.as_deref().and_then(interpreta);
    if l2.is_some_and(|l| l.totale.is_some()) {
        return Ok(t2.unwrap());
    }

    let bin_inv = binarizza_per_ocr(&immagine, true);
    let t3 = riconosci(motore, &bin_inv).ok();
    let l3 = t3.as_deref().and_then(interpreta);
    if l3.is_some_and(|l| l.totale.is_some()) {
        return Ok(t3.unwrap());
    }

    if l1.is_some() {
        return Ok(testo);
    }
    if l2.is_some() {
        return Ok(t2.unwrap());
    }
    if l3.is_some() {
        return Ok(t3.unwrap());
    }

    Ok(testo)
}

pub fn info_zona_ocr(
    cfg: &OcrConfig,
    centro_finestra: Option<(i32, i32)>,
) -> Option<super::InfoZonaOcr> {
    let tutti = Monitor::all().ok()?;
    let totale_monitor = tutti.len().max(1);
    let m = scegli_monitor(cfg.monitor, centro_finestra).ok()?;
    let mx = m.x().ok()?;
    let my = m.y().ok()?;
    let w = m.width().ok()?;
    let h = m.height().ok()?;

    let numero_monitor = if cfg.monitor >= 1 {
        cfg.monitor
    } else {
        tutti
            .iter()
            .position(|cand| cand.x().ok() == Some(mx) && cand.y().ok() == Some(my))
            .map(|i| i + 1)
            .unwrap_or(1)
    };

    let x = ((cfg.x * w as f32) as u32).min(w.saturating_sub(1));
    let y = ((cfg.y * h as f32) as u32).min(h.saturating_sub(1));
    let rw = ((cfg.width * w as f32) as u32).clamp(1, w - x);
    let rh = ((cfg.height * h as f32) as u32).clamp(1, h - y);

    Some(super::InfoZonaOcr {
        numero_monitor,
        totale_monitor,
        rect_ocr: (
            mx + x as i32,
            my + y as i32,
            (rw as i32).max(180),
            (rh as i32).max(100),
        ),
        dim_monitor: (w, h),
    })
}

fn cattura(
    cfg: &OcrConfig,
    centro_finestra: Option<(i32, i32)>,
) -> Result<image::RgbaImage, String> {
    let m = scegli_monitor(cfg.monitor, centro_finestra)?;
    let w = m.width().map_err(|e| e.to_string())?;
    let h = m.height().map_err(|e| e.to_string())?;
    let x = ((cfg.x * w as f32) as u32).min(w.saturating_sub(1));
    let y = ((cfg.y * h as f32) as u32).min(h.saturating_sub(1));
    let rw = ((cfg.width * w as f32) as u32).clamp(1, w - x);
    let rh = ((cfg.height * h as f32) as u32).clamp(1, h - y);
    m.capture_region(x, y, rw, rh)
        .map_err(|e| format!("Screen capture failed: {e}"))
}

/// Scala di grigi + normalizzazione del contrasto + ingrandimento:
/// l'OCR legge molto meglio i numeri piccoli o grigio chiaro (tipici di Marp).
fn prepara(img: image::RgbaImage, ingrandimento: u32) -> GrayImage {
    let mut grigio = DynamicImage::ImageRgba8(img).into_luma8();

    // Estende il contrasto (auto-level) se la dinamica è bassa ma non piatta
    let mut min_v = 255u8;
    let mut max_v = 0u8;
    for p in grigio.pixels() {
        let v = p.0[0];
        min_v = min_v.min(v);
        max_v = max_v.max(v);
    }
    let delta = max_v.saturating_sub(min_v);
    if (12..220).contains(&delta) {
        for p in grigio.pixels_mut() {
            let v = ((p.0[0].saturating_sub(min_v) as u32 * 255) / delta as u32).min(255) as u8;
            *p = Luma([v]);
        }
    }

    let (w, h) = grigio.dimensions();
    let lato = w.max(h).max(1) as f32;
    let scala = (ingrandimento.max(1) as f32).min(LATO_MASSIMO as f32 / lato);
    if (scala - 1.0).abs() <= 0.08 {
        return grigio;
    }
    let nw = ((w as f32 * scala).round() as u32).max(1);
    let nh = ((h as f32 * scala).round() as u32).max(1);
    image::imageops::resize(&grigio, nw, nh, FilterType::CatmullRom)
}

/// Crea una versione ad alto contrasto (nero su bianco, o invertita) per aiutare l'OCR
/// sui contatori slide chiari su sfondo scuro o grigi tenui.
fn binarizza_per_ocr(img: &GrayImage, inverti: bool) -> GrayImage {
    let mut somma: u64 = 0;
    for p in img.pixels() {
        somma += p.0[0] as u64;
    }
    let n = (img.width() as u64 * img.height() as u64).max(1);
    let media = (somma / n) as u8;
    let soglia = if inverti {
        media.saturating_add(18)
    } else {
        media.saturating_sub(18)
    };

    let mut out = GrayImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        let v = p.0[0];
        let acceso = if inverti { v >= soglia } else { v <= soglia };
        out.put_pixel(x, y, Luma([if acceso { 0 } else { 255 }]));
    }
    out
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
    ocr().map_err(|e| format!("OCR failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::Lettura;

    #[test]
    fn riconosce_debug_png_10_su_35() {
        let p = std::path::Path::new(r"D:\tool\ritardometro\ritardometro-debug.png");
        if !p.exists() {
            return;
        }
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        let motore = OcrEngine::TryCreateFromUserProfileLanguages().unwrap();
        let rgba = image::open(p).unwrap().into_rgba8();
        let (w, h) = rgba.dimensions();

        // 1) Simula la cattura nativa 1x (575x259) passata a prepara(..., 3)
        let grezza_1x = image::imageops::resize(&rgba, w / 3, h / 3, FilterType::CatmullRom);
        let prep_da_1x = prepara(grezza_1x, 3);
        let t_da_1x = riconosci(&motore, &prep_da_1x).unwrap();
        assert_eq!(
            interpreta(&t_da_1x),
            Some(Lettura {
                slide: 10,
                totale: Some(35)
            })
        );

        // 2) Anche passando l'immagine già 3x (1725x777), prepara() la riporta <= 900px e legge 10/35
        let prep_da_3x = prepara(rgba, 3);
        let t_da_3x = riconosci(&motore, &prep_da_3x).unwrap();
        assert_eq!(
            interpreta(&t_da_3x),
            Some(Lettura {
                slide: 10,
                totale: Some(35)
            })
        );
    }
}


