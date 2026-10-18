//! La finestra overlay: mette insieme timer, OCR, avatar e controlli.

use crate::avatar::{self, Bandiera, StatoAvatar, GIALLO, ROSSO, VERDE};
use crate::config::{Caricata, Config, Posizione};
use crate::ocr::{self, Lettura, Messaggio};
use crate::pacing::{self, Andatura, Bandierine, Cronometro, Ritmo};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Key, Pos2, Rect, Sense, Stroke, Vec2,
    ViewportCommand,
};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// Dimensioni dell'overlay a scala 1.
pub const LARGHEZZA: f32 = 220.0;
pub const ALTEZZA: f32 = 340.0;

const ARANCIO: Color32 = Color32::from_rgb(255, 160, 60);
const GRIGIO: Color32 = Color32::from_rgb(190, 190, 190);

pub struct Ritardometro {
    cfg: Config,
    percorso_config: PathBuf,
    avviso: Option<String>,

    crono: Cronometro,
    bandierine: Bandierine,

    slide: Option<u32>,
    totale_letto: Option<u32>,
    candidata: Option<Lettura>,
    conferme: u8,
    ocr_rx: Option<Receiver<Messaggio>>,
    stato_ocr: String,

    posizionato: bool,
    nascita: Instant,
    /// Modalità demo: simula un talk a velocità accelerata.
    demo: Option<Instant>,
}

/// In demo un talk da 30 minuti dura un minuto.
const VELOCITA_DEMO: f32 = 30.0;

impl Ritardometro {
    pub fn new(cc: &eframe::CreationContext<'_>, caricata: Caricata, demo: bool) -> Self {
        let Caricata {
            config: cfg,
            percorso,
            avviso,
        } = caricata;

        let (ocr_rx, stato_ocr) = if demo {
            (None, format!("demo: talk simulato a {VELOCITA_DEMO}x"))
        } else if cfg.ocr.attivo {
            match ocr::avvia(cfg.ocr.clone(), cc.egui_ctx.clone()) {
                Ok(rx) => (Some(rx), "OCR in avvio…".to_string()),
                Err(e) => (None, e),
            }
        } else {
            (None, "OCR disattivato: solo timer".to_string())
        };

        let bandierine = Bandierine::new(pacing::risolvi_milestone(
            &cfg.milestone,
            cfg.durata_secondi(),
        ));

        Self {
            cfg,
            percorso_config: percorso,
            avviso,
            crono: Cronometro::new(),
            bandierine,
            slide: None,
            totale_letto: None,
            candidata: None,
            conferme: 0,
            ocr_rx,
            stato_ocr,
            posizionato: false,
            nascita: Instant::now(),
            demo: demo.then(Instant::now),
        }
    }

    /// In demo il tempo scorre veloce e le slide avanzano con un ritmo
    /// irregolare, così si vedono tutte le reazioni dell'avatar.
    fn simula(&mut self) {
        let Some(inizio) = self.demo else { return };
        let durata = self.cfg.durata_secondi();
        let ciclo = durata + self.cfg.domande_minuti * 60.0 + 90.0;
        let sim = (inizio.elapsed().as_secs_f32() * VELOCITA_DEMO) % ciclo;
        self.crono = Cronometro::new();
        self.crono.correggi(sim);
        self.crono.avvia();

        let totale = self.totale().unwrap_or(30).max(1);
        let f = (sim / durata).min(1.0);
        // prima corre troppo, poi si perde in chiacchiere, poi recupera
        let g = f + 0.13 * (std::f32::consts::TAU * 1.5 * f).sin();
        self.slide = Some((1 + (g * totale as f32).floor().max(0.0) as u32).min(totale));
    }

    fn totale(&self) -> Option<u32> {
        if self.cfg.totale_slide > 0 {
            Some(self.cfg.totale_slide)
        } else {
            self.totale_letto
        }
    }

    fn azzera(&mut self) {
        self.crono.azzera();
        self.bandierine.azzera();
    }

