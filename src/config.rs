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
    #[serde(alias = "durata_minuti")]
    pub duration_minutes: f32,
    #[serde(alias = "domande_minuti")]
    pub questions_minutes: f32,
    #[serde(alias = "totale_slide")]
    pub total_slides: u32,
    #[serde(alias = "avvio_automatico")]
    pub auto_start: bool,
    #[serde(alias = "tolleranza_secondi")]
    pub tolerance_seconds: f32,
    #[serde(alias = "panico_secondi")]
    pub panic_seconds: f32,
    /// Presentazioni senza numerazione: multiplo del tempo medio per slide dopo
    /// il quale, se la schermata non cambia, l'avatar si tocca il polso.
    /// `0` disattiva del tutto la sorveglianza dello schermo.
    pub stuck_factor: f32,
    #[serde(alias = "scala")]
    pub scale: f32,
    #[serde(alias = "opacita_sfondo")]
    pub background_opacity: f32,
    #[serde(alias = "opacita")]
    pub opacity: f32,
    #[serde(alias = "posizione")]
    pub position: Posizione,
    #[serde(alias = "margine")]
    pub margin: f32,
    pub ocr: OcrConfig,
    pub milestone: Vec<MilestoneConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Posizione {
    #[serde(alias = "alto-sinistra", alias = "top-left")]
    TopLeft,
    #[serde(alias = "alto-destra", alias = "top-right")]
    TopRight,
    #[serde(alias = "basso-sinistra", alias = "bottom-left")]
    BottomLeft,
    #[serde(alias = "basso-destra", alias = "bottom-right")]
    BottomRight,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OcrConfig {
    #[serde(alias = "attivo")]
    pub enabled: bool,
    pub monitor: usize,
    /// Se la zona configurata non legge nulla, cerca il contatore negli altri angoli.
    #[serde(alias = "zona_automatica")]
    pub auto_zone: bool,
    pub x: f32,
    pub y: f32,
    #[serde(alias = "larghezza")]
    pub width: f32,
    #[serde(alias = "altezza")]
    pub height: f32,
    #[serde(alias = "intervallo_ms")]
    pub interval_ms: u64,
    #[serde(alias = "ingrandimento")]
    pub magnification: u32,
    #[serde(alias = "salva_debug")]
    pub save_debug: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MilestoneConfig {
    #[serde(alias = "percentuale")]
    pub percentage: Option<f32>,
    #[serde(alias = "mancano_minuti")]
    pub minutes_left: Option<f32>,
    #[serde(alias = "testo")]
    pub text: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            duration_minutes: 30.0,
            questions_minutes: 10.0,
            total_slides: 30,
            auto_start: true,
            tolerance_seconds: 60.0,
            panic_seconds: 180.0,
            stuck_factor: 1.5,
            scale: 1.0,
            background_opacity: 0.70,
            opacity: 1.0,
            position: Posizione::TopRight,
            margin: 24.0,
            ocr: OcrConfig::default(),
            milestone: vec![
                MilestoneConfig {
                    percentage: Some(50.0),
                    minutes_left: None,
                    text: "HALF WAY!".into(),
                },
                MilestoneConfig {
                    percentage: None,
                    minutes_left: Some(5.0),
                    text: "5 MIN LEFT".into(),
                },
            ],
        }
    }
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            monitor: 0,
            auto_zone: true,
            x: 0.85,
            y: 0.88,
            width: 0.15,
            height: 0.12,
            interval_ms: 250,
            magnification: 3,
            save_debug: false,
        }
    }
}

impl Config {
    pub fn durata_secondi(&self) -> f32 {
        self.duration_minutes * 60.0
    }

    /// Corregge valori impossibili invece di far fallire l'avvio sul palco.
    fn normalizza(mut self) -> Self {
        self.duration_minutes = self.duration_minutes.max(1.0);
        self.questions_minutes = self.questions_minutes.max(0.0);
        self.tolerance_seconds = self.tolerance_seconds.max(0.0);
        self.panic_seconds = self.panic_seconds.max(self.tolerance_seconds + 1.0);
        // Sotto il tempo medio per slide il gesto scatterebbe su ogni slide lunga.
        self.stuck_factor = if self.stuck_factor <= 0.0 {
            0.0
        } else {
            self.stuck_factor.max(1.0)
        };
        self.scale = self.scale.clamp(0.5, 3.0);
        self.background_opacity = self.background_opacity.clamp(0.0, 1.0);
        self.opacity = self.opacity.clamp(0.15, 1.0);
        let o = &mut self.ocr;
        o.x = o.x.clamp(0.0, 0.99);
        o.y = o.y.clamp(0.0, 0.99);
        // min/max invece di clamp: con x = 0.99 il limite superiore può
        // risultare più piccolo di quello inferiore per gli arrotondamenti.
        o.width = o.width.min(1.0 - o.x).max(0.005);
        o.height = o.height.min(1.0 - o.y).max(0.005);
        o.interval_ms = o.interval_ms.max(100);
        o.magnification = o.magnification.clamp(1, 6);
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
        assert_eq!(da_file.duration_minutes, def.duration_minutes);
        assert_eq!(da_file.total_slides, def.total_slides);
        assert_eq!(da_file.stuck_factor, def.stuck_factor);
        assert_eq!(da_file.position, def.position);
        assert_eq!(da_file.ocr.x, def.ocr.x);
        assert_eq!(da_file.ocr.auto_zone, def.ocr.auto_zone);
        assert_eq!(da_file.ocr.interval_ms, def.ocr.interval_ms);
        assert_eq!(da_file.milestone, def.milestone);
    }

    #[test]
    fn file_parziale_usa_i_default() {
        let c = da_testo("duration_minutes = 45\n").unwrap();
        assert_eq!(c.duration_minutes, 45.0);
        assert_eq!(c.total_slides, 30);
        assert!(c.ocr.enabled);

        // Verifica retrocompatibilità parametri in italiano (alias)
        let c_it = da_testo("durata_minuti = 45\ntotale_slide = 25\n").unwrap();
        assert_eq!(c_it.duration_minutes, 45.0);
        assert_eq!(c_it.total_slides, 25);
    }

    #[test]
    fn chiave_sconosciuta_e_un_errore() {
        assert!(da_testo("durata_minuto = 45\n").is_err());
        assert!(da_testo("unknown_key = 45\n").is_err());
    }

    #[test]
    fn il_fattore_di_stallo_resta_sensato() {
        assert_eq!(da_testo("stuck_factor = 0\n").unwrap().stuck_factor, 0.0);
        assert_eq!(da_testo("stuck_factor = -3\n").unwrap().stuck_factor, 0.0);
        // Sotto 1 il gesto scatterebbe prima del tempo medio di una slide
        assert_eq!(da_testo("stuck_factor = 0.2\n").unwrap().stuck_factor, 1.0);
        assert_eq!(da_testo("stuck_factor = 2.5\n").unwrap().stuck_factor, 2.5);
    }

    #[test]
    fn valori_assurdi_vengono_corretti() {
        let c = da_testo("duration_minutes = 0\nscale = 100\n[ocr]\nx = 2.0\n").unwrap();
        assert_eq!(c.duration_minutes, 1.0);
        assert_eq!(c.scale, 3.0);
        assert!(c.ocr.x + c.ocr.width <= 1.0 + 1e-6);
        assert!(c.ocr.width > 0.0);
    }
}
