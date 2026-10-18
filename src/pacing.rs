//! Logica pura: cronometro, calcolo di anticipo/ritardo e milestone.
//! Niente grafica qui dentro, così è tutto testabile.

use crate::config::MilestoneConfig;
use std::time::{Duration, Instant};

// ------------------------------------------------------------------
// Cronometro con pausa
// ------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Cronometro {
    partito: Option<Instant>,
    accumulato: Duration,
    mai_avviato: bool,
}

impl Cronometro {
    pub fn new() -> Self {
        Self {
            mai_avviato: true,
            ..Default::default()
        }
    }

    pub fn avvia(&mut self) {
        if self.partito.is_none() {
            self.partito = Some(Instant::now());
            self.mai_avviato = false;
        }
    }

    pub fn pausa(&mut self) {
        if let Some(p) = self.partito.take() {
            self.accumulato += p.elapsed();
        }
    }

    pub fn alterna(&mut self) {
        if self.in_corso() {
            self.pausa();
        } else {
            self.avvia();
        }
    }

    pub fn azzera(&mut self) {
        *self = Self::new();
    }

    pub fn in_corso(&self) -> bool {
        self.partito.is_some()
    }

    pub fn mai_avviato(&self) -> bool {
        self.mai_avviato
    }

    pub fn trascorsi(&self) -> f32 {
        let corrente = self.partito.map(|p| p.elapsed()).unwrap_or_default();
        (self.accumulato + corrente).as_secs_f32()
    }

    /// Sposta il tempo avanti/indietro (es. hai fatto partire il timer in ritardo).
    pub fn correggi(&mut self, secondi: f32) {
        let nuovo = (self.trascorsi() + secondi).max(0.0);
        let attivo = self.in_corso();
        self.accumulato = Duration::from_secs_f32(nuovo);
        self.partito = attivo.then(Instant::now);
        self.mai_avviato = false;
    }
}

// ------------------------------------------------------------------
// Anticipo / ritardo
// ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ritmo {
    InLinea,
    Anticipo,
    Ritardo,
    Panico,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Andatura {
    /// Secondi di scarto: positivo = sei in anticipo, negativo = in ritardo.
    pub scarto_secondi: f32,
    /// Slide su cui dovresti essere adesso.
    pub slide_attesa: u32,
    pub ritmo: Ritmo,
}

/// Confronta la slide corrente con il tempo trascorso.
///
/// Ogni slide "vale" `durata / totale` secondi: stare sulla slide `n`
/// significa aver completato `n - 1` slide.
pub fn andatura(
    trascorsi: f32,
    durata: f32,
    slide: u32,
    totale: u32,
    tolleranza: f32,
    panico: f32,
) -> Option<Andatura> {
    if totale == 0 || slide == 0 || durata <= 0.0 {
        return None;
    }
    let slide = slide.min(totale);
    let per_slide = durata / totale as f32;
    let fatte = (slide - 1) as f32;
    let attese = trascorsi / per_slide;
    let scarto = (fatte - attese) * per_slide;
    let slide_attesa = ((attese.floor() as u32) + 1).min(totale);

    let ritmo = if scarto.abs() <= tolleranza {
        Ritmo::InLinea
    } else if scarto > 0.0 {
        Ritmo::Anticipo
    } else if -scarto >= panico {
        Ritmo::Panico
    } else {
        Ritmo::Ritardo
    };

    Some(Andatura {
        scarto_secondi: scarto,
        slide_attesa,
        ritmo,
    })
}

// ------------------------------------------------------------------
// Milestone (bandierine)
// ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Milestone {
    pub secondi: f32,
    pub testo: String,
}

pub fn risolvi_milestone(cfg: &[MilestoneConfig], durata: f32) -> Vec<Milestone> {
    let mut v: Vec<Milestone> = cfg
        .iter()
        .filter_map(|m| {
            let secondi = match (m.percentuale, m.mancano_minuti) {
                (Some(p), _) => durata * p / 100.0,
                (None, Some(min)) => durata - min * 60.0,
                (None, None) => return None,
            };
            (secondi > 0.0 && secondi < durata).then(|| Milestone {
                secondi,
                testo: m.testo.clone(),
            })
        })
        .collect();
    v.sort_by(|a, b| a.secondi.total_cmp(&b.secondi));
    v
}

/// Tiene traccia di quali bandierine sono già state sventolate.
///
/// Lo scatto avviene sul tempo del talk, ma l'animazione dura secondi
/// "veri": così si vede anche se il timer è in pausa o in modalità demo.
#[derive(Debug)]
pub struct Bandierine {
    lista: Vec<Milestone>,
    /// (indice, istante reale di inizio animazione)
    attiva: Option<(usize, f32)>,
    ultimo: f32,
    reale: f32,
}

impl Bandierine {
    pub const DURATA_ANIMAZIONE: f32 = 8.0;

    pub fn new(lista: Vec<Milestone>) -> Self {
        Self {
            lista,
            attiva: None,
            ultimo: 0.0,
            reale: 0.0,
        }
    }

    pub fn azzera(&mut self) {
        self.attiva = None;
        self.ultimo = 0.0;
    }