    fn imposta_slide(&mut self, nuova: u32) {
        let vecchia = self.slide;
        self.slide = Some(nuova);
        // Avvio automatico: appena si passa alla slide successiva alla prima vista.
        if self.cfg.avvio_automatico && self.crono.mai_avviato() {
            if let Some(v) = vecchia {
                if nuova > v {
                    self.crono.avvia();
                }
            }
        }
    }

    /// Legge i messaggi del thread OCR. Una lettura viene accettata solo se
    /// arriva uguale due volte di fila, così un errore isolato non fa saltare l'avatar.
    fn ricevi_ocr(&mut self) {
        let Some(rx) = &self.ocr_rx else { return };
        let messaggi: Vec<Messaggio> = rx.try_iter().collect();
        for m in messaggi {
            match m {
                Messaggio::Lettura(l) => {
                    if let Some(tot) = self.totale() {
                        if l.slide > tot && l.totale.is_none() {
                            self.stato_ocr = format!("letto {} (oltre {tot}): ignoro", l.slide);
                            continue;
                        }
                    }
                    if self.candidata == Some(l) {
                        self.conferme = self.conferme.saturating_add(1);
                    } else {
                        self.candidata = Some(l);
                        self.conferme = 1;
                    }
                    if self.conferme >= 2 {
                        if let Some(t) = l.totale {
                            self.totale_letto = Some(t);
                        }
                        if self.slide != Some(l.slide) {
                            self.imposta_slide(l.slide);
                        }
                        self.stato_ocr = "OCR ok".into();
                    }
                }
                Messaggio::Niente(testo) => {
                    let testo = testo.trim();
                    self.stato_ocr = if testo.is_empty() {
                        "OCR: non vedo il contatore".into()
                    } else {
                        format!("OCR: non capisco \"{}\"", accorcia(testo, 18))
                    };
                }
                Messaggio::Errore(e) => self.stato_ocr = e,
            }
        }
    }

    fn tastiera(&mut self, ctx: &egui::Context) {
        let (spazio, r, piu, meno, destra, sinistra, esci) = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::R),
                i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals),
                i.key_pressed(Key::Minus),
                i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::PageDown),
                i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::PageUp),
                i.key_pressed(Key::Escape) || i.key_pressed(Key::Q),
            )
        });
        if spazio {
            self.crono.alterna();
        }
        if r {
            self.azzera();
        }
        if piu {
            self.crono.correggi(60.0);
        }
        if meno {
            self.crono.correggi(-60.0);
        }
        if destra {
            let s = self.slide.unwrap_or(0) + 1;
            self.imposta_slide(s);
        }
        if sinistra {
            let s = self.slide.unwrap_or(2).saturating_sub(1).max(1);
            self.imposta_slide(s);
        }
        if esci {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    /// Mette la finestra nell'angolo scelto appena si conosce la dimensione del monitor.
    fn posiziona(&mut self, ctx: &egui::Context) {
        if self.posizionato {
            return;
        }
        let Some(monitor) = ctx.input(|i| i.viewport().monitor_size) else {
            return;
        };
        let dim = Vec2::new(LARGHEZZA, ALTEZZA) * self.cfg.scala;
        let m = self.cfg.margine;
        let x = match self.cfg.posizione {
            Posizione::AltoSinistra | Posizione::BassoSinistra => m,
            Posizione::AltoDestra | Posizione::BassoDestra => monitor.x - dim.x - m,
        };
        let y = match self.cfg.posizione {
            Posizione::AltoSinistra | Posizione::AltoDestra => m,
            Posizione::BassoSinistra | Posizione::BassoDestra => monitor.y - dim.y - m - 48.0,
        };
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
            x.max(0.0),
            y.max(0.0),
        )));
        self.posizionato = true;
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        let etichetta = if self.crono.in_corso() {
            "Pausa"
        } else {
            "Avvia"
        };
        if ui.button(format!("{etichetta}   (spazio)")).clicked() {
            self.crono.alterna();
            ui.close();
        }
        if ui.button("Azzera   (R)").clicked() {
            self.azzera();
            ui.close();
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Tempo");
            if ui.button("-1 min").clicked() {
                self.crono.correggi(-60.0);
            }
            if ui.button("+1 min").clicked() {
                self.crono.correggi(60.0);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Slide");
            if ui.button("-1").clicked() {
                let s = self.slide.unwrap_or(2).saturating_sub(1).max(1);
                self.imposta_slide(s);
            }
            if ui.button("+1").clicked() {
                let s = self.slide.unwrap_or(0) + 1;
                self.imposta_slide(s);
            }
        });
        ui.separator();
        if ui.button("Apri configurazione").clicked() {
            apri_file(&self.percorso_config);
            ui.close();
        }
        ui.label(
            egui::RichText::new("riavvia dopo aver modificato")
                .small()
                .color(GRIGIO),
        );
        ui.separator();
        if ui.button("Esci   (Esc)").clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn andatura(&self, trascorsi: f32) -> Option<Andatura> {
        pacing::andatura(
            trascorsi,
            self.cfg.durata_secondi(),
            self.slide?,
            self.totale()?,
            self.cfg.tolleranza_secondi,
            self.cfg.panico_secondi,
        )
    }
}

