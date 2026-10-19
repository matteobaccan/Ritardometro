//! Configurazione letta da `ritardometro.toml`.
//!
//! Se il file non esiste viene creato accanto all'eseguibile con i valori
//! di default e i commenti, così è facile da modificare prima di un evento.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// File di configurazione di esempio, scritto su disco al primo avvio.
/// È lo stesso `ritardometro.toml` che si trova nel repository.
pub const TEMPLATE: &str = include_str!("../ritardometro.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub durata_minuti: f32,
    pub domande_minuti: f32,
    pub totale_slide: u32,
    pub avvio_automatico: bool,
    pub tolleranza_secondi: f32,
    pub panico_secondi: f32,
    pub scala: f32,
    pub opacita_sfondo: f32,
    pub opacita: f32,
    pub posizione: Posizione,
    pub margine: f32,
    pub ocr: OcrConfig,
    pub milestone: Vec<MilestoneConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Posizione {
    AltoSinistra,
    AltoDestra,
    BassoSinistra,
    BassoDestra,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OcrConfig {
    pub attivo: bool,
    pub monitor: usize,
    pub x: f32,
    pub y: f32,
    pub larghezza: f32,
    pub altezza: f32,
    pub intervallo_ms: u64,
    pub ingrandimento: u32,
    pub salva_debug: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MilestoneConfig {
    pub percentuale: Option<f32>,
    pub mancano_minuti: Option<f32>,
    pub testo: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            durata_minuti: 30.0,
            domande_minuti: 10.0,
            totale_slide: 30,
            avvio_automatico: true,
            tolleranza_secondi: 60.0,
            panico_secondi: 180.0,
            scala: 1.0,
            opacita_sfondo: 0.70,
            opacita: 1.0,
            posizione: Posizione::AltoDestra,
            margine: 24.0,
            ocr: OcrConfig::default(),
            milestone: vec![
                MilestoneConfig {
                    percentuale: Some(50.0),
                    mancano_minuti: None,
                    testo: "HALF WAY!".into(),
                },
                MilestoneConfig {
                    percentuale: None,
                    mancano_minuti: Some(5.0),
                    testo: "5 MIN LEFT".into(),
                },
            ],
        }
    }
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            attivo: true,
            monitor: 0,
            x: 0.85,
            y: 0.88,
            larghezza: 0.15,
            altezza: 0.12,
            intervallo_ms: 250,
            ingrandimento: 3,
            salva_debug: false,
        }
    }
}

impl Config {
    pub fn durata_secondi(&self) -> f32 {
        self.durata_minuti * 60.0
    }

    /// Corregge valori impossibili invece di far fallire l'avvio sul palco.
    fn normalizza(mut self) -> Self {
        self.durata_minuti = self.durata_minuti.max(1.0);
        self.domande_minuti = self.domande_minuti.max(0.0);
        self.tolleranza_secondi = self.tolleranza_secondi.max(0.0);
        self.panico_secondi = self.panico_secondi.max(self.tolleranza_secondi + 1.0);
        self.scala = self.scala.clamp(0.5, 3.0);
        self.opacita_sfondo = self.opacita_sfondo.clamp(0.0, 1.0);
        self.opacita = self.opacita.clamp(0.15, 1.0);
        let o = &mut self.ocr;
        o.x = o.x.clamp(0.0, 0.99);
        o.y = o.y.clamp(0.0, 0.99);
        // min/max invece di clamp: con x = 0.99 il limite superiore può
        // risultare più piccolo di quello inferiore per gli arrotondamenti.
        o.larghezza = o.larghezza.min(1.0 - o.x).max(0.005);
        o.altezza = o.altezza.min(1.0 - o.y).max(0.005);
        o.intervallo_ms = o.intervallo_ms.max(100);
        o.ingrandimento = o.ingrandimento.clamp(1, 6);
        self
    }
}

/// Esito del caricamento: la config da usare più un eventuale avviso da mostrare.
pub struct Caricata {
    pub config: Config,
    pub percorso: PathBuf,
    pub avviso: Option<String>,
}

/// Percorso di default: `ritardometro.toml` accanto all'eseguibile.
pub fn percorso_default() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("ritardometro.toml")))
        .unwrap_or_else(|| PathBuf::from("ritardometro.toml"))
}

pub fn da_testo(testo: &str) -> Result<Config, String> {
    toml::from_str::<Config>(testo)
        .map(Config::normalizza)
        .map_err(|e| e.to_string())
}

/// Carica la configurazione. Non fallisce mai: in caso di problemi usa i
/// default e restituisce un avviso da mostrare nell'overlay.
pub fn carica(percorso: &Path) -> Caricata {
    match std::fs::read_to_string(percorso) {
        Ok(testo) => match da_testo(&testo) {
            Ok(config) => Caricata {
                config,
                percorso: percorso.to_path_buf(),
                avviso: None,
            },
            Err(e) => Caricata {
                config: Config::default(),
                percorso: percorso.to_path_buf(),
                avviso: Some(format!(
                    "Error in config file, falling back to defaults:\n{e}"
                )),
            },
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let avviso = std::fs::write(percorso, TEMPLATE)
                .err()
                .map(|e| format!("Could not create {}: {e}", percorso.display()));
            Caricata {
                config: Config::default(),
                percorso: percorso.to_path_buf(),
                avviso,
            }
        }
        Err(e) => Caricata {
            config: Config::default(),
                percorso: percorso.to_path_buf(),
                avviso: Some(format!("Could not read {}: {e}", percorso.display())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn il_template_corrisponde_ai_default() {
        let da_file = da_testo(TEMPLATE).expect("il template deve essere valido");
        let def = Config::default().normalizza();
        assert_eq!(da_file.durata_minuti, def.durata_minuti);
        assert_eq!(da_file.totale_slide, def.totale_slide);
        assert_eq!(da_file.posizione, def.posizione);
        assert_eq!(da_file.ocr.x, def.ocr.x);
        assert_eq!(da_file.ocr.intervallo_ms, def.ocr.intervallo_ms);
        assert_eq!(da_file.milestone, def.milestone);
    }

    #[test]
    fn file_parziale_usa_i_default() {
        let c = da_testo("durata_minuti = 45\n").unwrap();
        assert_eq!(c.durata_minuti, 45.0);
        assert_eq!(c.totale_slide, 30);
        assert!(c.ocr.attivo);
    }

    #[test]
    fn chiave_sconosciuta_e_un_errore() {
        assert!(da_testo("durata_minuto = 45\n").is_err());
    }

    #[test]
    fn valori_assurdi_vengono_corretti() {
        let c = da_testo("durata_minuti = 0\nscala = 100\n[ocr]\nx = 2.0\n").unwrap();
        assert_eq!(c.durata_minuti, 1.0);
        assert_eq!(c.scala, 3.0);
        assert!(c.ocr.x + c.ocr.larghezza <= 1.0 + 1e-6);
        assert!(c.ocr.larghezza > 0.0);
    }
}
