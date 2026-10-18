//! Lettura del numero di slide dallo schermo.
//!
//! Un thread in background cattura periodicamente l'angolo dello schermo
//! con il contatore, lo passa all'OCR integrato di Windows e manda il
//! risultato all'interfaccia tramite un canale.

use crate::config::OcrConfig;
use regex::Regex;
use std::sync::mpsc::Receiver;
use std::sync::OnceLock;

#[cfg(windows)]
mod windows;

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

/// Avvia il thread di lettura. Restituisce `Err` se l'OCR non è disponibile.
pub fn avvia(cfg: OcrConfig, ctx: eframe::egui::Context) -> Result<Receiver<Messaggio>, String> {
    #[cfg(windows)]
    {
        windows::avvia(cfg, ctx)
    }
    #[cfg(not(windows))]
    {
        let _ = (cfg, ctx);
        Err("OCR disponibile solo su Windows: funziona come semplice timer".into())
    }
}

/// Estrae il numero di slide dal testo riconosciuto.
///
/// Gestisce i formati più comuni: `12`, `12 / 40`, `12/40`, `12 di 40`,
/// `12 of 40`, e alcuni errori tipici dell'OCR (`O` al posto di `0`, ecc.).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn interpreta(testo: &str) -> Option<Lettura> {
    static CON_TOTALE: OnceLock<Regex> = OnceLock::new();
    static NUMERO: OnceLock<Regex> = OnceLock::new();
    let con_totale = CON_TOTALE
        .get_or_init(|| Regex::new(r"(?i)(\d{1,3})\s*(?:/|\\|di|de|of|\|)\s*(\d{1,3})").unwrap());
    let numero = NUMERO.get_or_init(|| Regex::new(r"\d{1,3}").unwrap());

    let pulito = correggi_ocr(testo);

    if let Some(c) = con_totale.captures(&pulito) {
        let slide: u32 = c[1].parse().ok()?;
        let totale: u32 = c[2].parse().ok()?;
        if slide >= 1 && totale >= slide {
            return Some(Lettura {
                slide,
                totale: Some(totale),
            });
        }
    }

    // Solo numero (paginazione Marp di default): prendo l'ultimo numero,
    // che nell'angolo in basso a destra è quasi sempre il contatore.
    numero
        .find_iter(&pulito)
        .filter_map(|m| m.as_str().parse::<u32>().ok())
        .filter(|&n| n >= 1)
        .last()
        .map(|slide| Lettura {
            slide,
            totale: None,
        })
}

/// L'OCR scambia spesso lettere e cifre quando sono vicine ad altre cifre.
#[cfg_attr(not(windows), allow(dead_code))]
fn correggi_ocr(testo: &str) -> String {
    let caratteri: Vec<char> = testo.chars().collect();
    let cifra = |i: usize| caratteri.get(i).is_some_and(|c| c.is_ascii_digit());
    caratteri
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let vicino_a_cifra = (i > 0 && cifra(i - 1)) || cifra(i + 1);
            match c {
                'O' | 'o' | 'Q' | 'D' if vicino_a_cifra => '0',
                'l' | 'I' | 'i' if vicino_a_cifra => '1',
                'S' | 's' if vicino_a_cifra => '5',
                'B' if vicino_a_cifra => '8',
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
    }

    #[test]
    fn errori_tipici_ocr() {
        assert_eq!(interpreta("1O"), l(10, None));
        assert_eq!(interpreta("2l / 3O"), l(21, Some(30)));
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