/// Le fasi del talk, per decidere cosa mostrare.
enum Fase {
    Attesa,
    Talk,
    Domande { restanti: f32 },
    Sforato { oltre: f32 },
}

impl eframe::App for Ritardometro {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // finestra trasparente
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.posiziona(&ctx);
        self.simula();
        self.ricevi_ocr();
        self.tastiera(&ctx);

        let s = self.cfg.scala;
        let t = self.nascita.elapsed().as_secs_f32();
        let durata = self.cfg.durata_secondi();
        let domande = self.cfg.domande_minuti * 60.0;
        let trascorsi = self.crono.trascorsi();
        self.bandierine.aggiorna(trascorsi, t);
        let andatura = self.andatura(trascorsi);
        let totale = self.totale();

        let fase = if self.crono.mai_avviato() {
            Fase::Attesa
        } else if trascorsi < durata {
            Fase::Talk
        } else {
            let oltre = trascorsi - durata;
            // Se a fine tempo sei ancora lontano dall'ultima slide, hai sforato davvero.
            let indietro = matches!((self.slide, totale), (Some(sl), Some(tot)) if sl + 1 < tot);
            if !indietro && oltre < domande {
                Fase::Domande {
                    restanti: domande - oltre,
                }
            } else {
                Fase::Sforato { oltre }
            }
        };

        // ---------------- interazione ----------------
        let area = ui.max_rect();
        let risposta = ui.interact(area, Id::unique("overlay"), Sense::click_and_drag());
        if risposta.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if risposta.clicked() {
            self.crono.alterna();
        }
        risposta.context_menu(|ui| self.menu(ui));

        // ---------------- stato dell'avatar ----------------
        let (mut stato, colore_scarto) = stato_avatar(&self.cfg, &fase, andatura.as_ref());
        if let Some((testo, p)) = self.bandierine.corrente() {
            if stato.bandiera.is_none() {
                stato.bandiera = Some(Bandiera {
                    testo: testo.to_string(),
                    colore: GIALLO,
                    alzata: (p / 0.08).min((1.0 - p) / 0.08).clamp(0.0, 1.0),
                });
            }
        }
        // Movimenti morbidi invece di scatti.
        let anim = 0.7;
        stato.umore = ctx.animate_value_with_time(Id::unique("umore"), stato.umore, anim);
        stato.alza_verde = ctx.animate_value_with_time(Id::unique("verde"), stato.alza_verde, anim);
        stato.alza_rosso = ctx.animate_value_with_time(Id::unique("rosso"), stato.alza_rosso, anim);

