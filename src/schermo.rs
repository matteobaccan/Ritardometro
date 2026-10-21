//! Rilevamento del cambio di schermata, per le presentazioni senza numerazione.
//!
//! Non serve l'OCR: basta una miniatura in scala di grigi dello schermo da
//! confrontare con la precedente. Se non cambia niente per molto più del tempo
//! medio che una slide dovrebbe durare, chi parla si sta dilungando.
//!
//! La cattura usa `xcap`, disponibile qui su Windows e macOS: su Linux
//! pretende `libpipewire-0.3` a build time, quindi lì il rilevamento non c'è.

// Dove non c'è cattura il confronto delle impronte resta compilato (e testato)
// ma inutilizzato dal programma.
#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

/// Celle della miniatura: pochissime bastano per sapere *se* la schermata è
/// cambiata, e rendono il confronto insensibile al rumore di compressione.
pub const LARG: u32 = 64;
pub const ALT: u32 = 36;

/// Quanto deve cambiare la luminosità di una cella perché conti come diversa.
const SOGLIA_CELLA: i16 = 24;
/// Frazione di celle diverse oltre la quale la schermata è cambiata davvero:
/// sotto ci stanno cursore, orologio di sistema e notifiche di passaggio.
const SOGLIA_CAMBIO: f32 = 0.06;

/// Ogni quanto si guarda lo schermo. Le soglie in gioco sono decine di secondi,
/// quindi un secondo è abbondante e costa quasi niente.
const INTERVALLO_MS: u64 = 1000;

/// Miniatura in scala di grigi di una schermata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impronta(Vec<u8>);

impl Impronta {
    /// Riduce l'immagine catturata a `LARG x ALT` celle di luminosità.
    pub fn da_immagine(img: &image::RgbaImage) -> Self {
        let piccola = image::imageops::resize(img, LARG, ALT, image::imageops::FilterType::Triangle);
        Self(
            image::DynamicImage::ImageRgba8(piccola)
                .into_luma8()
                .into_raw(),
        )
    }

    /// Frazione di celle che differiscono in modo percepibile (0.0..1.0).
    pub fn differenza(&self, altra: &Impronta) -> f32 {
        if self.0.len() != altra.0.len() || self.0.is_empty() {
            return 1.0;
        }
        let diverse = self
            .0
            .iter()
            .zip(&altra.0)
            .filter(|(a, b)| (**a as i16 - **b as i16).abs() >= SOGLIA_CELLA)
            .count();
        diverse as f32 / self.0.len() as f32
    }

    pub fn cambiata(&self, altra: &Impronta) -> bool {
        self.differenza(altra) >= SOGLIA_CAMBIO
    }

    /// Spegne le celle coperte dal rettangolo indicato, in frazioni di schermo.
    ///
    /// Serve per l'overlay del Ritardometro: l'avatar respira e il cronometro
    /// scorre, quindi senza mascheratura la schermata risulterebbe sempre nuova.
    pub fn maschera(&mut self, (x, y, larghezza, altezza): (f32, f32, f32, f32)) {
        let da_x = (x * LARG as f32).floor().clamp(0.0, LARG as f32) as u32;
        let a_x = ((x + larghezza) * LARG as f32).ceil().clamp(0.0, LARG as f32) as u32;
        let da_y = (y * ALT as f32).floor().clamp(0.0, ALT as f32) as u32;
        let a_y = ((y + altezza) * ALT as f32).ceil().clamp(0.0, ALT as f32) as u32;
        for riga in da_y..a_y {
            for colonna in da_x..a_x {
                self.0[(riga * LARG + colonna) as usize] = 0;
            }
        }
    }
}

/// La finestra è ancora visibile su uno dei monitor collegati?
///
/// Serve al ripristino della posizione: se lo schermo su cui stava l'overlay
/// non c'è più, rimetterlo lì lo renderebbe invisibile. Si pretende che il
/// centro cada dentro un monitor e che almeno il 60% dell'area sia a video.
pub fn posizione_visibile(finestra: (i32, i32, u32, u32), monitor: &[(i32, i32, u32, u32)]) -> bool {
    let (fx, fy, fw, fh) = finestra;
    if fw == 0 || fh == 0 {
        return false;
    }
    let cx = fx + fw as i32 / 2;
    let cy = fy + fh as i32 / 2;
    let area = fw as i64 * fh as i64;
    monitor.iter().any(|&(mx, my, mw, mh)| {
        let dentro = cx >= mx && cx < mx + mw as i32 && cy >= my && cy < my + mh as i32;
        let sovrapposta_x = (fx + fw as i32).min(mx + mw as i32) - fx.max(mx);
        let sovrapposta_y = (fy + fh as i32).min(my + mh as i32) - fy.max(my);
        dentro
            && sovrapposta_x > 0
            && sovrapposta_y > 0
            && (sovrapposta_x as i64 * sovrapposta_y as i64) * 10 >= area * 6
    })
}

#[derive(Debug, Clone)]
pub enum Messaggio {
    /// La schermata è cambiata: siamo passati a un'altra slide.
    Cambiato,
    Errore(String),
}