    /// Da chiamare a ogni frame con il tempo del talk e un orologio reale (secondi).
    pub fn aggiorna(&mut self, trascorsi: f32, reale: f32) {
        self.reale = reale;
        for (i, m) in self.lista.iter().enumerate() {
            if self.ultimo < m.secondi && m.secondi <= trascorsi {
                self.attiva = Some((i, reale));
            }
        }
        if let Some((_, inizio)) = self.attiva {
            if reale - inizio > Self::DURATA_ANIMAZIONE {
                self.attiva = None;
            }
        }
        self.ultimo = trascorsi;
    }

    /// Bandierina da mostrare ora, con il progresso dell'animazione (0..1).
    pub fn corrente(&self) -> Option<(&str, f32)> {
        self.attiva.map(|(i, inizio)| {
            let p = ((self.reale - inizio) / Self::DURATA_ANIMAZIONE).clamp(0.0, 1.0);
            (self.lista[i].testo.as_str(), p)
        })
    }
}

// ------------------------------------------------------------------
// Formattazione
// ------------------------------------------------------------------

/// `754.2` -> `"12:34"`, gestisce anche le ore.
pub fn mm_ss(secondi: f32) -> String {
    let s = secondi.abs().floor() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// `80.0` -> `"+1:20"`, `-125.0` -> `"-2:05"`.
pub fn scarto(secondi: f32) -> String {
    let segno = if secondi >= 0.0 { '+' } else { '-' };
    let s = secondi.abs().round() as u64;
    format!("{segno}{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: f32 = 30.0 * 60.0;

    #[test]
    fn una_slide_al_minuto_in_linea() {
        // minuto 10 e mezzo, slide 11: perfetto
        let a = andatura(630.0, D, 11, 30, 60.0, 180.0).unwrap();
        assert_eq!(a.ritmo, Ritmo::InLinea);
        assert_eq!(a.slide_attesa, 11);
        assert!((a.scarto_secondi - (-30.0)).abs() < 0.01);
    }

    #[test]
    fn in_ritardo_e_panico() {
        // minuto 15, ancora slide 14: 2 minuti indietro
        let a = andatura(900.0, D, 14, 30, 60.0, 180.0).unwrap();
        assert_eq!(a.ritmo, Ritmo::Ritardo);
        assert!((a.scarto_secondi + 120.0).abs() < 0.01);
        // minuto 20, slide 15: 6 minuti indietro
        let a = andatura(1200.0, D, 15, 30, 60.0, 180.0).unwrap();
        assert_eq!(a.ritmo, Ritmo::Panico);
    }

    #[test]
    fn in_anticipo() {
        // minuto 5, già alla slide 9
        let a = andatura(300.0, D, 9, 30, 60.0, 180.0).unwrap();
        assert_eq!(a.ritmo, Ritmo::Anticipo);
        assert!((a.scarto_secondi - 180.0).abs() < 0.01);
    }

    #[test]
    fn slide_non_uguali_ai_minuti() {
        // 40 slide in 20 minuti: 30 secondi a slide
        let a = andatura(600.0, 1200.0, 21, 40, 60.0, 180.0).unwrap();
        assert!(a.scarto_secondi.abs() < 0.01);
        assert_eq!(a.slide_attesa, 21);
    }

    #[test]
    fn dati_mancanti() {
        assert!(andatura(10.0, D, 0, 30, 60.0, 180.0).is_none());
        assert!(andatura(10.0, D, 3, 0, 60.0, 180.0).is_none());
    }

    #[test]
    fn milestone_meta_e_cinque_minuti() {
        let m = risolvi_milestone(&crate::config::Config::default().milestone, D);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].secondi, 900.0);
        assert_eq!(m[1].secondi, 1500.0);
    }

    #[test]
    fn la_bandierina_sventola_una_volta() {
        let mut b = Bandierine::new(vec![Milestone {
            secondi: 10.0,
            testo: "X".into(),
        }]);
        b.aggiorna(9.0, 100.0);
        assert!(b.corrente().is_none());
        // scatta anche se il tempo del talk salta avanti (demo, correzione +1 min)
        b.aggiorna(70.0, 100.5);
        assert_eq!(b.corrente().map(|c| c.0), Some("X"));
        // dura 8 secondi reali, anche se il timer è in pausa
        b.aggiorna(70.0, 107.0);
        assert!(b.corrente().is_some());
        b.aggiorna(70.0, 109.0);
        assert!(b.corrente().is_none());
        // non si ripete
        b.aggiorna(80.0, 110.0);
        assert!(b.corrente().is_none());
        // dopo un azzeramento può ripartire
        b.azzera();
        b.aggiorna(11.0, 120.0);
        assert!(b.corrente().is_some());
    }

    #[test]
    fn formattazione() {
        assert_eq!(mm_ss(754.9), "12:34");
        assert_eq!(mm_ss(-61.0), "01:01");
        assert_eq!(mm_ss(3725.0), "1:02:05");
        assert_eq!(scarto(80.0), "+1:20");
        assert_eq!(scarto(-125.0), "-2:05");
    }

    #[test]
    fn correzione_del_tempo() {
        let mut c = Cronometro::new();
        c.correggi(90.0);
        assert!((c.trascorsi() - 90.0).abs() < 0.01);
        c.correggi(-200.0);
        assert_eq!(c.trascorsi(), 0.0);
        assert!(!c.in_corso());
    }
}