        // ---------------- disegno ----------------
        let painter = ui.painter_at(area);
        let fondo = area.shrink(2.0 * s);
        painter.rect_filled(
            fondo,
            CornerRadius::same((18.0 * s) as u8),
            Color32::from_black_alpha((self.cfg.opacita_sfondo * 255.0) as u8),
        );

        let lato_avatar = 196.0 * s;
        let rect_avatar = Rect::from_min_size(
            Pos2::new(area.center().x - lato_avatar / 2.0, area.min.y + 6.0 * s),
            Vec2::new(
                lato_avatar,
                lato_avatar * avatar::ALTEZZA / avatar::LARGHEZZA,
            ),
        );
        avatar::disegna(&painter, rect_avatar, &stato, t);

        let cx = area.center().x;
        let mut y = rect_avatar.max.y + 4.0 * s;

        // tempo grande
        let (tempo, colore_tempo) = match &fase {
            Fase::Attesa => (pacing::mm_ss(durata), Color32::WHITE),
            Fase::Talk => (pacing::mm_ss(durata - trascorsi), Color32::WHITE),
            Fase::Domande { restanti } => (pacing::mm_ss(*restanti), GIALLO),
            Fase::Sforato { oltre } => (format!("+{}", pacing::mm_ss(*oltre)), ROSSO),
        };
        let lampeggia =
            !self.crono.in_corso() && !self.crono.mai_avviato() && (t * 1.6).sin() < 0.0;
        let colore_tempo = if lampeggia {
            colore_tempo.gamma_multiply(0.45)
        } else {
            colore_tempo
        };
        painter.text(
            Pos2::new(cx, y),
            Align2::CENTER_TOP,
            tempo,
            FontId::monospace(40.0 * s),
            colore_tempo,
        );
        y += 46.0 * s;

        // riga di stato
        let riga = match (&fase, &andatura) {
            (Fase::Attesa, _) => {
                if self.cfg.avvio_automatico && self.ocr_rx.is_some() {
                    "clic o cambia slide per partire".to_string()
                } else {
                    "clic per partire".to_string()
                }
            }
            (Fase::Domande { .. }, _) => "tempo delle domande".to_string(),
            (Fase::Sforato { .. }, _) => "tempo scaduto!".to_string(),
            (Fase::Talk, Some(a)) => {
                let giudizio = match a.ritmo {
                    Ritmo::InLinea => "in linea",
                    Ritmo::Anticipo => "anticipo",
                    Ritmo::Ritardo => "ritardo",
                    Ritmo::Panico => "accelera!",
                };
                format!(
                    "slide {}/{}  {} {}",
                    self.slide.unwrap_or(0),
                    totale.unwrap_or(0),
                    pacing::scarto(a.scarto_secondi),
                    giudizio
                )
            }
            (Fase::Talk, None) => match self.slide {
                Some(sl) => format!("slide {sl}  (totale sconosciuto)"),
                None => "tempo rimanente".to_string(),
            },
        };
        painter.text(
            Pos2::new(cx, y),
            Align2::CENTER_TOP,
            riga,
            FontId::proportional(14.0 * s),
            colore_scarto,
        );
        y += 21.0 * s;

        // barra: tempo trascorso (bianco) e posizione nelle slide (tacca colorata)
        let barra = Rect::from_min_size(
            Pos2::new(area.min.x + 18.0 * s, y),
            Vec2::new(area.width() - 36.0 * s, 6.0 * s),
        );
        painter.rect_filled(
            barra,
            CornerRadius::same((3.0 * s) as u8),
            Color32::from_white_alpha(40),
        );
        let p_tempo = (trascorsi / durata).clamp(0.0, 1.0);
        let mut pieno = barra;
        pieno.set_width(barra.width() * p_tempo);
        painter.rect_filled(
            pieno,
            CornerRadius::same((3.0 * s) as u8),
            Color32::from_white_alpha(170),
        );
        if let (Some(sl), Some(tot)) = (self.slide, totale) {
            if tot > 0 {
                let p = ((sl.saturating_sub(1)) as f32 / tot as f32).clamp(0.0, 1.0);
                let x = barra.min.x + barra.width() * p;
                painter.line_segment(
                    [
                        Pos2::new(x, barra.min.y - 4.0 * s),
                        Pos2::new(x, barra.max.y + 4.0 * s),
                    ],
                    Stroke::new(3.0 * s, colore_scarto),
                );
            }
        }
        y += 14.0 * s;