#[cfg(any(windows, target_os = "macos"))]
pub use disponibile::{avvia, monitor_rettangoli, numero_monitor};
/// Usata dall'OCR, che esiste solo su Windows.
#[cfg(windows)]
pub use disponibile::scegli_monitor;

#[cfg(any(windows, target_os = "macos"))]
mod disponibile {
    use super::{Impronta, Messaggio, INTERVALLO_MS};
    use crate::ocr::StatoOcrCondiviso;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use xcap::Monitor;

    /// Monitor da catturare: `0` segue la finestra del Ritardometro.
    pub fn scegli_monitor(indice: usize, centro_finestra: Option<(i32, i32)>) -> Result<Monitor, String> {
        if indice == 0 {
            // Se conosciamo la posizione della finestra del Ritardometro, usiamo lo schermo
            // su cui si trova la finestra!
            if let Some((cx, cy)) = centro_finestra {
                if let Ok(m) = Monitor::from_point(cx, cy) {
                    return Ok(m);
                }
            }
            let monitor = Monitor::all().map_err(|e| format!("Monitors not found: {e}"))?;
            monitor
                .iter()
                .find(|m| m.is_primary().unwrap_or(false))
                .or_else(|| monitor.first())
                .cloned()
                .ok_or_else(|| "No monitor found".into())
        } else {
            let monitor = Monitor::all().map_err(|e| format!("Monitors not found: {e}"))?;
            let n = monitor.len();
            monitor
                .into_iter()
                .nth(indice - 1)
                .ok_or_else(|| format!("Monitor {indice} does not exist ({n} found)"))
        }
    }

    pub fn numero_monitor() -> usize {
        Monitor::all().map(|m| m.len().max(1)).unwrap_or(1)
    }

    /// Rettangoli fisici `(x, y, larghezza, altezza)` dei monitor collegati.
    pub fn monitor_rettangoli() -> Vec<(i32, i32, u32, u32)> {
        let Ok(tutti) = Monitor::all() else {
            return Vec::new();
        };
        tutti
            .iter()
            .filter_map(|m| Some((m.x().ok()?, m.y().ok()?, m.width().ok()?, m.height().ok()?)))
            .collect()
    }

    /// Avvia il thread che sorveglia la schermata.
    pub fn avvia(
        condiviso: Arc<Mutex<StatoOcrCondiviso>>,
        ctx: eframe::egui::Context,
    ) -> Result<Receiver<Messaggio>, String> {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("ritardometro-schermo".into())
            .spawn(move || ciclo(condiviso, tx, ctx))
            .map_err(|e| format!("Cannot start the screen watcher thread: {e}"))?;
        Ok(rx)
    }

    fn ciclo(
        condiviso: Arc<Mutex<StatoOcrCondiviso>>,
        tx: Sender<Messaggio>,
        ctx: eframe::egui::Context,
    ) {
        let mut precedente: Option<Impronta> = None;
        let mut ultimo_errore: Option<String> = None;

        loop {
            let stato = match condiviso.lock() {
                Ok(s) => s.clone(),
                Err(_) => return,
            };
            if stato.in_pausa || !stato.sorveglia_schermo {
                precedente = None;
                std::thread::sleep(Duration::from_millis(150));
                continue;
            }

            match impronta(stato.cfg.monitor, stato.centro_finestra, stato.rect_finestra) {
                Ok(attuale) => {
                    ultimo_errore = None;
                    let cambiata = precedente.as_ref().is_some_and(|p| attuale.cambiata(p));
                    precedente = Some(attuale);
                    if cambiata {
                        if tx.send(Messaggio::Cambiato).is_err() {
                            return; // l'interfaccia è stata chiusa
                        }
                        ctx.request_repaint();
                    }
                }
                Err(e) => {
                    // Una cattura che fallisce di continuo (su macOS manca il
                    // permesso di registrazione schermo) va detta una volta sola.
                    if ultimo_errore.as_deref() != Some(e.as_str()) {
                        ultimo_errore = Some(e.clone());
                        if tx.send(Messaggio::Errore(e)).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                    }
                    precedente = None;
                }
            }

            std::thread::sleep(Duration::from_millis(INTERVALLO_MS));
        }
    }

