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
    /// Rettangolo fisico `(sinistra, alto, destra, basso)` della finestra dell'overlay.
    pub rect_finestra: Option<(i32, i32, i32, i32)>,
    /// Se vero, il cronometro è in pausa e l'OCR non deve leggere lo schermo.
    pub in_pausa: bool,
    /// Se falso, nessuno deve catturare lo schermo per l'orologio: in modalità
    /// Timer non si tocca lo schermo affatto.
    pub sorveglia_schermo: bool,
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
    /// La ricerca automatica sta provando una nuova zona dello schermo.
    ZonaCambiata(Zona),
}

// ------------------------------------------------------------------
// Zona di cattura e ricerca automatica del contatore
// ------------------------------------------------------------------

/// Rettangolo di cattura espresso in frazioni dello schermo (`0.0..1.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zona {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Zona {
    pub fn da_config(cfg: &OcrConfig) -> Self {
        Self {
            x: cfg.x,
            y: cfg.y,
            width: cfg.width,
            height: cfg.height,
        }
    }

    /// Due zone che si sovrappongono quasi del tutto (entro il 2% dello schermo):
    /// serve a non provare due volte la stessa porzione durante la scansione.
    pub fn vicina(&self, a: &Zona) -> bool {
        (self.x - a.x).abs() < 0.02
            && (self.y - a.y).abs() < 0.02
            && (self.width - a.width).abs() < 0.02
            && (self.height - a.height).abs() < 0.02
    }

    /// Vero se i due rettangoli hanno un'area in comune.
    pub fn sovrapposta(&self, a: &Zona) -> bool {
        self.x < a.x + a.width
            && a.x < self.x + self.width
            && self.y < a.y + a.height
            && a.y < self.y + self.height
    }

    /// Nome leggibile della posizione, per lo stato nell'overlay: `"bottom-right"`.
    pub fn etichetta(&self) -> String {
        let cx = self.x + self.width / 2.0;
        let cy = self.y + self.height / 2.0;
        let verticale = if cy < 0.34 {
            "top"
        } else if cy > 0.66 {
            "bottom"
        } else {
            "middle"
        };
        let orizzontale = if cx < 0.34 {
            "left"
        } else if cx > 0.66 {
            "right"
        } else {
            "center"
        };
        format!("{verticale}-{orizzontale}")
    }
}

/// Copia della configurazione con un'altra zona di cattura.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn con_zona(cfg: &OcrConfig, zona: Zona) -> OcrConfig {
    OcrConfig {
        x: zona.x,
        y: zona.y,
        width: zona.width,
        height: zona.height,
        ..cfg.clone()
    }
}

/// Zone da provare, in ordine di probabilità: prima quella configurata, poi gli
/// angoli e i centri dei bordi, mantenendo le dimensioni scelte dall'utente.
pub fn zone_candidate(cfg: &OcrConfig) -> Vec<Zona> {
    let base = Zona::da_config(cfg);
    let w = base.width.clamp(0.05, 0.5);
    let h = base.height.clamp(0.05, 0.5);
    let destra = 1.0 - w;
    let basso = 1.0 - h;
    let centro = (1.0 - w) / 2.0;

    let mut zone = vec![base];
    for (x, y) in [
        (destra, basso),
        (0.0, basso),
        (centro, basso),
        (destra, 0.0),
        (0.0, 0.0),
        (centro, 0.0),
    ] {
        let z = Zona {
            x,
            y,
            width: w,
            height: h,
        };
        if !zone.iter().any(|a| a.vicina(&z)) {
            zone.push(z);
        }
    }
    zone
}

/// Cosa fare dopo aver passato una cattura all'OCR.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Esito {
    /// Lettura affidabile: mandala all'interfaccia.
    Accetta(Lettura),
    /// Numero letto da una zona non ancora confermata: aspetto una conferma
    /// prima di crederci, così un "3" di un elenco puntato non diventa una slide.
    Verifica,
    /// La zona non legge più niente: passo alla successiva.
    Ruota(Zona),
    /// Nessun numero, zona invariata.
    Niente,
}

/// Letture a vuoto consecutive prima di abbandonare una zona che aveva funzionato
/// (a 250 ms sono ~4 secondi: una slide senza numerazione non fa ripartire la ricerca).
const MANCANZE_ZONA_CONFERMATA: u32 = 16;
/// Letture a vuoto consecutive prima di scartare una zona in prova.
const MANCANZE_ZONA_IN_PROVA: u32 = 2;
/// Letture valide consecutive per confermare una zona trovata dalla scansione.
const CONFERME_PER_BLOCCARE: u32 = 2;