        // riga piccola: pausa, stato OCR o avvisi
        let nota = if let Some(a) = &self.avviso {
            accorcia(a.lines().next().unwrap_or(a), 40)
        } else if !self.crono.in_corso() && !self.crono.mai_avviato() {
            "in pausa".to_string()
        } else {
            accorcia(&self.stato_ocr, 40)
        };
        painter.text(
            Pos2::new(cx, y),
            Align2::CENTER_TOP,
            nota,
            FontId::proportional(10.5 * s),
            GRIGIO,
        );

        // L'avviso sparisce dopo il primo avvio.
        if self.crono.in_corso() {
            self.avviso = None;
        }

        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

/// Traduce il ritmo del talk in espressione e posizione delle braccia.
fn stato_avatar(cfg: &Config, fase: &Fase, andatura: Option<&Andatura>) -> (StatoAvatar, Color32) {
    let tol = cfg.tolleranza_secondi;
    let pan = cfg.panico_secondi;
    let frazione = |x: f32| ((x - tol) / (pan - tol).max(1.0)).clamp(0.0, 1.0);

    match fase {
        Fase::Attesa => (StatoAvatar::default(), GRIGIO),
        Fase::Domande { .. } => (
            StatoAvatar {
                umore: 0.9,
                alza_verde: 0.6,
                alza_rosso: 0.0,
                sudore: false,
                bandiera: Some(Bandiera {
                    testo: "DOMANDE?".into(),
                    colore: GIALLO,
                    alzata: 1.0,
                }),
            },
            GIALLO,
        ),
        Fase::Sforato { .. } => (
            StatoAvatar {
                umore: -1.0,
                alza_verde: 0.0,
                alza_rosso: 1.0,
                sudore: true,
                bandiera: Some(Bandiera {
                    testo: "STOP!".into(),
                    colore: ROSSO,
                    alzata: 1.0,
                }),
            },
            ROSSO,
        ),
        Fase::Talk => match andatura {
            None => (
                StatoAvatar {
                    umore: 0.5,
                    ..Default::default()
                },
                Color32::WHITE,
            ),
            Some(a) => match a.ritmo {
                Ritmo::InLinea => (
                    StatoAvatar {
                        umore: 0.9,
                        alza_verde: 0.25,
                        alza_rosso: 0.25,
                        ..Default::default()
                    },
                    Color32::WHITE,
                ),
                Ritmo::Anticipo => {
                    let f = frazione(a.scarto_secondi);
                    (
                        StatoAvatar {
                            umore: 0.7,
                            alza_verde: 0.45 + 0.55 * f,
                            alza_rosso: 0.05,
                            ..Default::default()
                        },
                        VERDE,
                    )
                }
                Ritmo::Ritardo | Ritmo::Panico => {
                    let f = frazione(-a.scarto_secondi);
                    (
                        StatoAvatar {
                            umore: 0.1 - 1.1 * f,
                            alza_verde: 0.05,
                            alza_rosso: 0.45 + 0.55 * f,
                            sudore: a.ritmo == Ritmo::Panico,
                            bandiera: None,
                        },
                        if a.ritmo == Ritmo::Panico {
                            ROSSO
                        } else {
                            ARANCIO
                        },
                    )
                }
            },
        },
    }
}

fn accorcia(testo: &str, max: usize) -> String {
    if testo.chars().count() <= max {
        testo.to_string()
    } else {
        let mut s: String = testo.chars().take(max - 1).collect();
        s.push('…');
        s
    }
}

fn apri_file(percorso: &std::path::Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("notepad").arg(percorso).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(percorso).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(percorso).spawn();
}
