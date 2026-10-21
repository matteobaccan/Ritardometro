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
    /// Posizione della finestra all'ultima chiusura: la scrive il programma,
    /// non l'utente. Assente = si parte dall'angolo indicato da `position`.
    pub window: Option<FinestraSalvata>,
    pub milestone: Vec<MilestoneConfig>,
}

/// Angolo in alto a sinistra della finestra, in pixel fisici dello schermo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FinestraSalvata {
    pub x: i32,
    pub y: i32,
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
            window: None,
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
    /// Falso quando il file esiste ma non si è riusciti a leggerlo o a
    /// interpretarlo: riscriverlo cancellerebbe la configurazione dell'utente.
    pub salvabile: bool,
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
                salvabile: true,
            },
            Err(e) => Caricata {
                config: Config::default(),
                percorso: percorso.to_path_buf(),
                avviso: Some(format!(
                    "Error in config file, falling back to defaults:\n{e}"
                )),
                salvabile: false,
            },
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let avviso = std::fs::write(percorso, TEMPLATE)
                .err()
                .map(|e| format!("Could not create {}: {e}", percorso.display()));
            Caricata {
                config: Config::default(),
                percorso: percorso.to_path_buf(),
                avviso: avviso.clone(),
                salvabile: avviso.is_none(),
            }
        }
        Err(e) => Caricata {
            config: Config::default(),
            percorso: percorso.to_path_buf(),
            avviso: Some(format!("Could not read {}: {e}", percorso.display())),
            salvabile: false,
        },
    }
}

// ------------------------------------------------------------------
// Salvataggio delle impostazioni cambiate dal menu
// ------------------------------------------------------------------

/// Riscrive nel file solo le chiavi indicate, lasciando intatto tutto il resto.
///
/// Non si ri-serializza l'intera struttura: il `ritardometro.toml` è pieno di
/// commenti che spiegano ogni parametro, e sono la documentazione del programma.
fn imposta_chiave(testo: &str, sezione: Option<&str>, chiave: &str, valore: &str) -> String {
    let intestazione = sezione.map(|s| format!("[{s}]"));
    let mut righe: Vec<String> = testo.lines().map(str::to_string).collect();
    let mut dentro = sezione.is_none();
    let mut fine_sezione = None;

    for i in 0..righe.len() {
        let riga = righe[i].trim();
        if riga.starts_with('[') {
            // Una nuova intestazione chiude la sezione precedente
            if dentro {
                fine_sezione = Some(i);
                dentro = false;
                break;
            }
            dentro = intestazione.as_deref() == Some(riga);
            continue;
        }
        if !dentro {
            continue;
        }
        let Some((prima, _)) = riga.split_once('=') else {
            continue;
        };
        if prima.trim() == chiave {
            righe[i] = format!("{chiave} = {valore}");
            return righe.join("\n") + "\n";
        }
    }

    // Chiave assente: la aggiungo in fondo alla sua sezione, saltando le righe vuote
    let mut punto = fine_sezione.unwrap_or(righe.len());
    while punto > 0 && righe[punto - 1].trim().is_empty() {
        punto -= 1;
    }
    if !dentro && fine_sezione.is_none() {
        // Manca proprio la sezione: la creo in fondo al file
        if let Some(h) = intestazione {
            righe.push(String::new());
            righe.push(h);
            righe.push(format!("{chiave} = {valore}"));
            return righe.join("\n") + "\n";
        }
    }
    righe.insert(punto, format!("{chiave} = {valore}"));
    righe.join("\n") + "\n"
}