/// Ricerca automatica della zona del contatore (issue #7).
///
/// Parte dalla zona configurata, che è considerata affidabile da subito. Se
/// smette di leggere, prova le altre e si blocca sulla prima che restituisce
/// un contatore credibile, ricordandola come preferita.
#[derive(Debug)]
pub struct Scansione {
    candidate: Vec<Zona>,
    indice: usize,
    /// Ultima zona che ha prodotto letture valide: tornandoci è subito affidabile.
    preferita: usize,
    confermata: bool,
    mancanze: u32,
    conferme: u32,
    attiva: bool,
    /// Dov'è la finestra del Ritardometro: va evitata, altrimenti l'OCR legge
    /// il numero di slide che l'overlay stesso sta mostrando.
    overlay: Option<Zona>,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Scansione {
    pub fn new(cfg: &OcrConfig) -> Self {
        Self {
            candidate: zone_candidate(cfg),
            indice: 0,
            preferita: 0,
            confermata: true,
            mancanze: 0,
            conferme: 0,
            attiva: cfg.auto_zone,
            overlay: None,
        }
    }

    /// Comunica dove si trova la finestra dell'overlay, in frazioni dello schermo catturato.
    pub fn imposta_overlay(&mut self, zona: Option<Zona>) {
        self.overlay = zona;
    }

    fn coperta_dall_overlay(&self, indice: usize) -> bool {
        self.overlay
            .is_some_and(|o| self.candidate[indice].sovrapposta(&o))
    }

    /// Passa alla prima zona successiva non coperta dall'overlay.
    fn ruota(&mut self) -> Esito {
        let partenza = self.indice;
        for _ in 0..self.candidate.len() {
            self.indice = (self.indice + 1) % self.candidate.len();
            if !self.coperta_dall_overlay(self.indice) {
                break;
            }
        }
        if self.indice == partenza {
            // Nessuna zona libera: resto dov'ero invece di segnalare un cambio a ogni giro.
            return Esito::Niente;
        }
        self.mancanze = 0;
        self.conferme = 0;
        self.confermata = self.indice == self.preferita;
        Esito::Ruota(self.zona())
    }

    pub fn zona(&self) -> Zona {
        self.candidate[self.indice]
    }

    /// Recepisce i cambi di configurazione fatti dal menu. Restituisce `true`
    /// se la zona di cattura è cambiata (da segnalare all'interfaccia).
    pub fn aggiorna_config(&mut self, cfg: &OcrConfig) -> bool {
        if !Zona::da_config(cfg).vicina(&self.candidate[0]) {
            let zona_prima = self.zona();
            *self = Self::new(cfg);
            return !self.zona().vicina(&zona_prima);
        }
        if cfg.auto_zone == self.attiva {
            return false;
        }
        self.attiva = cfg.auto_zone;
        if cfg.auto_zone || self.indice == 0 {
            return false;
        }
        // Autorilevamento spento: si torna alla zona del file di configurazione.
        self.indice = 0;
        self.preferita = 0;
        self.confermata = true;
        self.mancanze = 0;
        self.conferme = 0;
        true
    }

    pub fn registra(&mut self, lettura: Option<Lettura>) -> Esito {
        // Se l'overlay si è spostato sopra la zona catturata, qualsiasi numero
        // letto è il nostro: cambio zona senza nemmeno guardarlo.
        if self.attiva && self.candidate.len() > 1 && self.coperta_dall_overlay(self.indice) {
            return self.ruota();
        }
        match lettura {
            Some(l) => {
                self.mancanze = 0;
                if self.confermata {
                    return Esito::Accetta(l);
                }
                self.conferme += 1;
                if l.totale.is_some() || self.conferme >= CONFERME_PER_BLOCCARE {
                    self.confermata = true;
                    self.preferita = self.indice;
                    Esito::Accetta(l)
                } else {
                    Esito::Verifica
                }
            }
            None => {
                self.conferme = 0;
                self.mancanze += 1;
                let soglia = if self.confermata {
                    MANCANZE_ZONA_CONFERMATA
                } else {
                    MANCANZE_ZONA_IN_PROVA
                };
                if !self.attiva || self.candidate.len() < 2 || self.mancanze < soglia {
                    return Esito::Niente;
                }
                self.ruota()
            }
        }
    }
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
    crate::schermo::numero_monitor()
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


    fn cfg_test() -> OcrConfig {
        OcrConfig::default()
    }

    fn lettura(slide: u32) -> Option<Lettura> {
        Some(Lettura {
            slide,
            totale: None,
        })
    }

    /// Ripete `n` letture a vuoto e restituisce l'ultimo esito.
    fn a_vuoto(s: &mut Scansione, n: u32) -> Esito {
        let mut esito = Esito::Niente;
        for _ in 0..n {
            esito = s.registra(None);
        }
        esito
    }

    #[test]
    fn le_candidate_coprono_angoli_e_centri_senza_doppioni() {
        let zone = zone_candidate(&cfg_test());
        // La zona configurata (basso a destra) coincide con una candidata: niente doppioni.
        assert_eq!(zone.len(), 6);
        assert_eq!(zone[0], Zona::da_config(&cfg_test()));
        let etichette: Vec<String> = zone.iter().map(Zona::etichetta).collect();
        assert_eq!(
            etichette,
            [
                "bottom-right",
                "bottom-left",
                "bottom-center",
                "top-right",
                "top-left",
                "top-center"
            ]
        );
        // Tutte dentro lo schermo
        for z in &zone {
            assert!(z.x >= 0.0 && z.x + z.width <= 1.0 + 1e-6);
            assert!(z.y >= 0.0 && z.y + z.height <= 1.0 + 1e-6);
        }
    }

    #[test]
    fn la_zona_configurata_non_viene_abbandonata_mentre_legge() {
        let mut s = Scansione::new(&cfg_test());
        let zona = s.zona();
        for _ in 0..50 {
            // una slide senza numero ogni tanto non fa ripartire la ricerca
            a_vuoto(&mut s, MANCANZE_ZONA_CONFERMATA - 1);
            assert_eq!(s.registra(lettura(7)), Esito::Accetta(Lettura { slide: 7, totale: None }));
        }
        assert_eq!(s.zona(), zona);
    }

    #[test]
    fn senza_letture_ruota_sulle_altre_zone() {
        let mut s = Scansione::new(&cfg_test());
        assert_eq!(a_vuoto(&mut s, MANCANZE_ZONA_CONFERMATA - 1), Esito::Niente);
        let Esito::Ruota(z1) = s.registra(None) else {
            panic!("doveva cambiare zona");
        };
        assert_eq!(z1.etichetta(), "bottom-left");
        // Le zone in prova vengono scartate molto più in fretta
        let Esito::Ruota(z2) = a_vuoto(&mut s, MANCANZE_ZONA_IN_PROVA) else {
            panic!("doveva cambiare zona");
        };
        assert_eq!(z2.etichetta(), "bottom-center");
    }

    #[test]
    fn una_zona_trovata_va_confermata_prima_di_essere_creduta() {
        let mut s = Scansione::new(&cfg_test());
        a_vuoto(&mut s, MANCANZE_ZONA_CONFERMATA);
        // Primo numero letto da una zona nuova: potrebbe essere un elenco puntato
        assert_eq!(s.registra(lettura(3)), Esito::Verifica);
        assert_eq!(
            s.registra(lettura(3)),
            Esito::Accetta(Lettura { slide: 3, totale: None })
        );
        // Ora è confermata: le letture successive passano subito
        assert_eq!(
            s.registra(lettura(4)),
            Esito::Accetta(Lettura { slide: 4, totale: None })
        );
    }

    #[test]
    fn un_contatore_con_totale_conferma_subito_la_zona() {
        let mut s = Scansione::new(&cfg_test());
        a_vuoto(&mut s, MANCANZE_ZONA_CONFERMATA);
        let l = Lettura { slide: 3, totale: Some(30) };
        assert_eq!(s.registra(Some(l)), Esito::Accetta(l));
    }

    #[test]
    fn tornando_sulla_zona_preferita_le_letture_passano_subito() {
        let mut s = Scansione::new(&cfg_test());
        let configurata = s.zona();
        a_vuoto(&mut s, MANCANZE_ZONA_CONFERMATA);
        assert_ne!(s.zona(), configurata);
        // Giro completo: si torna alla zona configurata, che era quella preferita
        while s.zona() != configurata {
            a_vuoto(&mut s, MANCANZE_ZONA_IN_PROVA);
        }
        assert_eq!(
            s.registra(lettura(5)),
            Esito::Accetta(Lettura { slide: 5, totale: None })
        );
    }

    #[test]
    fn con_auto_zone_disattivato_la_zona_non_cambia_mai() {
        let cfg = OcrConfig {
            auto_zone: false,
            ..OcrConfig::default()
        };
        let mut s = Scansione::new(&cfg);
        let zona = s.zona();
        assert_eq!(a_vuoto(&mut s, 500), Esito::Niente);
        assert_eq!(s.zona(), zona);
    }

    #[test]
    fn spegnere_l_autorilevamento_riporta_alla_zona_configurata() {
        let mut s = Scansione::new(&cfg_test());
        let configurata = s.zona();
        a_vuoto(&mut s, MANCANZE_ZONA_CONFERMATA);
        assert_ne!(s.zona(), configurata);

        let cfg_off = OcrConfig {
            auto_zone: false,
            ..OcrConfig::default()
        };
        assert!(s.aggiorna_config(&cfg_off));
        assert_eq!(s.zona(), configurata);
        // e la riaccensione non sposta nulla finché la zona legge
        let cfg_on = cfg_test();
        assert!(!s.aggiorna_config(&cfg_on));
        assert_eq!(s.zona(), configurata);
    }

    #[test]
    fn cambiare_zona_nel_config_fa_ripartire_la_scansione() {
        let mut s = Scansione::new(&cfg_test());
        a_vuoto(&mut s, MANCANZE_ZONA_CONFERMATA);
        let nuovo = OcrConfig {
            x: 0.0,
            y: 0.0,
            ..OcrConfig::default()
        };
        assert!(s.aggiorna_config(&nuovo));
        assert_eq!(s.zona(), Zona::da_config(&nuovo));
        assert_eq!(s.zona().etichetta(), "top-left");
    }

#[test]
    fn la_zona_coperta_dall_overlay_viene_abbandonata_subito() {
        let mut s = Scansione::new(&cfg_test());
        let configurata = s.zona();
        // L'utente trascina il Ritardometro sopra il contatore: quello che si
        // legge lì è il numero di slide scritto dall'overlay stesso.
        s.imposta_overlay(Some(Zona {
            x: 0.8,
            y: 0.8,
            width: 0.2,
            height: 0.2,
        }));
        let finto = Lettura {
            slide: 12,
            totale: Some(30),
        };
        let Esito::Ruota(nuova) = s.registra(Some(finto)) else {
            panic!("doveva scartare la zona coperta dall'overlay");
        };
        assert_ne!(nuova, configurata);
        assert_eq!(nuova.etichetta(), "bottom-left");
        // e la rotazione successiva non ci ritorna sopra
        for _ in 0..12 {
            if let Esito::Ruota(z) = a_vuoto(&mut s, MANCANZE_ZONA_IN_PROVA) {
                assert!(!z.sovrapposta(&Zona {
                    x: 0.8,
                    y: 0.8,
                    width: 0.2,
                    height: 0.2
                }));
            }
        }
    }

    #[test]
    fn senza_autorilevamento_l_overlay_non_sposta_la_zona() {
        let cfg = OcrConfig {
            auto_zone: false,
            ..OcrConfig::default()
        };
        let mut s = Scansione::new(&cfg);
        let zona = s.zona();
        s.imposta_overlay(Some(Zona {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }));
        assert!(matches!(s.registra(lettura(4)), Esito::Accetta(_)));
        assert_eq!(s.zona(), zona);
    }

    #[test]
    fn zone_sovrapposte_e_no() {
        let a = Zona {
            x: 0.85,
            y: 0.88,
            width: 0.15,
            height: 0.12,
        };
        assert!(a.sovrapposta(&a));
        assert!(a.sovrapposta(&Zona {
            x: 0.9,
            y: 0.9,
            width: 0.1,
            height: 0.1
        }));
        // angolo opposto: nessuna sovrapposizione
        assert!(!a.sovrapposta(&Zona {
            x: 0.0,
            y: 0.0,
            width: 0.15,
            height: 0.12
        }));
        // overlay su un altro monitor (coordinate negative)
        assert!(!a.sovrapposta(&Zona {
            x: -1.0,
            y: 0.88,
            width: 0.15,
            height: 0.12
        }));
    }

#[test]
    fn se_nessuna_zona_e_libera_la_scansione_sta_ferma() {
        let mut s = Scansione::new(&cfg_test());
        let zona = s.zona();
        s.imposta_overlay(Some(Zona {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }));
        for _ in 0..10 {
            assert_eq!(s.registra(lettura(4)), Esito::Niente);
        }
        assert_eq!(s.zona(), zona);
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
