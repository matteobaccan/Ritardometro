//! Lettura del numero di slide dallo schermo.
//!
//! Un thread in background cattura periodicamente l'angolo dello schermo
//! con il contatore, lo passa all'OCR integrato di Windows e manda il
//! risultato all'interfaccia tramite un canale.

use crate::config::OcrConfig;
use regex::Regex;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(windows)]
mod windows;

/// Stato condiviso tra la UI e il thread OCR, così l'OCR sa sempre su quale
/// monitor si trova la finestra del Ritardometro (e recepisce cambi live dal menu).
#[derive(Debug, Clone)]
pub struct StatoOcrCondiviso {
    pub cfg: OcrConfig,
    /// Centro fisico (x, y) della finestra dell'overlay sullo schermo.
    pub centro_finestra: Option<(i32, i32)>,
    /// Se vero, il cronometro è in pausa e l'OCR non deve leggere lo schermo.
    pub in_pausa: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lettura {
    pub slide: u32,
    pub totale: Option<u32>,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Messaggio {
    Lettura(Lettura),
    /// L'OCR ha girato ma non ha trovato un numero.
    Niente(String),
    Errore(String),
}

/// Informazioni geometriche sul monitor e sulla zona OCR, usate per mostrare
/// l'effetto visivo di identificazione monitor ("Mon 1 / Mon 2 / Mon 3").
#[derive(Debug, Clone, Copy)]
pub struct InfoZonaOcr {
    /// Indice 1-based del monitor scelto (`1`, `2`, `3`...).
    pub numero_monitor: usize,
    /// Numero totale di monitor rilevati.
    pub totale_monitor: usize,
    /// Coordinate fisiche `(x, y, larghezza, altezza)` del rettangolo OCR sullo schermo.
    pub rect_ocr: (i32, i32, i32, i32),
    /// Risoluzione in pixel `(larghezza, altezza)` del monitor scelto.
    pub dim_monitor: (u32, u32),
}

/// Restituisce la posizione fisica della zona OCR e il numero del monitor selezionato.
pub fn info_zona_ocr(cfg: &OcrConfig, centro_finestra: Option<(i32, i32)>) -> Option<InfoZonaOcr> {
    #[cfg(windows)]
    {
        windows::info_zona_ocr(cfg, centro_finestra)
    }
    #[cfg(not(windows))]
    {
        let _ = (cfg, centro_finestra);
        None
    }
}

/// Restituisce quanti monitor sono stati rilevati sul sistema.
pub fn numero_monitor() -> usize {
    #[cfg(windows)]
    {
        windows::numero_monitor()
    }
    #[cfg(not(windows))]
    {
        1
    }
}

/// Avvia il thread di lettura. Restituisce `Err` se l'OCR non è disponibile.
pub fn avvia(
    condiviso: Arc<Mutex<StatoOcrCondiviso>>,
    ctx: eframe::egui::Context,
) -> Result<Receiver<Messaggio>, String> {
    #[cfg(windows)]
    {
        windows::avvia(condiviso, ctx)
    }
    #[cfg(not(windows))]
    {
        let _ = (condiviso, ctx);
        Err("OCR disponibile solo su Windows: funziona come semplice timer".into())
    }
}

/// Estrae il numero di slide dal testo riconosciuto.
///
/// Gestisce i formati più comuni: `12`, `12 / 40`, `12/40`, `12 di 40`,
/// `12 of 40`, `12 40` (quando l'OCR perde la barra `/`), e gli errori tipici
/// dell'OCR anche su numeri a singola cifra (`I / 30` -> `1 / 30`, `/ 30` senza
/// slide corrente ignorato invece di scambiare il totale per la slide corrente).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn interpreta(testo: &str) -> Option<Lettura> {
    static CON_TOTALE: OnceLock<Regex> = OnceLock::new();
    static SOLO_TOTALE: OnceLock<Regex> = OnceLock::new();
    static BARRA_COME_CIFRA: OnceLock<Regex> = OnceLock::new();
    static DUE_NUMERI_FINALI: OnceLock<Regex> = OnceLock::new();
    static NUMERO: OnceLock<Regex> = OnceLock::new();

    let con_totale = CON_TOTALE.get_or_init(|| {
        Regex::new(r"(?i)(\d{1,3})\s*(?:/|\\|di|de|su|of|\||-)\s*(\d{1,3})").unwrap()
    });
    // Se l'OCR legge solo "/ 30" (perdendo la cifra prima della barra), 30 è il totale
    // e NON la slide corrente: non dobbiamo restituire `slide: 30`.
    let solo_totale =
        SOLO_TOTALE.get_or_init(|| Regex::new(r"(?i)^\s*(?:/|\\|di|de|su|of)\s*\d{1,3}\s*$").unwrap());
    // Spesso Windows OCR legge la barra `/` inclinata come `1` o `7` staccata da spazi:
    // es. "10 / 35" -> "10 1 35" oppure "10 7 35".
    let barra_come_cifra = BARRA_COME_CIFRA
        .get_or_init(|| Regex::new(r"(?:^|\D)(\d{1,3})\s+[17]\s+(\d{1,3})\s*$").unwrap());
    // Se l'OCR perde la barra sottile `/` in "3 / 30" e restituisce "3 30" a fine riga:
    let due_numeri_finali =
        DUE_NUMERI_FINALI.get_or_init(|| Regex::new(r"(?:^|\D)(\d{1,3})\s+(\d{1,3})\s*$").unwrap());
    let numero = NUMERO.get_or_init(|| Regex::new(r"\d+").unwrap());

    let pulito = correggi_ocr(testo);

    // 1. Formato esplicito "slide / totale" (prendiamo l'ultima occorrenza valida nel riquadro)
    let mut ultimo_con_totale = None;
    for c in con_totale.captures_iter(&pulito) {
        if let (Ok(slide), Ok(totale)) = (c[1].parse::<u32>(), c[2].parse::<u32>()) {
            if slide >= 1 && totale >= slide && totale >= 2 {
                ultimo_con_totale = Some(Lettura {
                    slide,
                    totale: Some(totale),
                });
            }
        }
    }
    if ultimo_con_totale.is_some() {
        return ultimo_con_totale;
    }

    // 2. Se c'è solo "/ 30" senza la slide corrente prima della barra, non scambiare il totale per la slide!
    if solo_totale.is_match(pulito.trim()) {
        return None;
    }

    // 3. Se l'OCR ha letto la barra `/` come `1` o `7` tra due numeri ("10 1 35"):
    if let Some(c) = barra_come_cifra.captures(&pulito) {
        if let (Ok(a), Ok(b)) = (c[1].parse::<u32>(), c[2].parse::<u32>()) {
            if a >= 1 && b >= a && (2..=999).contains(&b) {
                return Some(Lettura {
                    slide: a,
                    totale: Some(b),
                });
            }
        }
    }

    // 4. Se ci sono due numeri separati da spazio a fine testo ("3 30") con primo <= secondo,
    // è un contatore "slide / totale" in cui l'OCR non ha visto la barra `/`.
    if let Some(c) = due_numeri_finali.captures(&pulito) {
        if let (Ok(a), Ok(b)) = (c[1].parse::<u32>(), c[2].parse::<u32>()) {
            if a >= 1 && b >= a && (2..=999).contains(&b) {
                return Some(Lettura {
                    slide: a,
                    totale: Some(b),
                });
            }
        }
    }

    // 5. Solo numero (paginazione Marp di default): prendo l'ultimo numero valido (1..=999),
    // che nell'angolo in basso a destra è il contatore.
    numero
        .find_iter(&pulito)
        .filter_map(|m| m.as_str().parse::<u32>().ok())
        .filter(|&n| (1..=999).contains(&n))
        .last()
        .map(|slide| Lettura {
            slide,
            totale: None,
        })
}

/// L'OCR scambia spesso lettere e cifre, sia quando sono vicine ad altre cifre (`1O` -> `10`,
/// `2l` -> `21`), sia quando la slide corrente è a singola cifra davanti a `/` (`l / 30` -> `1 / 30`,
/// `I/30` -> `1/30`, `S / 30` -> `5 / 30`).
#[cfg_attr(not(windows), allow(dead_code))]
fn correggi_ocr(testo: &str) -> String {
    let caratteri: Vec<char> = testo.chars().collect();
    let n = caratteri.len();
    let cifra = |i: usize| caratteri.get(i).is_some_and(|c| c.is_ascii_digit());
    let spazio_o_inizio = |i: isize| {
        if i < 0 {
            true
        } else {
            caratteri
                .get(i as usize)
                .is_some_and(|c| c.is_whitespace())
        }
    };
    // Guarda se subito dopo (eventuali spazi) c'è un `/` seguito da cifre o un numero
    let precede_separatore_totale = |i: usize| {
        let resto: String = caratteri[i + 1..].iter().collect();
        let r = resto.trim_start();
        if let Some(dopo_sep) = r
            .strip_prefix('/')
            .or_else(|| r.strip_prefix('\\'))
            .or_else(|| r.strip_prefix('|'))
        {
            dopo_sep
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        } else {
            false
        }
    };

    caratteri
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let vicino_a_cifra = (i > 0 && cifra(i - 1)) || (i + 1 < n && cifra(i + 1));
            let singola_prima_di_barra =
                spazio_o_inizio(i as isize - 1) && precede_separatore_totale(i);
            let contesto_numerico = vicino_a_cifra || singola_prima_di_barra;
            match c {
                'O' | 'o' | 'Q' | 'D' | 'Ø' | 'ø' | '@' if vicino_a_cifra => '0',
                'l' | 'I' | 'i' | '|' | '!' if contesto_numerico => '1',
                'S' | 's' if contesto_numerico => '5',
                'B' if contesto_numerico => '8',
                'Z' | 'z' if contesto_numerico => '2',
                _ => c,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(slide: u32, totale: Option<u32>) -> Option<Lettura> {
        Some(Lettura { slide, totale })
    }

    #[test]
    fn formati_comuni() {
        assert_eq!(interpreta("12"), l(12, None));
        assert_eq!(interpreta("12 / 40"), l(12, Some(40)));
        assert_eq!(interpreta("12/40"), l(12, Some(40)));
        assert_eq!(interpreta("12 di 40"), l(12, Some(40)));
        assert_eq!(interpreta("Slide 3 of 25"), l(3, Some(25)));
        // Quando l'OCR perde la barra `/` tra slide e totale:
        assert_eq!(interpreta("3 25"), l(3, Some(25)));
    }

    #[test]
    fn errori_tipici_ocr() {
        assert_eq!(interpreta("1O"), l(10, None));
        assert_eq!(interpreta("2l / 3O"), l(21, Some(30)));
        // Singola cifra davanti alla barra `/`
        assert_eq!(interpreta("l / 30"), l(1, Some(30)));
        assert_eq!(interpreta("I/30"), l(1, Some(30)));
        assert_eq!(interpreta("S / 30"), l(5, Some(30)));
    }

    #[test]
    fn solo_totale_dopo_barra_viene_ignorato() {
        assert_eq!(interpreta("/ 30"), None);
        assert_eq!(interpreta(" / 25 "), None);
    }

    #[test]
    fn testo_intorno() {
        assert_eq!(interpreta("@matteobaccan  #DevSecOpsDay  17"), l(17, None));
    }

    #[test]
    fn niente_da_leggere() {
        assert_eq!(interpreta(""), None);
        assert_eq!(interpreta("Grazie!"), None);
        assert_eq!(interpreta("0"), None);
    }

    #[test]
    fn totale_minore_della_slide_non_e_un_contatore() {
        // "40 / 12" non ha senso come contatore: prendo l'ultimo numero
        assert_eq!(interpreta("40 / 12"), l(12, None));
    }
}