    /// Cattura il monitor e ne restituisce l'impronta, senza la zona coperta
    /// dalla finestra del Ritardometro.
    fn impronta(
        monitor: usize,
        centro_finestra: Option<(i32, i32)>,
        rect_finestra: Option<(i32, i32, i32, i32)>,
    ) -> Result<Impronta, String> {
        let m = scegli_monitor(monitor, centro_finestra)?;
        let immagine = m
            .capture_image()
            .map_err(|e| format!("Screen capture failed: {e}"))?;
        let mut impronta = Impronta::da_immagine(&immagine);

        if let (Some((sinistra, alto, destra, basso)), Ok(mx), Ok(my), Ok(w), Ok(h)) = (
            rect_finestra,
            m.x(),
            m.y(),
            m.width(),
            m.height(),
        ) {
            if w > 0 && h > 0 && destra > sinistra && basso > alto {
                impronta.maschera((
                    (sinistra - mx) as f32 / w as f32,
                    (alto - my) as f32 / h as f32,
                    (destra - sinistra) as f32 / w as f32,
                    (basso - alto) as f32 / h as f32,
                ));
            }
        }
        Ok(impronta)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn numero_monitor() -> usize {
    1
}

/// Senza `xcap` non si sa quali monitor ci siano: decide chi chiama.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn monitor_rettangoli() -> Vec<(i32, i32, u32, u32)> {
    Vec::new()
}

/// Su Linux `xcap` non è compilato: il rilevamento non è disponibile.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn avvia(
    _condiviso: std::sync::Arc<std::sync::Mutex<crate::ocr::StatoOcrCondiviso>>,
    _ctx: eframe::egui::Context,
) -> Result<std::sync::mpsc::Receiver<Messaggio>, String> {
    Err("Screen change detection is not available on this platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

const SCHERMO: (i32, i32, u32, u32) = (0, 0, 1920, 1080);
    const SECONDO: (i32, i32, u32, u32) = (1920, -200, 2560, 1440);

    #[test]
    fn una_finestra_dentro_lo_schermo_e_visibile() {
        assert!(posizione_visibile((1600, 24, 220, 340), &[SCHERMO]));
        // anche sul secondo monitor, con origine negativa
        assert!(posizione_visibile((4200, -150, 220, 340), &[SCHERMO, SECONDO]));
    }

    #[test]
    fn una_finestra_fuori_schermo_non_e_visibile() {
        // monitor scollegato: le coordinate salvate non esistono più
        assert!(!posizione_visibile((4200, -150, 220, 340), &[SCHERMO]));
        // mezza fuori dal bordo destro: meno del 60% a video
        assert!(!posizione_visibile((1830, 24, 220, 340), &[SCHERMO]));
        // sparita in alto a sinistra
        assert!(!posizione_visibile((-9000, -9000, 220, 340), &[SCHERMO]));
        // nessun monitor noto
        assert!(!posizione_visibile((100, 100, 220, 340), &[]));
    }

    #[test]
    fn una_finestra_appena_sul_bordo_resta_valida() {
        // sporge di 40 px su 220: l'81% è ancora a video
        assert!(posizione_visibile((1740, 700, 220, 340), &[SCHERMO]));
    }

    /// Schermata finta: sfondo uniforme con un rettangolo chiaro dentro.
    fn schermata(sfondo: u8, rettangolo: Option<(u32, u32, u32, u32)>) -> RgbaImage {
        let mut img = RgbaImage::from_pixel(640, 360, Rgba([sfondo, sfondo, sfondo, 255]));
        if let Some((x, y, w, h)) = rettangolo {
            for py in y..(y + h).min(360) {
                for px in x..(x + w).min(640) {
                    img.put_pixel(px, py, Rgba([255, 255, 255, 255]));
                }
            }
        }
        img
    }

    #[test]
    fn la_stessa_schermata_non_risulta_cambiata() {
        let a = Impronta::da_immagine(&schermata(30, Some((50, 50, 200, 120))));
        let b = Impronta::da_immagine(&schermata(30, Some((50, 50, 200, 120))));
        assert_eq!(a.differenza(&b), 0.0);
        assert!(!a.cambiata(&b));
    }

    #[test]
    fn un_dettaglio_piccolo_non_e_un_cambio_di_slide() {
        // Un orologio o una notifica occupano una porzione minima dello schermo
        let a = Impronta::da_immagine(&schermata(30, None));
        let b = Impronta::da_immagine(&schermata(30, Some((600, 0, 40, 20))));
        assert!(a.differenza(&b) > 0.0, "qualcosa deve essere cambiato");
        assert!(!a.cambiata(&b), "ma non abbastanza da essere una slide nuova");
    }

    #[test]
    fn una_slide_nuova_risulta_cambiata() {
        let a = Impronta::da_immagine(&schermata(30, Some((20, 20, 300, 200))));
        let b = Impronta::da_immagine(&schermata(30, Some((300, 120, 300, 200))));
        assert!(a.cambiata(&b));
    }

    #[test]
    fn la_zona_dell_overlay_viene_ignorata() {
        // Solo l'angolo in alto a destra cambia: è lì che sta il Ritardometro,
        // che si anima da solo e non deve passare per una slide nuova.
        let a = Impronta::da_immagine(&schermata(30, None));
        let b = Impronta::da_immagine(&schermata(30, Some((448, 0, 192, 180))));
        assert!(a.cambiata(&b), "senza maschera il cambio si vede");

        let overlay = (0.7, 0.0, 0.3, 0.5);
        let mut a = a;
        let mut b = b;
        a.maschera(overlay);
        b.maschera(overlay);
        assert!(!a.cambiata(&b));
    }

    #[test]
    fn impronte_di_dimensione_diversa_contano_come_cambio() {
        let a = Impronta::da_immagine(&schermata(30, None));
        let b = Impronta(vec![]);
        assert_eq!(a.differenza(&b), 1.0);
    }
}