fn numero(v: f32) -> String {
    format!("{v:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// Applica al testo del file tutte le impostazioni modificabili dal menu.
pub fn aggiorna_testo(testo: &str, cfg: &Config) -> String {
    let mut t = testo.to_string();
    for (sezione, chiave, valore) in [
        (None, "scale", numero(cfg.scale)),
        (None, "background_opacity", numero(cfg.background_opacity)),
        (None, "opacity", numero(cfg.opacity)),
        (Some("ocr"), "enabled", cfg.ocr.enabled.to_string()),
        (Some("ocr"), "monitor", cfg.ocr.monitor.to_string()),
        (Some("ocr"), "auto_zone", cfg.ocr.auto_zone.to_string()),
        (Some("ocr"), "interval_ms", cfg.ocr.interval_ms.to_string()),
        (Some("ocr"), "save_debug", cfg.ocr.save_debug.to_string()),
    ] {
        t = imposta_chiave(&t, sezione, chiave, &valore);
    }
    if let Some(f) = cfg.window {
        t = imposta_chiave(&t, Some("window"), "x", &f.x.to_string());
        t = imposta_chiave(&t, Some("window"), "y", &f.y.to_string());
    }
    t
}

/// Salva le impostazioni nel file di configurazione, conservandone i commenti.
///
/// Rilegge il file appena prima di scrivere, così una modifica fatta a mano
/// mentre il programma è aperto non viene persa (vince comunque chi scrive per
/// ultimo sulle chiavi in conflitto), e scrive su un file temporaneo che poi
/// rinomina: un'interruzione a metà non lascia una configurazione monca.
pub fn salva(percorso: &Path, cfg: &Config) -> Result<(), String> {
    let attuale = std::fs::read_to_string(percorso).unwrap_or_else(|_| TEMPLATE.to_string());
    let nuovo = aggiorna_testo(&attuale, cfg);
    if nuovo == attuale {
        return Ok(());
    }
    let temporaneo = percorso.with_extension("toml.tmp");
    std::fs::write(&temporaneo, &nuovo)
        .map_err(|e| format!("Could not write {}: {e}", temporaneo.display()))?;
    std::fs::rename(&temporaneo, percorso)
        .map_err(|e| format!("Could not update {}: {e}", percorso.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

#[test]
    fn salvare_non_tocca_commenti_e_altre_chiavi() {
        let c = Config {
            scale: 1.3,
            ocr: OcrConfig {
                enabled: false,
                monitor: 2,
                ..OcrConfig::default()
            },
            ..Config::default()
        };
        let nuovo = aggiorna_testo(TEMPLATE, &c);

        // I commenti del template restano tutti
        for riga in TEMPLATE.lines().filter(|r| r.trim_start().starts_with('#')) {
            assert!(nuovo.contains(riga), "commento perso: {riga}");
        }
        assert!(nuovo.contains("scale = 1.3"));
        assert!(nuovo.contains("enabled = false"));
        assert!(nuovo.contains("monitor = 2"));
        // Le chiavi che il menu non tocca restano come stavano
        assert!(nuovo.contains("duration_minutes = 30"));
        assert!(nuovo.contains("text = \"HALF WAY!\""));

        // E il risultato si rilegge dando esattamente i valori salvati
        let riletta = da_testo(&nuovo).unwrap();
        assert_eq!(riletta.scale, 1.3);
        assert!(!riletta.ocr.enabled);
        assert_eq!(riletta.ocr.monitor, 2);
        assert_eq!(riletta.duration_minutes, 30.0);
        assert_eq!(riletta.milestone, Config::default().milestone);
    }

    #[test]
    fn la_chiave_giusta_nella_sezione_giusta() {
        // `enabled` esiste solo dentro [ocr]: una chiave omonima in cima al
        // file non deve essere confusa con quella della sezione.
        let testo = "enabled = true\n\n[ocr]\nenabled = true\n";
        let fuori = imposta_chiave(testo, None, "enabled", "false");
        assert_eq!(fuori, "enabled = false\n\n[ocr]\nenabled = true\n");
        let dentro = imposta_chiave(testo, Some("ocr"), "enabled", "false");
        assert_eq!(dentro, "enabled = true\n\n[ocr]\nenabled = false\n");
    }

    #[test]
    fn una_chiave_mancante_viene_aggiunta_nella_sua_sezione() {
        let testo = "duration_minutes = 30\n\n[ocr]\nmonitor = 0\n\n[[milestone]]\ntext = \"X\"\n";
        let t = imposta_chiave(testo, Some("ocr"), "auto_zone", "false");
        assert_eq!(
            t,
            "duration_minutes = 30\n\n[ocr]\nmonitor = 0\nauto_zone = false\n\n[[milestone]]\ntext = \"X\"\n"
        );
        // Una chiave di primo livello finisce prima della prima sezione
        let t = imposta_chiave(testo, None, "scale", "1.5");
        assert!(t.starts_with("duration_minutes = 30\nscale = 1.5\n"));
        assert!(da_testo(&t).is_ok());
    }

    #[test]
    fn la_sezione_window_viene_creata_e_riletta() {
        let c = Config {
            window: Some(FinestraSalvata { x: 1920, y: -40 }),
            ..Config::default()
        };
        let t = aggiorna_testo(TEMPLATE, &c);
        assert!(t.contains("[window]"));
        let riletta = da_testo(&t).unwrap();
        assert_eq!(riletta.window, Some(FinestraSalvata { x: 1920, y: -40 }));

        // Salvare una seconda volta aggiorna i valori senza duplicare la sezione
        let c2 = Config {
            window: Some(FinestraSalvata { x: 10, y: 20 }),
            ..riletta
        };
        let t2 = aggiorna_testo(&t, &c2);
        assert_eq!(t2.matches("[window]").count(), 1);
        assert_eq!(
            da_testo(&t2).unwrap().window,
            Some(FinestraSalvata { x: 10, y: 20 })
        );
    }

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
