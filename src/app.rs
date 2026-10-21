//! La finestra overlay: mette insieme timer, OCR, avatar e controlli.

use crate::avatar::{self, Bandiera, StatoAvatar, GIALLO, ROSSO, VERDE};
use crate::config::{Caricata, Config, FinestraSalvata, Posizione};
use crate::ocr::{self, Lettura, Messaggio, StatoOcrCondiviso};
use crate::pacing::{self, Andatura, Bandierine, Cronometro, Ritmo};
use crate::schermo;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Key, Pos2, Rect, Sense, Stroke, Vec2,
    ViewportCommand,
};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Dimensioni dell'overlay a scala 1.
pub const LARGHEZZA: f32 = 220.0;
pub const ALTEZZA: f32 = 340.0;

const ARANCIO: Color32 = Color32::from_rgb(255, 160, 60);
const GRIGIO: Color32 = Color32::from_rgb(190, 190, 190);

#[cfg(windows)]
mod win32_drag {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicIsize, Ordering};

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetCursorPos(lpPoint: *mut Point) -> i32;
        pub fn GetActiveWindow() -> isize;
        pub fn GetWindowRect(hWnd: isize, lpRect: *mut Rect) -> i32;
        pub fn SetWindowPos(
            hWnd: isize,
            hWndInsertAfter: isize,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> i32;
        pub fn GetAsyncKeyState(vKey: i32) -> i16;
        pub fn GetWindowLongW(hWnd: isize, nIndex: i32) -> i32;
        pub fn SetWindowLongW(hWnd: isize, nIndex: i32, dwNewLong: i32) -> i32;
        pub fn GetWindowLongPtrW(hWnd: isize, nIndex: i32) -> isize;
        pub fn SetWindowLongPtrW(hWnd: isize, nIndex: i32, dwNewLong: isize) -> isize;
        pub fn CallWindowProcW(
            lpPrevWndFunc: isize,
            hWnd: isize,
            Msg: u32,
            wParam: usize,
            lParam: isize,
        ) -> isize;
        pub fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        pub fn DwmSetWindowAttribute(
            hwnd: isize,
            dwAttribute: u32,
            pvAttribute: *const c_void,
            cbAttribute: u32,
        ) -> i32;
    }

    pub const VK_LBUTTON: i32 = 0x01;
    pub const HWND_TOPMOST: isize = -1;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SWP_FRAMECHANGED: u32 = 0x0020;

    /// Posiziona una finestra identificata dal titolo nelle coordinate fisiche esatte
    /// `(x, y, w, h)` di qualsiasi monitor (anche con DPI diversi) e ne rimuove il bordo.
    pub fn posiziona_finestra_per_titolo(titolo: &str, x: i32, y: i32, w: i32, h: i32) {
        let mut wide: Vec<u16> = titolo.encode_utf16().collect();
        wide.push(0);
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
            if hwnd != 0 {
                rimuovi_bordo_finestra(hwnd);
                SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE);
            }
        }
    }

    const GWLP_WNDPROC: i32 = -4;
    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const WM_NCCALCSIZE: u32 = 0x0083;
    const WM_NCPAINT: u32 = 0x0085;

    const WS_POPUP: u32 = 0x8000_0000;
    const WS_CAPTION: u32 = 0x00C0_0000;
    const WS_BORDER: u32 = 0x0080_0000;
    const WS_DLGFRAME: u32 = 0x0040_0000;
    const WS_THICKFRAME: u32 = 0x0004_0000;
    const WS_EX_DLGMODALFRAME: u32 = 0x0000_0001;
    const WS_EX_WINDOWEDGE: u32 = 0x0000_0100;
    const WS_EX_CLIENTEDGE: u32 = 0x0000_0200;
    const WS_EX_STATICEDGE: u32 = 0x0002_0000;

    const DWMWA_NCRENDERING_POLICY: u32 = 2;
    const DWMNCRP_DISABLED: u32 = 1;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_DONOTROUND: u32 = 1;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;

    static OLD_WNDPROC: AtomicIsize = AtomicIsize::new(0);

    /// Intercetta `WM_NCCALCSIZE` prima della WndProc di `winit`.
    /// Quando `MARKER_UNDECORATED_SHADOW` è attivo, `winit` esegue `params.rgrc[0].top += 1`,
    /// lasciando una striscia non-client di 1px in cima alla finestra che Windows disegna bianca.
    /// Restituendo `0` senza alterare `rgrc[0]`, l'area client trasparente copre il 100% della finestra.
    unsafe extern "system" fn wndproc_senza_bordo(
        hwnd: isize,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        if msg == WM_NCCALCSIZE || msg == WM_NCPAINT {
            return 0;
        }
        let old = OLD_WNDPROC.load(Ordering::Relaxed);
        if old != 0 {
            unsafe { CallWindowProcW(old, hwnd, msg, wparam, lparam) }
        } else {
            0
        }
    }

    /// Rimuove la righina bianca superiore di 1px e il bordo/ombra che `egui-winit` attiva
    /// con `with_undecorated_shadow(true)`.
    pub fn rimuovi_bordo_finestra(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        unsafe {
            let nostra_proc = wndproc_senza_bordo as *const () as isize;
            let prev = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let ha_bordo_style = (style & (WS_CAPTION | WS_BORDER | WS_DLGFRAME | WS_THICKFRAME)) != 0
                || (style & WS_POPUP) == 0;

            if prev == nostra_proc && !ha_bordo_style {
                return;
            }

            let mut rect = Rect::default();
            let ha_rect = GetWindowRect(hwnd, &mut rect) != 0;

            if prev != 0 && prev != nostra_proc {
                OLD_WNDPROC.store(prev, Ordering::Relaxed);
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, nostra_proc);
            }

            let ncrp: u32 = DWMNCRP_DISABLED;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                (&ncrp as *const u32).cast::<c_void>(),
                std::mem::size_of::<u32>() as u32,
            );
            let corner: u32 = DWMWCP_DONOTROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                (&corner as *const u32).cast::<c_void>(),
                std::mem::size_of::<u32>() as u32,
            );
            let border_none: u32 = DWMWA_COLOR_NONE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                (&border_none as *const u32).cast::<c_void>(),
                std::mem::size_of::<u32>() as u32,
            );

            let nuovo_style = (style
                & !(WS_CAPTION | WS_BORDER | WS_DLGFRAME | WS_THICKFRAME))
                | WS_POPUP;
            SetWindowLongW(hwnd, GWL_STYLE, nuovo_style as i32);

            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let nuovo_ex = ex_style
                & !(WS_EX_DLGMODALFRAME
                    | WS_EX_WINDOWEDGE
                    | WS_EX_CLIENTEDGE
                    | WS_EX_STATICEDGE);
            SetWindowLongW(hwnd, GWL_EXSTYLE, nuovo_ex as i32);

            if ha_rect {
                SetWindowPos(
                    hwnd,
                    0,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct TrascinamentoWin32 {
    hwnd: isize,
    offset_cx: i32,
    offset_cy: i32,
    start_x: i32,
    start_y: i32,
    mosso: bool,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct ResizeWin32 {
    hwnd: isize,
    dir: egui::ResizeDirection,
    start_cur: win32_drag::Point,
    start_rect: win32_drag::Rect,
    scala0: f32,
}

/// Su macOS un'app a schermo intero (un PDF in Anteprima, Keynote, PowerPoint)
/// occupa uno Space tutto suo: le finestre delle altre app restano nello Space
/// di prima e spariscono dalla vista, anche se sono "sempre in primo piano".
///
/// L'unico modo per restare visibili è dirlo ad AppKit: `collectionBehavior` con
/// `canJoinAllSpaces` e un livello di finestra sopra quello delle presentazioni.
/// Né `eframe` né `winit` 0.30 espongono queste proprietà, quindi si passa
/// direttamente dall'`NSWindow`.
#[cfg(target_os = "macos")]
mod macos_spaces {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// `NSWindowCollectionBehaviorCanJoinAllSpaces` (1 << 0): la finestra compare
    /// in tutti gli Space, compresi quelli delle app a schermo intero.
    /// `Stationary` (1 << 4): non scorre via con l'animazione di cambio Space.
    /// `FullScreenAuxiliary` (1 << 8): può affiancare una finestra a schermo intero.
    const COMPORTAMENTO: usize = (1 << 0) | (1 << 4) | (1 << 8);

    /// `NSScreenSaverWindowLevel`. Il livello `AlwaysOnTop` di winit
    /// (`NSFloatingWindowLevel`, 3) sta sotto le finestre di presentazione, che
    /// usano livelli alti: da qui sopra il Ritardometro resta visibile anche
    /// durante uno slideshow di Keynote o PowerPoint.
    const LIVELLO: isize = 1000;

    pub fn mostra_sopra_schermo_intero(finestra: &impl HasWindowHandle) {
        let Ok(handle) = finestra.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return;
        };
        // SAFETY: `ns_view` è l'NSView della finestra appena creata da winit;
        // `-[NSView window]` restituisce un riferimento non posseduto, quindi
        // basta un puntatore grezzo senza gestirne il ciclo di vita.
        unsafe {
            let vista: *mut AnyObject = appkit.ns_view.as_ptr().cast();
            let finestra: *mut AnyObject = msg_send![vista, window];
            if finestra.is_null() {
                return;
            }
            let _: () = msg_send![finestra, setCollectionBehavior: COMPORTAMENTO];
            let _: () = msg_send![finestra, setLevel: LIVELLO];
        }
    }
}

pub struct Ritardometro {
    cfg: Config,
    percorso_config: PathBuf,
    avviso: Option<String>,

    crono: Cronometro,
    bandierine: Bandierine,

    /// Ultimo numero di slide conosciuto, da leggere sempre tramite `slide()`:
    /// da solo non dice se ci si può ancora credere.
    slide_letta: Option<u32>,
    /// Chi ha fornito quel numero, e quando.
    fonte_slide: Option<FonteSlide>,
    totale_letto: Option<u32>,
    candidata: Option<Lettura>,
    conferme: u8,
    ocr_rx: Option<Receiver<Messaggio>>,
    ocr_condiviso: Arc<Mutex<StatoOcrCondiviso>>,
    stato_ocr: String,
    /// Zona dello schermo realmente catturata: cambia quando la ricerca
    /// automatica trova il contatore in un altro angolo.
    zona_ocr: Option<ocr::Zona>,

    /// Impostazioni al momento dell'ultimo salvataggio: servono ad accorgersi
    /// di un cambiamento senza dover agganciare ogni singolo controllo del menu.
    preferenze: Preferenze,
    /// Quando è scattata la modifica in attesa di essere scritta su file.
    salva_da: Option<Instant>,
    /// Falso se il file di configurazione non è leggibile: riscriverlo
    /// cancellerebbe quello che c'è dentro.
    salvabile: bool,
    /// Chiavi imposte dalla riga di comando: valgono per questa esecuzione ma
    /// non vanno scritte nel file.
    da_cli: Vec<&'static str>,
    /// Rettangolo fisico della finestra, per ricordarne la posizione.
    rect_finestra: Option<(i32, i32, i32, i32)>,

    /// Sorveglianza della schermata, per le presentazioni senza numerazione.
    schermo_rx: Option<Receiver<schermo::Messaggio>>,
    /// Ultimo cambio di schermata rilevato.
    ultimo_cambio: Instant,
    /// Finché la cattura non ha dato prova di funzionare (almeno un cambio
    /// visto) non accusiamo nessuno di essersi fermato: su macOS senza permesso
    /// di registrazione schermo vedremmo un'immagine ferma per sempre.
    cambio_visto: bool,

    posizionato: bool,
    resize_attivo: Option<(egui::ResizeDirection, Pos2, f32)>,
    #[cfg(windows)]
    hwnd_principale: isize,
    #[cfg(windows)]
    bordo_rimosso: bool,
    #[cfg(windows)]
    trascinamento: Option<TrascinamentoWin32>,
    #[cfg(windows)]
    resize_win32: Option<ResizeWin32>,
    #[cfg(windows)]
    trascinato_appena: bool,

    menu_aperto: bool,
    menu_pos: Option<Pos2>,
    scheda: Scheda,
    num_monitor: usize,
    evidenzia_ocr: Option<(Instant, ocr::InfoZonaOcr)>,

    nascita: Instant,
    /// Modalità demo: simula un talk a velocità accelerata.
    demo: Option<Instant>,
}

/// Le schede del menu di configurazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Scheda {
    #[default]
    Talk,
    Overlay,
    Ocr,
    Milestone,
}

/// Da dove arriva il numero di slide. L'OCR può smettere di confermarlo (un
/// contatore che sparisce, l'OCR spento); una scelta fatta a mano no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FonteSlide {
    Ocr(Instant),
    Manuale,
}

/// Le impostazioni che si possono cambiare dal menu (più la posizione della
/// finestra): quando questa istantanea cambia, il file va riscritto.
#[derive(Debug, Clone, PartialEq)]
struct Preferenze {
    talk: (f32, f32, u32, bool, f32, f32, f32),
    aspetto: (f32, f32, f32, Posizione, f32),
    ocr: (bool, usize, bool, f32, f32, f32, f32, u64, u32, bool),
    finestra: Option<FinestraSalvata>,
    milestone: Vec<crate::config::MilestoneConfig>,
}

impl Preferenze {
    fn da(cfg: &Config) -> Self {
        Self {
            talk: (
                cfg.duration_minutes,
                cfg.questions_minutes,
                cfg.total_slides,
                cfg.auto_start,
                cfg.tolerance_seconds,
                cfg.panic_seconds,
                cfg.stuck_factor,
            ),
            aspetto: (
                cfg.scale,
                cfg.background_opacity,
                cfg.opacity,
                cfg.position,
                cfg.margin,
            ),
            ocr: (
                cfg.ocr.enabled,
                cfg.ocr.monitor,
                cfg.ocr.auto_zone,
                cfg.ocr.x,
                cfg.ocr.y,
                cfg.ocr.width,
                cfg.ocr.height,
                cfg.ocr.interval_ms,
                cfg.ocr.magnification,
                cfg.ocr.save_debug,
            ),
            finestra: cfg.window,
            milestone: cfg.milestone.clone(),
        }
    }
}

/// Quanto si aspetta, dopo l'ultima modifica, prima di scrivere su disco:
/// trascinare uno slider non deve significare cento scritture.
const ATTESA_SALVATAGGIO: Duration = Duration::from_millis(900);

/// In demo un talk da 30 minuti dura un minuto.
const VELOCITA_DEMO: f32 = 30.0;

impl Ritardometro {
    pub fn new(cc: &eframe::CreationContext<'_>, caricata: Caricata, demo: bool) -> Self {
        // Disattiva lo zoom da tastiera interno di egui (Ctrl+ / Ctrl- / Ctrl+0):
        // altrimenti egui modifica `zoom_factor` con un frame di ritardo rispetto a `InnerSize`,
        // annullando il primo passo quando si inverte direzione tra Ctrl+ e Ctrl-.
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.set_visuals(tema_scuro_contrasto());
        #[cfg(target_os = "macos")]
        macos_spaces::mostra_sopra_schermo_intero(cc);

        let Caricata {
            config: cfg,
            percorso,
            avviso,
            salvabile,
            da_cli,
        } = caricata;

        let ocr_condiviso = Arc::new(Mutex::new(StatoOcrCondiviso {
            cfg: cfg.ocr.clone(),
            centro_finestra: None,
            rect_finestra: None,
            in_pausa: false,
        }));

        let (ocr_rx, stato_ocr) = if demo {
            (None, format!("demo: simulated talk at {VELOCITA_DEMO}x"))
        } else if cfg.ocr.enabled {
            match ocr::avvia(Arc::clone(&ocr_condiviso), cc.egui_ctx.clone()) {
                Ok(rx) => (Some(rx), "Starting OCR...".to_string()),
                Err(e) => (None, e),
            }
        } else {
            (None, "OCR disabled: timer only".to_string())
        };

        // Il rilevatore di cambio schermata serve solo quando il gesto è attivo.
        let schermo_rx = if demo || cfg.stuck_factor <= 0.0 {
            None
        } else {
            schermo::avvia(Arc::clone(&ocr_condiviso), cc.egui_ctx.clone()).ok()
        };

        let bandierine = Bandierine::new(pacing::risolvi_milestone(
            &cfg.milestone,
            cfg.durata_secondi(),
        ));
        let cfg_iniziale = cfg.clone();

        Self {
            cfg,
            percorso_config: percorso,
            avviso,
            crono: Cronometro::new(),
            bandierine,
            slide_letta: None,
            fonte_slide: None,
            totale_letto: None,
            candidata: None,
            conferme: 0,
            ocr_rx,
            ocr_condiviso,
            stato_ocr,
            zona_ocr: None,
            preferenze: Preferenze::da(&cfg_iniziale),
            salva_da: None,
            salvabile,
            da_cli,
            rect_finestra: None,
            schermo_rx,
            ultimo_cambio: Instant::now(),
            cambio_visto: false,
            posizionato: false,
            resize_attivo: None,
            #[cfg(windows)]
            hwnd_principale: 0,
            #[cfg(windows)]
            bordo_rimosso: false,
            #[cfg(windows)]
            trascinamento: None,
            #[cfg(windows)]
            resize_win32: None,
            #[cfg(windows)]
            trascinato_appena: false,
            menu_aperto: false,
            menu_pos: None,
            scheda: Scheda::default(),
            num_monitor: ocr::numero_monitor(),
            evidenzia_ocr: None,
            nascita: Instant::now(),
            demo: demo.then(Instant::now),
        }
    }

    /// In demo il tempo scorre veloce e le slide avanzano con un ritmo
    /// irregolare, così si vedono tutte le reazioni dell'avatar.
    fn simula(&mut self) {
        let Some(inizio) = self.demo else { return };
        let durata = self.cfg.durata_secondi();
        let ciclo = durata + self.cfg.questions_minutes * 60.0 + 90.0;
        let sim = (inizio.elapsed().as_secs_f32() * VELOCITA_DEMO) % ciclo;
        self.crono = Cronometro::new();
        self.crono.correggi(sim);
        self.crono.avvia();

        let totale = self.totale().unwrap_or(30).max(1);
        let f = (sim / durata).min(1.0);
        // prima corre troppo, poi si perde in chiacchiere, poi recupera
        let g = f + 0.13 * (std::f32::consts::TAU * 1.5 * f).sin();
        self.slide_letta = Some((1 + (g * totale as f32).floor().max(0.0) as u32).min(totale));
        self.fonte_slide = Some(FonteSlide::Manuale);

        // Una finestra della demo imita una presentazione senza numerazione
        // rimasta ferma sulla stessa schermata, per mostrare anche il gesto
        // dell'orologio: è l'unico modo di vederlo senza un talk vero.
        const DA: f32 = 0.55;
        const A: f32 = 0.72;
        if self.cfg.stuck_factor > 0.0 && (DA..A).contains(&f) {
            self.slide_letta = None;
            self.fonte_slide = None;
            self.cambio_visto = true;
            let soglia = durata / totale as f32 * self.cfg.stuck_factor;
            let avanzamento = (f - DA) / (A - DA);
            let fermo_da = Duration::from_secs_f32(soglia * (1.0 + avanzamento));
            self.ultimo_cambio = Instant::now()
                .checked_sub(fermo_da)
                .unwrap_or_else(Instant::now);
        } else {
            self.cambio_visto = false;
            self.ultimo_cambio = Instant::now();
        }
    }

    /// Il numero di slide, ma solo finché ci si può ancora credere.
    ///
    /// Quando l'OCR smette di trovare il contatore, il numero fermo diventa in
    /// fretta una bugia: il tempo passa, chi parla avanza, e il Ritardometro
    /// mostrerebbe un ritardo inventato che cresce da solo. Passata la
    /// scadenza si preferisce non sapere su che slide siamo — niente conto
    /// sulle slide, niente numeri a video — e restare un cronometro onesto.
    /// Un numero impostato a mano con le frecce invece resta: è un'asserzione
    /// di chi parla, non una misura che può scadere.
    fn slide(&self) -> Option<u32> {
        match self.fonte_slide? {
            FonteSlide::Manuale => self.slide_letta,
            FonteSlide::Ocr(quando) => {
                let scadenza = pacing::scadenza_lettura(
                    self.cfg.durata_secondi(),
                    self.totale().unwrap_or(0),
                );
                if quando.elapsed().as_secs_f32() < scadenza {
                    self.slide_letta
                } else {
                    None
                }
            }
        }
    }

    fn totale(&self) -> Option<u32> {
        self.totale_letto
            .or_else(|| (self.cfg.total_slides > 0).then_some(self.cfg.total_slides))
    }

    fn in_pausa(&self) -> bool {
        !self.crono.in_corso() && !self.crono.mai_avviato()
    }

    fn alterna_pausa(&mut self) {
        self.crono.alterna();
        self.sincronizza_ocr_condiviso();
    }

    fn azzera(&mut self) {
        self.crono.azzera();
        self.bandierine.azzera();
        self.sincronizza_ocr_condiviso();
    }

    fn imposta_slide(&mut self, nuova: u32, fonte: FonteSlide) {
        let vecchia = self.slide_letta;
        self.slide_letta = Some(nuova);
        self.fonte_slide = Some(fonte);
        // Avvio automatico: appena si passa alla slide successiva alla prima vista.
        if self.cfg.auto_start && self.crono.mai_avviato() {
            if let Some(v) = vecchia {
                if nuova > v {
                    self.crono.avvia();
                    self.sincronizza_ocr_condiviso();
                }
            }
        }
    }

    /// Legge i messaggi del thread OCR. Le letture con totale (`slide/totale`), la prima lettura
    /// o un avanzamento adiacente (`±1`) vengono accettate subito; i salti isolati di un singolo
    /// numero richiedono due letture uguali di fila per evitare falsi positivi.
    fn ricevi_ocr(&mut self) {
        let Some(rx) = &self.ocr_rx else { return };
        let messaggi: Vec<Messaggio> = rx.try_iter().collect();
        if self.in_pausa() {
            return;
        }
        for m in messaggi {
            match m {
                Messaggio::Lettura(l) => {
                    if let Some(tot) = self.totale() {
                        if l.slide > tot && l.totale.is_none() {
                            self.stato_ocr = format!("read {} (exceeds {tot}): ignored", l.slide);
                            continue;
                        }
                        // Se sappiamo già che il contatore ha un totale (es. "2 / 30") e in un
                        // frame l'OCR perde la slide corrente leggendo solo "30", non saltare a 30!
                        if self.totale_letto == Some(tot)
                            && l.totale.is_none()
                            && l.slide == tot
                            && self.slide_letta.is_none_or(|cur| cur + 1 < tot)
                        {
                            self.stato_ocr = format!("OCR: detected total only (/{tot})");
                            continue;
                        }
                    }
                    if self.candidata == Some(l) {
                        self.conferme = self.conferme.saturating_add(1);
                    } else {
                        self.candidata = Some(l);
                        self.conferme = 1;
                    }
                    let immediata = l.totale.is_some()
                        || self.slide_letta.is_none()
                        || self
                            .slide_letta
                            .is_some_and(|cur| cur.abs_diff(l.slide) <= 1);
                    if immediata || self.conferme >= 2 {
                        if let Some(t) = l.totale {
                            self.totale_letto = Some(t);
                        }
                        // Anche una lettura uguale alla precedente conta:
                        // conferma che il contatore è ancora lì e rimanda la scadenza.
                        self.imposta_slide(l.slide, FonteSlide::Ocr(Instant::now()));
                        let dove = self.suffisso_zona();
                        self.stato_ocr = match self.totale() {
                            Some(tot) => format!("OCR ok: slide {}/{tot}{dove}", l.slide),
                            None => format!("OCR ok: slide {}{dove}", l.slide),
                        };
                    }
                }
                Messaggio::Niente(testo) => {
                    let testo = testo.trim();
                    self.stato_ocr = if testo.is_empty() {
                        "OCR: slide counter not found".into()
                    } else {
                        format!("OCR: unrecognized text \"{}\"", accorcia(testo, 18))
                    };
                }
                Messaggio::Errore(e) => self.stato_ocr = e,
                Messaggio::ZonaCambiata(z) => {
                    self.zona_ocr = Some(z);
                    self.stato_ocr = format!("OCR: looking for counter ({})", z.etichetta());
                }
            }
        }
    }

    /// Scrive sul file le impostazioni cambiate dal menu, poco dopo l'ultima
    /// modifica. Il confronto è sull'istantanea, così nessun controllo del menu
    /// può dimenticarsi di chiedere il salvataggio.
    fn salva_preferenze(&mut self) {
        if !self.salvabile {
            return;
        }
        let attuali = Preferenze::da(&self.cfg);
        if attuali != self.preferenze {
            self.preferenze = attuali;
            self.salva_da = Some(Instant::now());
        }
        if self.salva_da.is_some_and(|t| t.elapsed() >= ATTESA_SALVATAGGIO) {
            self.scrivi_config();
        }
    }

    fn scrivi_config(&mut self) {
        self.salva_da = None;
        if let Err(e) = crate::config::salva(&self.percorso_config, &self.cfg, &self.da_cli) {
            self.avviso = Some(e);
        }
    }

    /// Ricorda dove è stata trascinata la finestra. Si aspetta che il
    /// posizionamento iniziale si sia assestato, altrimenti si salverebbe la
    /// posizione di partenza invece di quella scelta.
    fn ricorda_posizione(&mut self) {
        if !self.posizionato || self.nascita.elapsed() < Duration::from_millis(1500) {
            return;
        }
        let Some((sinistra, alto, _, _)) = self.rect_finestra else {
            return;
        };
        let spostata = self
            .cfg
            .window
            .is_none_or(|v| (v.x - sinistra).abs() > 2 || (v.y - alto).abs() > 2);
        if spostata {
            self.cfg.window = Some(FinestraSalvata {
                x: sinistra,
                y: alto,
            });
        }
    }

    /// Rimette la finestra dov'era all'ultima chiusura, ma solo se quel punto è
    /// ancora su uno schermo collegato: dopo aver staccato il monitor di un
    /// evento, l'overlay resterebbe invisibile fuori dallo schermo.
    fn ripristina_posizione(&mut self, ctx: &egui::Context, monitor_corrente: Vec2) -> bool {
        let Some(f) = self.cfg.window else {
            return false;
        };
        let ppp = ctx.pixels_per_point().max(0.1);
        let dim = Vec2::new(LARGHEZZA, ALTEZZA) * self.cfg.scale * ppp;
        let mut schermi = schermo::monitor_rettangoli();
        if schermi.is_empty() {
            // Senza elenco dei monitor (Linux) si controlla almeno quello corrente.
            schermi.push((
                0,
                0,
                (monitor_corrente.x * ppp) as u32,
                (monitor_corrente.y * ppp) as u32,
            ));
        }
        if !schermo::posizione_visibile((f.x, f.y, dim.x as u32, dim.y as u32), &schermi) {
            self.cfg.window = None;
            return false;
        }

        // Su Windows si usano direttamente i pixel fisici: con monitor a DPI
        // diversi la conversione in punti sbaglierebbe schermo.
        #[cfg(windows)]
        if self.hwnd_principale != 0 {
            unsafe {
                win32_drag::SetWindowPos(
                    self.hwnd_principale,
                    win32_drag::HWND_TOPMOST,
                    f.x,
                    f.y,
                    0,
                    0,
                    win32_drag::SWP_NOSIZE | win32_drag::SWP_NOACTIVATE,
                );
            }
            return true;
        }

        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
            f.x as f32 / ppp,
            f.y as f32 / ppp,
        )));
        true
    }

    /// Accende l'OCR dal menu: se il programma era partito con l'OCR spento il
    /// thread non esiste ancora e va creato adesso.
    fn accendi_ocr(&mut self, ctx: &egui::Context) {
        if self.ocr_rx.is_some() || self.demo.is_some() {
            return;
        }
        match ocr::avvia(Arc::clone(&self.ocr_condiviso), ctx.clone()) {
            Ok(rx) => {
                self.ocr_rx = Some(rx);
                self.stato_ocr = "Starting OCR...".to_string();
            }
            Err(e) => {
                self.cfg.ocr.enabled = false;
                self.stato_ocr = e;
            }
        }
    }

    /// Legge il thread che sorveglia la schermata (presentazioni senza numerazione).
    fn ricevi_schermo(&mut self) {
        let Some(rx) = &self.schermo_rx else { return };
        let messaggi: Vec<schermo::Messaggio> = rx.try_iter().collect();
        for m in messaggi {
            match m {
                schermo::Messaggio::Cambiato => {
                    self.ultimo_cambio = Instant::now();
                    self.cambio_visto = true;
                }
                schermo::Messaggio::Errore(e) => {
                    // Senza catture affidabili il gesto resta disarmato.
                    self.cambio_visto = false;
                    if self.ocr_rx.is_none() {
                        self.stato_ocr = e;
                    }
                }
            }
        }
    }

    /// Da quanto tempo la schermata è ferma, come intensità del gesto
    /// "guarda l'orologio" (0 = niente). Vale solo durante il talk e solo
    /// quando non sappiamo su che slide siamo: con il contatore letto
    /// dall'OCR il ritardo lo calcola `andatura()`, che è molto più preciso.
    fn stallo(&self, fase: &Fase, andatura: Option<&Andatura>) -> f32 {
        if !matches!(fase, Fase::Talk) || andatura.is_some() || !self.cambio_visto {
            return 0.0;
        }
        pacing::stallo(
            self.ultimo_cambio.elapsed().as_secs_f32(),
            self.cfg.durata_secondi(),
            self.totale().unwrap_or(0),
            self.cfg.stuck_factor,
        )
    }

    /// `" @ bottom-left"` quando la ricerca automatica ha spostato la cattura
    /// rispetto alla zona scritta nel file di configurazione.
    fn suffisso_zona(&self) -> String {
        match self.zona_ocr {
            Some(z) if !z.vicina(&ocr::Zona::da_config(&self.cfg.ocr)) => {
                format!(" @ {}", z.etichetta())
            }
            _ => String::new(),
        }
    }

    /// Configurazione OCR con la zona davvero in uso, per evidenziarla sullo schermo.
    fn ocr_cfg_effettiva(&self) -> crate::config::OcrConfig {
        match self.zona_ocr {
            Some(z) => ocr::con_zona(&self.cfg.ocr, z),
            None => self.cfg.ocr.clone(),
        }
    }

    fn tastiera(&mut self, ctx: &egui::Context) {
        let (spazio, r, piu, meno, ctrl, zero, destra, sinistra, esci, scroll_y) = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::R),
                i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals),
                i.key_pressed(Key::Minus),
                i.modifiers.ctrl || i.modifiers.command,
                i.key_pressed(Key::Num0),
                i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::PageDown),
                i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::PageUp),
                i.key_pressed(Key::Escape) || i.key_pressed(Key::Q),
                i.smooth_scroll_delta.y,
            )
        });
        if spazio {
            self.alterna_pausa();
        }
        if r {
            self.azzera();
        }
        if ctrl {
            if piu {
                self.cambia_scala(ctx, self.cfg.scale + 0.15);
            }
            if meno {
                self.cambia_scala(ctx, self.cfg.scale - 0.15);
            }
            if zero {
                self.cambia_scala(ctx, 1.0);
            }
        } else {
            if piu {
                self.crono.correggi(60.0);
            }
            if meno {
                self.crono.correggi(-60.0);
            }
        }
        if scroll_y.abs() > 0.1 {
            let passo = (scroll_y / 50.0).clamp(-0.3, 0.3);
            self.cambia_scala(ctx, self.cfg.scale + passo);
        }
        if destra {
            let s = self.slide_letta.unwrap_or(0) + 1;
            self.imposta_slide(s, FonteSlide::Manuale);
        }
        if sinistra {
            let s = self.slide_letta.unwrap_or(2).saturating_sub(1).max(1);
            self.imposta_slide(s, FonteSlide::Manuale);
        }
        if esci {
            if self.menu_aperto {
                self.menu_aperto = false;
            } else {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn cambia_scala(&mut self, ctx: &egui::Context, nuova: f32) {
        let nuova = nuova.clamp(0.5, 3.0);
        if (nuova - self.cfg.scale).abs() > 0.005 {
            self.cfg.scale = nuova;
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                ViewportCommand::InnerSize(Vec2::new(LARGHEZZA, ALTEZZA) * self.cfg.scale),
            );
            ctx.request_repaint();
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
        if self.ripristina_posizione(ctx, monitor) {
            self.posizionato = true;
            return;
        }
        let dim = Vec2::new(LARGHEZZA, ALTEZZA) * self.cfg.scale;
        let m = self.cfg.margin;
        let x = match self.cfg.position {
            Posizione::TopLeft | Posizione::BottomLeft => m,
            Posizione::TopRight | Posizione::BottomRight => monitor.x - dim.x - m,
        };
        let y = match self.cfg.position {
            Posizione::TopLeft | Posizione::TopRight => m,
            Posizione::BottomLeft | Posizione::BottomRight => monitor.y - dim.y - m - 48.0,
        };
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
            x.max(0.0),
            y.max(0.0),
        )));
        self.posizionato = true;
    }

    /// Mostra il pannello di configurazione in una finestra separata indipendente
    /// dalla dimensione dell'overlay, così non rimane mai schiacciato su finestre piccole.
    fn mostra_finestra_menu(&mut self, ctx: &egui::Context) {
        if !self.menu_aperto {
            return;
        }

        // Comoda all'apertura, ma riducibile: su un portatile da 768 px di
        // altezza una finestra fissa da 700 coprirebbe mezzo schermo.
        let dim_menu = Vec2::new(500.0, 700.0);
        let minimo = Vec2::new(430.0, 420.0);
        let mut builder = egui::ViewportBuilder::default()
            .with_title(concat!("Ritardometro v", env!("CARGO_PKG_VERSION"), " — Settings"))
            .with_inner_size(dim_menu)
            .with_min_inner_size(minimo)
            .with_resizable(true)
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_always_on_top();

        if let Some(pos) = self.menu_pos {
            builder = builder.with_position(pos);
        }

        let id_menu = egui::ViewportId::from_hash_of("ritardometro_menu_config");
        ctx.show_viewport_immediate(id_menu, builder, |ui, _class| {
            ui.ctx().set_theme(egui::Theme::Dark);
            ui.ctx().set_visuals(tema_scuro_contrasto());

            let chiudi_input = ui.input(|i| {
                i.viewport().close_requested() || i.key_pressed(Key::Escape)
            });
            if chiudi_input {
                self.menu_aperto = false;
            }

            let frame = egui::Frame::new()
                .fill(Color32::from_rgb(26, 28, 34))
                .inner_margin(14.0);
            egui::CentralPanel::default().frame(frame).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 7.0);
                self.disegna_contenuto_menu(ui, ctx);
            });
        });
    }

    fn disegna_contenuto_menu(&mut self, ui: &mut egui::Ui, ctx_root: &egui::Context) {
        // ---- testa fissa: i comandi che servono mentre si parla ----
        ui.horizontal(|ui| {
            let etichetta = if self.crono.in_corso() {
                "Pause (space)"
            } else {
                "Start (space)"
            };
            if ui.button(etichetta).clicked() {
                self.alterna_pausa();
            }
            if ui.button("Reset (R)").clicked() {
                self.azzera();
            }
        });
        ui.add_space(4.0);
        egui::Grid::new("comandi_dal_vivo")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Time:");
                ui.horizontal(|ui| {
                    if ui.button("−1 min").clicked() {
                        self.crono.correggi(-60.0);
                    }
                    if ui.button("+1 min").clicked() {
                        self.crono.correggi(60.0);
                    }
                });
                ui.end_row();

                ui.label("Slide:");
                ui.horizontal(|ui| {
                    if ui.button("−1").clicked() {
                        let s = self.slide_letta.unwrap_or(2).saturating_sub(1).max(1);
                        self.imposta_slide(s, FonteSlide::Manuale);
                    }
                    if ui.button("+1").clicked() {
                        let s = self.slide_letta.unwrap_or(0) + 1;
                        self.imposta_slide(s, FonteSlide::Manuale);
                    }
                    if let Some(sl) = self.slide() {
                        ui.label(format!("({sl}/{})", self.totale().unwrap_or(0)));
                    }
                });
                ui.end_row();
            });

        ui.separator();

        // ---- linguette ----
        ui.horizontal(|ui| {
            for (scheda, nome) in [
                (Scheda::Talk, "Talk"),
                (Scheda::Overlay, "Overlay"),
                (Scheda::Ocr, "OCR"),
                (Scheda::Milestone, "Milestones"),
            ] {
                if ui.selectable_label(self.scheda == scheda, nome).clicked() {
                    self.scheda = scheda;
                }
            }
        });

        ui.separator();

        // ---- corpo: una scheda per volta, con il piede sempre visibile ----
        const ALTEZZA_PIEDE: f32 = 64.0;
        let disponibile = (ui.available_height() - ALTEZZA_PIEDE).max(140.0);
        let scheda = self.scheda;
        egui::ScrollArea::vertical()
            .max_height(disponibile)
            .auto_shrink([false, false])
            .show(ui, |ui| match scheda {
                Scheda::Talk => self.scheda_talk(ui, ctx_root),
                Scheda::Overlay => self.scheda_overlay(ui, ctx_root),
                Scheda::Ocr => self.scheda_ocr(ui, ctx_root),
                Scheda::Milestone => self.scheda_milestone(ui),
            });

        ui.separator();

        // ---- piede fisso ----
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new(format!("Ritardometro v{}", env!("CARGO_PKG_VERSION")))
                    .strong(),
            );
            if ui
                .link("GitHub")
                .on_hover_text(env!("CARGO_PKG_REPOSITORY"))
                .clicked()
            {
                apri_url(env!("CARGO_PKG_REPOSITORY"));
            }
            ui.label(
                egui::RichText::new("· changes are saved automatically")
                    .small()
                    .color(Color32::from_rgb(195, 200, 215)),
            );
        });
        ui.horizontal(|ui| {
            if ui.button("Close Menu").clicked() {
                self.menu_aperto = false;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Quit App").clicked() {
                    ctx_root.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::Close);
                }
            });
        });
    }

    /// Tutto quello che riguarda il talk: quanto dura e quando preoccuparsi.
    fn scheda_talk(&mut self, ui: &mut egui::Ui, ctx_root: &egui::Context) {
        let durata_prima = self.cfg.duration_minutes;
        let fattore_prima = self.cfg.stuck_factor;

        egui::Grid::new("scheda_talk")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Talk duration:");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.duration_minutes)
                        .speed(0.5)
                        .range(1.0..=600.0)
                        .suffix(" min"),
                );
                ui.end_row();

                ui.label("Questions:");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.questions_minutes)
                        .speed(0.5)
                        .range(0.0..=180.0)
                        .suffix(" min"),
                )
                .on_hover_text("Countdown after the talk. 0 = no question time");
                ui.end_row();

                ui.label("Total slides:");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.total_slides)
                        .speed(1.0)
                        .range(0..=999),
                )
                .on_hover_text("0 = take the total from the counter on screen (\"12 / 40\")");
                ui.end_row();

                ui.label("Auto start:");
                ui.checkbox(
                    &mut self.cfg.auto_start,
                    "start the timer on the first slide change",
                );
                ui.end_row();

                ui.label("On track within:");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.tolerance_seconds)
                        .speed(1.0)
                        .range(0.0..=900.0)
                        .suffix(" s"),
                )
                .on_hover_text("Inside this gap the avatar stays happy");
                ui.end_row();

                ui.label("Panic after:");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.panic_seconds)
                        .speed(1.0)
                        .range(1.0..=1800.0)
                        .suffix(" s"),
                )
                .on_hover_text("Behind by this much and the avatar starts sweating");
                ui.end_row();

                ui.label("Dwell watch:");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.stuck_factor)
                        .speed(0.1)
                        .range(0.0..=10.0)
                        .suffix("× slide"),
                )
                .on_hover_text(
                    "No slide counter: show the pulsing watch when the screen has not \
                     changed for this many times the average slide. 0 = never watch the screen",
                );
                ui.end_row();
            });

        // Il panico deve restare oltre la tolleranza, altrimenti non scatta mai.
        self.cfg.panic_seconds = self.cfg.panic_seconds.max(self.cfg.tolerance_seconds + 1.0);
        if self.cfg.stuck_factor > 0.0 {
            self.cfg.stuck_factor = self.cfg.stuck_factor.max(1.0);
        }
        // Le milestone sono risolte in secondi: cambiata la durata, vanno rifatte.
        if self.cfg.duration_minutes != durata_prima {
            self.ricalcola_milestone();
        }
        if fattore_prima <= 0.0 && self.cfg.stuck_factor > 0.0 {
            self.accendi_schermo(ctx_root);
        }
    }

    /// Aspetto e posizione della finestra.
    fn scheda_overlay(&mut self, ui: &mut egui::Ui, ctx_root: &egui::Context) {
        egui::Grid::new("scheda_overlay")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(format!("Size ({:.0}%):", self.cfg.scale * 100.0));
                ui.horizontal(|ui| {
                    if ui.button(" − ").clicked() {
                        self.cambia_scala(ctx_root, self.cfg.scale - 0.15);
                    }
                    if ui.button(" + ").clicked() {
                        self.cambia_scala(ctx_root, self.cfg.scale + 0.15);
                    }
                    if ui.button("100%").clicked() {
                        self.cambia_scala(ctx_root, 1.0);
                    }
                });
                ui.end_row();

                let mut sfondo = (self.cfg.background_opacity * 100.0).round();
                ui.label("Background:");
                if ui
                    .add(
                        egui::Slider::new(&mut sfondo, 0.0..=100.0)
                            .suffix("%")
                            .fixed_decimals(0),
                    )
                    .changed()
                {
                    self.cfg.background_opacity = (sfondo / 100.0).clamp(0.0, 1.0);
                    ctx_root.request_repaint();
                }
                ui.end_row();

                let mut avatar = (self.cfg.opacity * 100.0).round();
                ui.label("Avatar & text:");
                if ui
                    .add(
                        egui::Slider::new(&mut avatar, 15.0..=100.0)
                            .suffix("%")
                            .fixed_decimals(0),
                    )
                    .changed()
                {
                    self.cfg.opacity = (avatar / 100.0).clamp(0.15, 1.0);
                    ctx_root.request_repaint();
                }
                ui.end_row();

                ui.label("Start corner:");
                ui.horizontal(|ui| {
                    for p in Posizione::TUTTE {
                        if ui
                            .selectable_label(self.cfg.position == p, p.nome())
                            .clicked()
                        {
                            self.cfg.position = p;
                        }
                    }
                });
                ui.end_row();

                ui.label("Margin:");
                ui.add(
                    egui::DragValue::new(&mut self.cfg.margin)
                        .speed(1.0)
                        .range(0.0..=300.0)
                        .suffix(" pt"),
                )
                .on_hover_text("Distance from the screen edge when the overlay is placed");
                ui.end_row();

                ui.label("Window:");
                if ui
                    .button("Move back to the corner")
                    .on_hover_text(
                        "Forget where the window was dragged and place it again in the corner",
                    )
                    .clicked()
                {
                    self.cfg.window = None;
                    self.posizionato = false;
                    ctx_root.request_repaint();
                }
                ui.end_row();
            });
    }

    /// Lettura del contatore di slide dallo schermo.
    fn scheda_ocr(&mut self, ui: &mut egui::Ui, ctx_root: &egui::Context) {
        egui::Grid::new("scheda_ocr")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("OCR:");
                if ui
                    .checkbox(
                        &mut self.cfg.ocr.enabled,
                        "read the slide counter from the screen",
                    )
                    .on_hover_text(
                        "Off = plain timer. The slide number can still be stepped \
                         with the ← → arrow keys",
                    )
                    .changed()
                {
                    self.sincronizza_ocr_condiviso();
                    if self.cfg.ocr.enabled {
                        self.accendi_ocr(ctx_root);
                    } else {
                        self.stato_ocr = "OCR disabled: timer only".to_string();
                        if matches!(self.fonte_slide, Some(FonteSlide::Ocr(_))) {
                            self.slide_letta = None;
                            self.fonte_slide = None;
                        }
                    }
                }
                ui.end_row();

                ui.label("Monitor:");
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.cfg.ocr.monitor == 0, "Auto")
                        .on_hover_text("Follow the display the Ritardometro is on")
                        .clicked()
                    {
                        self.cfg.ocr.monitor = 0;
                        self.sincronizza_ocr_condiviso();
                        self.avvia_evidenziazione_ocr();
                    }
                    for idx in 1..=self.num_monitor.max(1) {
                        if ui
                            .selectable_label(self.cfg.ocr.monitor == idx, format!("Mon {idx}"))
                            .clicked()
                        {
                            self.cfg.ocr.monitor = idx;
                            self.sincronizza_ocr_condiviso();
                            self.avvia_evidenziazione_ocr();
                        }
                    }
                });
                ui.end_row();

                ui.label("Auto-detect:");
                if ui
                    .checkbox(&mut self.cfg.ocr.auto_zone, "find the counter in any corner")
                    .on_hover_text(
                        "If the area below reads nothing for a few seconds, scan the other \
                         corners and lock onto the one showing the counter",
                    )
                    .changed()
                {
                    if !self.cfg.ocr.auto_zone {
                        self.zona_ocr = None;
                    }
                    self.sincronizza_ocr_condiviso();
                    self.avvia_evidenziazione_ocr();
                }
                ui.end_row();

                ui.label("Capture area:");
                ui.horizontal(|ui| {
                    let mut cambiata = false;
                    for (etichetta, valore) in [
                        ("x", &mut self.cfg.ocr.x),
                        ("y", &mut self.cfg.ocr.y),
                        ("w", &mut self.cfg.ocr.width),
                        ("h", &mut self.cfg.ocr.height),
                    ] {
                        let mut pct = *valore * 100.0;
                        if ui
                            .add(
                                egui::DragValue::new(&mut pct)
                                    .speed(0.5)
                                    .range(0.5..=100.0)
                                    .max_decimals(0)
                                    .prefix(format!("{etichetta} "))
                                    .suffix("%"),
                            )
                            .changed()
                        {
                            *valore = pct / 100.0;
                            cambiata = true;
                        }
                    }
                    if cambiata {
                        let o = &mut self.cfg.ocr;
                        o.x = o.x.clamp(0.0, 0.99);
                        o.y = o.y.clamp(0.0, 0.99);
                        o.width = o.width.min(1.0 - o.x).max(0.005);
                        o.height = o.height.min(1.0 - o.y).max(0.005);
                        self.sincronizza_ocr_condiviso();
                        self.avvia_evidenziazione_ocr();
                    }
                });
                ui.end_row();

                ui.label("");
                if ui
                    .button("Highlight the zone on screen")
                    .on_hover_text("Flash a spotlight over the area being captured")
                    .clicked()
                {
                    self.avvia_evidenziazione_ocr();
                    ctx_root.request_repaint();
                }
                ui.end_row();

                ui.label("Rate:");
                ui.horizontal(|ui| {
                    for (ms, etichetta) in
                        [(200_u64, "200 ms"), (350, "350 ms"), (500, "500 ms")]
                    {
                        if ui
                            .selectable_label(self.cfg.ocr.interval_ms == ms, etichetta)
                            .clicked()
                        {
                            self.cfg.ocr.interval_ms = ms;
                            self.sincronizza_ocr_condiviso();
                        }
                    }
                });
                ui.end_row();

                ui.label("Magnification:");
                if ui
                    .add(
                        egui::DragValue::new(&mut self.cfg.ocr.magnification)
                            .speed(1.0)
                            .range(1..=6)
                            .suffix("×"),
                    )
                    .on_hover_text("Small counters are recognised much better when enlarged")
                    .changed()
                {
                    self.sincronizza_ocr_condiviso();
                }
                ui.end_row();

                ui.label("Debug:");
                if ui
                    .checkbox(
                        &mut self.cfg.ocr.save_debug,
                        "save ritardometro-debug.png on every capture",
                    )
                    .changed()
                {
                    self.sincronizza_ocr_condiviso();
                }
                ui.end_row();
            });
    }

    /// Le bandierine: un fumetto che compare a un certo punto del talk.
    fn scheda_milestone(&mut self, ui: &mut egui::Ui) {
        let mut cambiate = false;
        let mut da_togliere = None;

        for (i, m) in self.cfg.milestone.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let a_percentuale = m.percentage.is_some();
                if ui
                    .selectable_label(a_percentuale, "at %")
                    .on_hover_text("Fires at a percentage of the talk")
                    .clicked()
                    && !a_percentuale
                {
                    m.percentage = Some(50.0);
                    m.minutes_left = None;
                    cambiate = true;
                }
                if ui
                    .selectable_label(!a_percentuale, "min left")
                    .on_hover_text("Fires when this many minutes are left")
                    .clicked()
                    && a_percentuale
                {
                    m.minutes_left = Some(5.0);
                    m.percentage = None;
                    cambiate = true;
                }

                if let Some(p) = m.percentage.as_mut() {
                    cambiate |= ui
                        .add(
                            egui::DragValue::new(p)
                                .speed(1.0)
                                .range(1.0..=99.0)
                                .suffix("%"),
                        )
                        .changed();
                } else if let Some(min) = m.minutes_left.as_mut() {
                    cambiate |= ui
                        .add(
                            egui::DragValue::new(min)
                                .speed(0.5)
                                .range(0.5..=600.0)
                                .suffix(" min"),
                        )
                        .changed();
                }

                cambiate |= ui
                    .add(
                        egui::TextEdit::singleline(&mut m.text)
                            .desired_width(130.0)
                            .char_limit(24)
                            .hint_text("balloon text"),
                    )
                    .changed();

                if ui.button("✕").on_hover_text("Remove").clicked() {
                    da_togliere = Some(i);
                }
            });
        }

        if let Some(i) = da_togliere {
            self.cfg.milestone.remove(i);
            cambiate = true;
        }

        ui.add_space(4.0);
        if ui.button("+ Add milestone").clicked() {
            self.cfg.milestone.push(crate::config::MilestoneConfig {
                percentage: Some(50.0),
                minutes_left: None,
                text: "HALF WAY!".to_string(),
            });
            cambiate = true;
        }
        if self.cfg.milestone.is_empty() {
            ui.label(
                egui::RichText::new("No milestones: the avatar never raises a balloon.")
                    .small()
                    .color(GRIGIO),
            );
        }

        if cambiate {
            self.ricalcola_milestone();
        }
    }

    /// Rifà la lista delle bandierine dai minuti/percentuali correnti.
    fn ricalcola_milestone(&mut self) {
        self.bandierine = Bandierine::new(pacing::risolvi_milestone(
            &self.cfg.milestone,
            self.cfg.durata_secondi(),
        ));
    }

    /// Avvia la sorveglianza dello schermo quando viene accesa a programma già partito.
    fn accendi_schermo(&mut self, ctx: &egui::Context) {
        if self.schermo_rx.is_some() || self.demo.is_some() {
            return;
        }
        self.schermo_rx = schermo::avvia(Arc::clone(&self.ocr_condiviso), ctx.clone()).ok();
    }

    fn avvia_evidenziazione_ocr(&mut self) {
        let centro = self
            .ocr_condiviso
            .lock()
            .ok()
            .and_then(|s| s.centro_finestra);
        if let Some(info) = ocr::info_zona_ocr(&self.ocr_cfg_effettiva(), centro) {
            self.evidenzia_ocr = Some((Instant::now(), info));
        }
    }

    /// Mostra per ~2.2 secondi un riquadro luminoso lampeggiante esattamente sopra
    /// la zona dello schermo (e sul monitor) in cui l'OCR legge il contatore delle slide.
    fn mostra_evidenziazione_ocr(&mut self, ctx: &egui::Context) {
        let Some((inizio, info)) = self.evidenzia_ocr else {
            return;
        };
        let trascorso = inizio.elapsed().as_secs_f32();
        if trascorso > 2.2 {
            self.evidenzia_ocr = None;
            return;
        }

        let (rx, ry, rw, rh) = info.rect_ocr;
        let titolo_evidenzia = "Ritardometro — OCR Area";
        let ppp = ctx.pixels_per_point().max(1.0);
        let builder = egui::ViewportBuilder::default()
            .with_title(titolo_evidenzia)
            .with_position(Pos2::new(rx as f32 / ppp, ry as f32 / ppp))
            .with_inner_size(Vec2::new(rw as f32 / ppp, rh as f32 / ppp))
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_taskbar(false);

        let id_ev = egui::ViewportId::from_hash_of("ritardometro_evidenzia_ocr");
        let modalita = if self.cfg.ocr.monitor == 0 {
            format!("AUTO -> DISPLAY {} of {}", info.numero_monitor, info.totale_monitor)
        } else {
            format!("DISPLAY {} of {}", info.numero_monitor, info.totale_monitor)
        };

        ctx.show_viewport_immediate(id_ev, builder, |ui, _class| {
            #[cfg(windows)]
            win32_drag::posiziona_finestra_per_titolo(titolo_evidenzia, rx, ry, rw, rh);

            let r = ui.max_rect();
            let p = ui.painter_at(r);
            let onda = (trascorso * 12.0).sin() * 0.5 + 0.5;
            let fade = ((2.2 - trascorso) / 0.4).clamp(0.0, 1.0);

            p.rect_filled(
                r.shrink(2.0),
                CornerRadius::same(10),
                Color32::from_black_alpha((185.0 * fade) as u8),
            );
            let col_bordo = if onda > 0.5 { GIALLO } else { VERDE };
            p.rect_stroke(
                r.shrink(3.0),
                CornerRadius::same(10),
                Stroke::new(4.0, col_bordo.linear_multiply(fade)),
                egui::StrokeKind::Inside,
            );

            let c = r.center();
            p.text(
                Pos2::new(c.x, c.y - 10.0),
                Align2::CENTER_CENTER,
                &modalita,
                FontId::proportional(20.0),
                GIALLO.linear_multiply(fade),
            );
            p.text(
                Pos2::new(c.x, c.y + 12.0),
                Align2::CENTER_CENTER,
                format!("Slide OCR Area ({}×{})", info.dim_monitor.0, info.dim_monitor.1),
                FontId::proportional(13.0),
                Color32::WHITE.linear_multiply(fade),
            );
        });
        ctx.request_repaint();
    }

    fn sincronizza_ocr_condiviso(&self) {
        if let Ok(mut s) = self.ocr_condiviso.lock() {
            s.cfg = self.cfg.ocr.clone();
            s.in_pausa = self.in_pausa();
        }
    }

    fn aggiorna_centro_finestra_ocr(&mut self, ctx: &egui::Context) {
        let mut riquadro: Option<(i32, i32, i32, i32)> = None;
        #[cfg(windows)]
        if self.hwnd_principale != 0 {
            let mut rect = win32_drag::Rect::default();
            if unsafe { win32_drag::GetWindowRect(self.hwnd_principale, &mut rect) } != 0 {
                riquadro = Some((rect.left, rect.top, rect.right, rect.bottom));
            }
        }
        if riquadro.is_none() {
            if let Some(outer) = ctx.input(|i| i.viewport().outer_rect) {
                let ppp = ctx.pixels_per_point();
                riquadro = Some((
                    (outer.left() * ppp) as i32,
                    (outer.top() * ppp) as i32,
                    (outer.right() * ppp) as i32,
                    (outer.bottom() * ppp) as i32,
                ));
            }
        }
        self.rect_finestra = riquadro.or(self.rect_finestra);
        if let Ok(mut s) = self.ocr_condiviso.lock() {
            if let Some(r @ (sinistra, alto, destra, basso)) = riquadro {
                s.centro_finestra = Some(((sinistra + destra) / 2, (alto + basso) / 2));
                s.rect_finestra = Some(r);
            }
            s.in_pausa = self.in_pausa();
        }
    }

    fn andatura(&self, trascorsi: f32) -> Option<Andatura> {
        pacing::andatura(
            trascorsi,
            self.cfg.durata_secondi(),
            self.slide()?,
            self.totale()?,
            self.cfg.tolerance_seconds,
            self.cfg.panic_seconds,
        )
    }

    /// Cattura l'HWND della finestra principale e rimuove il bordo nativo di 1px di Windows 11 / winit.
    #[cfg(windows)]
    fn aggiorna_hwnd_e_bordo_win32(&mut self) {
        let hwnd_attivo = unsafe { win32_drag::GetActiveWindow() };
        if hwnd_attivo != 0 && !self.menu_aperto {
            self.hwnd_principale = hwnd_attivo;
        }
        if self.hwnd_principale != 0 {
            win32_drag::rimuovi_bordo_finestra(self.hwnd_principale);
            self.bordo_rimosso = true;
        }
    }

    /// Su Windows gestisce il ridimensionamento direttamente in coordinate fisiche dello schermo
    /// ancorando il lato/angolo opposto, così tirando l'angolo in alto a sinistra (`NorthWest`)
    /// l'angolo in basso a destra rimane fisso invece di allargarsi verso destra/basso.
    #[cfg(windows)]
    fn aggiorna_resize_win32(
        &mut self,
        ctx: &egui::Context,
        avvia_dir: Option<egui::ResizeDirection>,
    ) {
        if let Some(dir) = avvia_dir {
            if self.resize_win32.is_none() && self.hwnd_principale != 0 {
                let mut cur = win32_drag::Point::default();
                let mut rect = win32_drag::Rect::default();
                unsafe {
                    if win32_drag::GetCursorPos(&mut cur) != 0
                        && win32_drag::GetWindowRect(self.hwnd_principale, &mut rect) != 0
                    {
                        self.resize_win32 = Some(ResizeWin32 {
                            hwnd: self.hwnd_principale,
                            dir,
                            start_cur: cur,
                            start_rect: rect,
                            scala0: self.cfg.scale,
                        });
                    }
                }
            }
        }

        if let Some(rw) = self.resize_win32 {
            let giu = unsafe {
                (win32_drag::GetAsyncKeyState(win32_drag::VK_LBUTTON) as u16 & 0x8000) != 0
            };
            if !giu {
                self.resize_win32 = None;
                self.resize_attivo = None;
            } else {
                let mut cur = win32_drag::Point::default();
                unsafe {
                    if win32_drag::GetCursorPos(&mut cur) != 0 {
                        let w0 = (rw.start_rect.right - rw.start_rect.left).max(1) as f32;
                        let h0 = (rw.start_rect.bottom - rw.start_rect.top).max(1) as f32;
                        let base_w = w0 / rw.scala0;
                        let base_h = h0 / rw.scala0;
                        let dx = (cur.x - rw.start_cur.x) as f32 / base_w;
                        let dy = (cur.y - rw.start_cur.y) as f32 / base_h;

                        use egui::ResizeDirection::*;
                        let nuova_scala = match rw.dir {
                            East => rw.scala0 + dx,
                            South => rw.scala0 + dy,
                            SouthEast => rw.scala0 + (dx + dy) * 0.5,
                            West => rw.scala0 - dx,
                            North => rw.scala0 - dy,
                            NorthWest => rw.scala0 - (dx + dy) * 0.5,
                            NorthEast => rw.scala0 + (dx - dy) * 0.5,
                            SouthWest => rw.scala0 + (dy - dx) * 0.5,
                        }
                        .clamp(0.5, 3.0);

                        let new_w = (base_w * nuova_scala).round() as i32;
                        let new_h = (base_h * nuova_scala).round() as i32;

                        let new_x = match rw.dir {
                            West | NorthWest | SouthWest => rw.start_rect.right - new_w,
                            _ => rw.start_rect.left,
                        };
                        let new_y = match rw.dir {
                            North | NorthWest | NorthEast => rw.start_rect.bottom - new_h,
                            _ => rw.start_rect.top,
                        };

                        self.cfg.scale = nuova_scala;
                        win32_drag::SetWindowPos(
                            rw.hwnd,
                            0,
                            new_x,
                            new_y,
                            new_w,
                            new_h,
                            win32_drag::SWP_NOZORDER | win32_drag::SWP_NOACTIVATE,
                        );
                    }
                }
                ctx.request_repaint();
            }
        }
    }

    /// Su Windows gestisce lo spostamento della finestra direttamente tramite coordinate
    /// fisiche del cursore e del centro finestra (`SetWindowPos`), evitando il bug di `winit`
    /// che durante `WM_DPICHANGED` riporta la finestra sul primo monitor.
    #[cfg(windows)]
    fn aggiorna_trascinamento_win32(&mut self, ctx: &egui::Context, avvia_drag: bool) {
        self.trascinato_appena = false;

        if avvia_drag && self.trascinamento.is_none() && self.hwnd_principale != 0 {
            let hwnd = self.hwnd_principale;
            let mut cur = win32_drag::Point::default();
            let mut rect = win32_drag::Rect::default();
            unsafe {
                if win32_drag::GetCursorPos(&mut cur) != 0
                    && win32_drag::GetWindowRect(hwnd, &mut rect) != 0
                {
                    let cx = (rect.left + rect.right) / 2;
                    let cy = (rect.top + rect.bottom) / 2;
                    self.trascinamento = Some(TrascinamentoWin32 {
                        hwnd,
                        offset_cx: cur.x - cx,
                        offset_cy: cur.y - cy,
                        start_x: cur.x,
                        start_y: cur.y,
                        mosso: false,
                    });
                }
            }
        }

        if let Some(mut tr) = self.trascinamento {
            let giu = unsafe {
                (win32_drag::GetAsyncKeyState(win32_drag::VK_LBUTTON) as u16 & 0x8000) != 0
            };
            if !giu {
                self.trascinato_appena = tr.mosso;
                self.trascinamento = None;
            } else {
                let mut cur = win32_drag::Point::default();
                let mut rect = win32_drag::Rect::default();
                unsafe {
                    if win32_drag::GetCursorPos(&mut cur) != 0
                        && win32_drag::GetWindowRect(tr.hwnd, &mut rect) != 0
                    {
                        if (cur.x - tr.start_x).abs() > 3 || (cur.y - tr.start_y).abs() > 3 {
                            tr.mosso = true;
                        }
                        if tr.mosso {
                            let w = rect.right - rect.left;
                            let h = rect.bottom - rect.top;
                            let nuovo_cx = cur.x - tr.offset_cx;
                            let nuovo_cy = cur.y - tr.offset_cy;
                            let x = nuovo_cx - w / 2;
                            let y = nuovo_cy - h / 2;
                            win32_drag::SetWindowPos(
                                tr.hwnd,
                                0,
                                x,
                                y,
                                0,
                                0,
                                win32_drag::SWP_NOSIZE
                                    | win32_drag::SWP_NOZORDER
                                    | win32_drag::SWP_NOACTIVATE,
                            );
                        }
                    }
                }
                self.trascinamento = Some(tr);
                ctx.request_repaint();
            }
        }
    }
}

/// Le fasi del talk, per decidere cosa mostrare.
enum Fase {
    Attesa,
    Talk,
    Domande { restanti: f32 },
    Concluso,
    Sforato { oltre: f32 },
}

impl eframe::App for Ritardometro {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // finestra trasparente
    }

    /// Alla chiusura si scrive subito quello che aspettava il ritardo.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.salvabile && self.salva_da.is_some() {
            self.scrivi_config();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(windows)]
        self.aggiorna_hwnd_e_bordo_win32();
        self.posiziona(&ctx);
        self.aggiorna_centro_finestra_ocr(&ctx);
        self.simula();
        self.ricevi_ocr();
        self.ricevi_schermo();
        // Con il cronometro fermo non si accumula stallo: la schermata ferma
        // mentre si è in pausa non è un ritardo.
        if !self.crono.in_corso() {
            self.ultimo_cambio = Instant::now();
        }
        self.tastiera(&ctx);

        let area = ui.max_rect();
        // La scala di disegno si adatta sempre alla dimensione attuale della finestra.
        let s = (area.width() / LARGHEZZA)
            .min(area.height() / ALTEZZA)
            .clamp(0.5, 3.0);

        let t = self.nascita.elapsed().as_secs_f32();
        let durata = self.cfg.durata_secondi();
        let domande = self.cfg.questions_minutes * 60.0;
        let trascorsi = self.crono.trascorsi();
        self.bandierine.aggiorna(trascorsi, t);
        let andatura = self.andatura(trascorsi);
        let totale = self.totale();

        // Sei arrivato all'ultima slide del talk?
        let ultima_slide = matches!(
            (self.slide(), totale),
            (Some(sl), Some(tot)) if tot >= 2 && sl >= tot
        );

        let fase = if self.crono.mai_avviato() {
            Fase::Attesa
        } else if trascorsi < durata {
            Fase::Talk
        } else {
            let oltre = trascorsi - durata;
            // Se a fine tempo sei ancora lontano dall'ultima slide, hai sforato davvero.
            let indietro = matches!((self.slide(), totale), (Some(sl), Some(tot)) if sl + 1 < tot);
            if !indietro && oltre < domande {
                Fase::Domande {
                    restanti: domande - oltre,
                }
            } else if !indietro && domande <= 0.0 && oltre < 30.0 {
                Fase::Concluso
            } else {
                Fase::Sforato { oltre }
            }
        };

        // ---------------- interazione: spostamento vs ridimensionamento ----------------
        let (hover_pos, interact_pos, premuto, giu) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.pointer.interact_pos(),
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
            )
        });

        let mut avvia_drag_win32 = false;
        let mut avvia_resize_dir = None;
        if !giu {
            self.resize_attivo = None;
        } else if premuto {
            if let Some(pos) = interact_pos.or(hover_pos) {
                if let Some(dir) = zona_resize(area, pos, s) {
                    self.resize_attivo = Some((dir, pos, self.cfg.scale));
                    avvia_resize_dir = Some(dir);
                } else if area.contains(pos) {
                    avvia_drag_win32 = true;
                }
            }
        }

        let dir_hover = hover_pos.and_then(|p| zona_resize(area, p, s));
        #[cfg(windows)]
        let dir_attivo = self
            .resize_win32
            .map(|r| r.dir)
            .or_else(|| self.resize_attivo.map(|(d, _, _)| d));
        #[cfg(not(windows))]
        let dir_attivo = self.resize_attivo.map(|(d, _, _)| d);

        if let Some(dir) = dir_attivo {
            ctx.set_cursor_icon(cursore_resize(dir));
        } else if let Some(dir) = dir_hover {
            ctx.set_cursor_icon(cursore_resize(dir));
        }

        let risposta = ui.interact(area, Id::unique("overlay"), Sense::click_and_drag());

        #[cfg(windows)]
        {
            self.aggiorna_resize_win32(&ctx, avvia_resize_dir);
            if self.resize_win32.is_none() {
                self.aggiorna_trascinamento_win32(&ctx, avvia_drag_win32);
            }
        }

        #[cfg(not(windows))]
        let _ = (avvia_drag_win32, avvia_resize_dir);

        #[cfg(not(windows))]
        if let Some((dir, origine, scala0)) = self.resize_attivo {
            if let Some(pos) = interact_pos {
                use egui::ResizeDirection::*;
                let dx = (pos.x - origine.x) / LARGHEZZA;
                let dy = (pos.y - origine.y) / ALTEZZA;
                let nuova_scala = match dir {
                    East => scala0 + dx,
                    South => scala0 + dy,
                    SouthEast => scala0 + (dx + dy) * 0.5,
                    West => scala0 - dx,
                    North => scala0 - dy,
                    NorthWest => scala0 - (dx + dy) * 0.5,
                    NorthEast => scala0 + (dx - dy) * 0.5,
                    SouthWest => scala0 + (dy - dx) * 0.5,
                };
                self.cambia_scala(&ctx, nuova_scala);
            }
        } else if risposta.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        #[cfg(windows)]
        let appena_trascinato = self.trascinato_appena;
        #[cfg(not(windows))]
        let appena_trascinato = false;

        if risposta.clicked()
            && !appena_trascinato
            && dir_attivo.is_none()
            && dir_hover.is_none()
        {
            self.alterna_pausa();
        }

        // Click destro: apre/chiude la finestra di configurazione esterna
        if risposta.secondary_clicked() {
            self.menu_aperto = !self.menu_aperto;
            if self.menu_aperto {
                self.num_monitor = ocr::numero_monitor();
                if let Some(outer) = ctx.input(|i| i.viewport().outer_rect) {
                    let x = if outer.left() > 395.0 {
                        outer.left() - 390.0
                    } else {
                        outer.right() + 10.0
                    };
                    let y = outer.top().max(20.0);
                    self.menu_pos = Some(Pos2::new(x, y));
                }
            }
        }
        self.mostra_finestra_menu(&ctx);
        self.mostra_evidenziazione_ocr(&ctx);
        self.ricorda_posizione();
        self.salva_preferenze();

        // ---------------- stato dell'avatar ----------------
        let stallo = self.stallo(&fase, andatura.as_ref());
        let (mut stato, colore_scarto) =
            stato_avatar(&self.cfg, &fase, andatura.as_ref(), ultima_slide, stallo);
        stato.opacita = self.cfg.opacity;
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
        let op = self.cfg.opacity.clamp(0.15, 1.0);
        let applica_op = |c: Color32| -> Color32 {
            if op >= 0.999 {
                c
            } else {
                c.linear_multiply(op)
            }
        };

        let painter = ui.painter_at(area);
        let fondo = area.shrink(2.0 * s);
        painter.rect_filled(
            fondo,
            CornerRadius::same((18.0 * s).clamp(4.0, 255.0) as u8),
            Color32::from_black_alpha((self.cfg.background_opacity * op * 255.0) as u8),
        );

        // Maniglia di ridimensionamento nell'angolo in basso a destra
        let in_resize = dir_attivo.is_some() || dir_hover.is_some();
        if risposta.hovered() || in_resize {
            let alpha = if in_resize { 220 } else { 110 };
            let c_grip = applica_op(Color32::from_white_alpha(alpha));
            let bx = fondo.max.x - 6.0 * s;
            let by = fondo.max.y - 6.0 * s;
            for d in [5.0_f32, 9.5, 14.0] {
                painter.line_segment(
                    [Pos2::new(bx - d * s, by), Pos2::new(bx, by - d * s)],
                    Stroke::new((1.6 * s).max(1.2), c_grip),
                );
            }
        }

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
            Fase::Concluso => ("00:00".to_string(), VERDE),
            Fase::Sforato { oltre } => (format!("+{}", pacing::mm_ss(*oltre)), ROSSO),
        };
        let lampeggia =
            !self.crono.in_corso() && !self.crono.mai_avviato() && (t * 1.6).sin() < 0.0;
        let colore_tempo = if lampeggia {
            colore_tempo.gamma_multiply(0.45)
        } else {
            colore_tempo
        };
        testo_contornato(
            &painter,
            Pos2::new(cx, y),
            Align2::CENTER_TOP,
            tempo,
            FontId::monospace(40.0 * s),
            applica_op(colore_tempo),
            applica_op(CONTORNO),
        );
        y += 46.0 * s;

        // riga di stato
        let riga = match (&fase, &andatura) {
            (Fase::Attesa, _) => match (self.slide(), totale) {
                (Some(sl), Some(tot)) => format!("slide {sl}/{tot} — click or advance slide"),
                (Some(sl), None) => format!("slide {sl} — click or advance slide"),
                _ if self.cfg.auto_start && self.ocr_rx.is_some() => {
                    "click or advance slide to start".to_string()
                }
                _ => "click to start".to_string(),
            },
            (Fase::Domande { .. }, _) => "applause! Q&A time".to_string(),
            (Fase::Concluso, _) => "talk concluded! applause!".to_string(),
            (Fase::Sforato { .. }, _) => "time's up!".to_string(),
            (Fase::Talk, _) if ultima_slide => format!(
                "slide {}/{}  talk finished! applause!",
                self.slide().unwrap_or(0),
                totale.unwrap_or(0),
            ),
            (Fase::Talk, Some(a)) => {
                let giudizio = match a.ritmo {
                    Ritmo::InLinea => "on track",
                    Ritmo::Anticipo => "ahead",
                    Ritmo::Ritardo => "behind",
                    Ritmo::Panico => "speed up!",
                };
                format!(
                    "slide {}/{}  {} {}",
                    self.slide().unwrap_or(0),
                    totale.unwrap_or(0),
                    pacing::scarto(a.scarto_secondi),
                    giudizio
                )
            }
            (Fase::Talk, None) if stallo > 0.0 => format!(
                "same screen for {}",
                pacing::mm_ss(self.ultimo_cambio.elapsed().as_secs_f32())
            ),
            (Fase::Talk, None) => match self.slide() {
                Some(sl) => format!("slide {sl}  (unknown total)"),
                None => "remaining time".to_string(),
            },
        };
        testo_contornato(
            &painter,
            Pos2::new(cx, y),
            Align2::CENTER_TOP,
            riga,
            FontId::proportional(14.0 * s),
            applica_op(colore_scarto),
            applica_op(CONTORNO),
        );
        y += 21.0 * s;

        // barra: tempo trascorso (bianco) e posizione nelle slide (tacca colorata)
        let barra = Rect::from_min_size(
            Pos2::new(area.min.x + 18.0 * s, y),
            Vec2::new(area.width() - 36.0 * s, 6.0 * s),
        );
        // Cornice scura dietro la barra: senza, su una slide bianca sparirebbe
        // anche lei, essendo disegnata in bianco trasparente.
        painter.rect_filled(
            barra.expand(1.6 * s),
            CornerRadius::same((4.0 * s) as u8),
            applica_op(CONTORNO),
        );
        painter.rect_filled(
            barra,
            CornerRadius::same((3.0 * s) as u8),
            applica_op(Color32::from_white_alpha(40)),
        );
        let p_tempo = (trascorsi / durata).clamp(0.0, 1.0);
        let mut pieno = barra;
        pieno.set_width(barra.width() * p_tempo);
        painter.rect_filled(
            pieno,
            CornerRadius::same((3.0 * s) as u8),
            applica_op(Color32::from_white_alpha(170)),
        );
        if let (Some(sl), Some(tot)) = (self.slide(), totale) {
            if tot > 0 {
                let p = ((sl.saturating_sub(1)) as f32 / tot as f32).clamp(0.0, 1.0);
                let x = barra.min.x + barra.width() * p;
                for (spessore, colore) in [
                    (5.4 * s, applica_op(CONTORNO)),
                    (3.0 * s, applica_op(colore_scarto)),
                ] {
                    painter.line_segment(
                        [
                            Pos2::new(x, barra.min.y - 4.0 * s),
                            Pos2::new(x, barra.max.y + 4.0 * s),
                        ],
                        Stroke::new(spessore, colore),
                    );
                }
            }
        }
        y += 14.0 * s;

        // riga piccola: pausa, stato OCR o avvisi
        let nota = if let Some(a) = &self.avviso {
            accorcia(a.lines().next().unwrap_or(a), 40)
        } else if !self.crono.in_corso() && !self.crono.mai_avviato() {
            "paused".to_string()
        } else {
            accorcia(&self.stato_ocr, 40)
        };
        testo_contornato(
            &painter,
            Pos2::new(cx, y),
            Align2::CENTER_TOP,
            nota,
            FontId::proportional(10.5 * s),
            applica_op(GRIGIO),
            applica_op(CONTORNO),
        );

        // L'avviso sparisce dopo il primo avvio.
        if self.crono.in_corso() {
            self.avviso = None;
        }

        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

/// Traduce il ritmo del talk in espressione e posizione delle braccia.
fn stato_avatar(
    cfg: &Config,
    fase: &Fase,
    andatura: Option<&Andatura>,
    ultima_slide: bool,
    stallo: f32,
) -> (StatoAvatar, Color32) {
    let tol = cfg.tolerance_seconds;
    let pan = cfg.panic_seconds;
    let frazione = |x: f32| ((x - tol) / (pan - tol).max(1.0)).clamp(0.0, 1.0);

    match fase {
        Fase::Attesa => (StatoAvatar::default(), GRIGIO),
        Fase::Domande { .. } => (
            StatoAvatar {
                umore: 1.0,
                alza_verde: 0.8,
                alza_rosso: 0.0,
                sudore: false,
                applauso: true,
                orologio: 0.0,
                opacita: 1.0,
                bandiera: Some(Bandiera {
                    testo: "Q&A?".into(),
                    colore: GIALLO,
                    alzata: 1.0,
                }),
            },
            GIALLO,
        ),
        Fase::Concluso => (
            StatoAvatar {
                umore: 1.0,
                alza_verde: 1.0,
                alza_rosso: 0.0,
                sudore: false,
                applauso: true,
                orologio: 0.0,
                opacita: 1.0,
                bandiera: Some(Bandiera {
                    testo: "GREAT!".into(),
                    colore: VERDE,
                    alzata: 1.0,
                }),
            },
            VERDE,
        ),
        Fase::Sforato { .. } => (
            StatoAvatar {
                umore: -1.0,
                alza_verde: 0.0,
                alza_rosso: 1.0,
                sudore: true,
                applauso: false,
                orologio: 0.0,
                opacita: 1.0,
                bandiera: Some(Bandiera {
                    testo: "STOP!".into(),
                    colore: ROSSO,
                    alzata: 1.0,
                }),
            },
            ROSSO,
        ),
        Fase::Talk if ultima_slide => (
            StatoAvatar {
                umore: 1.0,
                alza_verde: 1.0,
                alza_rosso: 0.0,
                sudore: false,
                applauso: true,
                orologio: 0.0,
                opacita: 1.0,
                bandiera: Some(Bandiera {
                    testo: "GREAT!".into(),
                    colore: VERDE,
                    alzata: 1.0,
                }),
            },
            VERDE,
        ),
        // Senza contatore slide l'unico segnale è la schermata che non cambia:
        // l'avatar si tocca il polso invece di restare neutro e muto.
        Fase::Talk if andatura.is_none() && stallo > 0.0 => (
            StatoAvatar {
                umore: 0.35 - 0.95 * stallo,
                alza_verde: 0.0,
                alza_rosso: 0.0,
                orologio: stallo,
                ..Default::default()
            },
            if stallo > 0.5 { ARANCIO } else { Color32::WHITE },
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
                            applauso: false,
                            orologio: 0.0,
                            opacita: 1.0,
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

/// Colore del contorno di testi e barra: quasi nero, come l'inchiostro della
/// caricatura.
const CONTORNO: Color32 = Color32::from_rgb(14, 10, 12);

/// Scrive il testo con un contorno scuro intorno.
///
/// Con lo sfondo dell'overlay trasparente l'HUD finisce direttamente sopra la
/// slide: su una slide bianca il testo bianco sparirebbe. Il contorno costa
/// quattro passate in più e rende la scritta leggibile su qualsiasi fondo,
/// con lo stesso tratto da fumetto dell'avatar.
#[allow(clippy::too_many_arguments)]
fn testo_contornato(
    painter: &egui::Painter,
    pos: Pos2,
    ancora: Align2,
    testo: impl ToString,
    font: FontId,
    colore: Color32,
    contorno: Color32,
) {
    let testo = testo.to_string();
    let spessore = (font.size * 0.085).max(1.0);
    for (dx, dy) in [
        (-1.0_f32, 0.0_f32),
        (1.0, 0.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-0.7, -0.7),
        (0.7, -0.7),
        (-0.7, 0.7),
        (0.7, 0.7),
    ] {
        painter.text(
            Pos2::new(pos.x + dx * spessore, pos.y + dy * spessore),
            ancora,
            &testo,
            font.clone(),
            contorno,
        );
    }
    painter.text(pos, ancora, testo, font, colore);
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

fn apri_url(url: &str) {
    // explorer.exe apre l'URL nel browser predefinito senza far lampeggiare una console
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Rileva se il puntatore si trova sui bordi o sulla maniglia in basso a destra per il ridimensionamento.
fn zona_resize(area: Rect, pos: Pos2, s: f32) -> Option<egui::ResizeDirection> {
    if !area.contains(pos) {
        return None;
    }
    let grip = (38.0 * s).clamp(28.0, 64.0);
    if pos.x >= area.max.x - grip && pos.y >= area.max.y - grip {
        return Some(egui::ResizeDirection::SouthEast);
    }

    let bordo = (14.0 * s).clamp(12.0, 24.0);
    let sx = pos.x <= area.min.x + bordo;
    let dx = pos.x >= area.max.x - bordo;
    let su = pos.y <= area.min.y + bordo;
    let giu = pos.y >= area.max.y - bordo;

    use egui::ResizeDirection::*;
    match (sx, dx, su, giu) {
        (true, _, true, _) => Some(NorthWest),
        (_, true, true, _) => Some(NorthEast),
        (true, _, _, true) => Some(SouthWest),
        (_, true, _, true) => Some(SouthEast),
        (true, _, _, _) => Some(West),
        (_, true, _, _) => Some(East),
        (_, _, true, _) => Some(North),
        (_, _, _, true) => Some(South),
        _ => None,
    }
}

fn cursore_resize(dir: egui::ResizeDirection) -> egui::CursorIcon {
    use egui::ResizeDirection::*;
    match dir {
        NorthWest | SouthEast => egui::CursorIcon::ResizeNwSe,
        NorthEast | SouthWest => egui::CursorIcon::ResizeNeSw,
        East | West => egui::CursorIcon::ResizeHorizontal,
        North | South => egui::CursorIcon::ResizeVertical,
    }
}

/// Tema scuro ad alto contrasto per la finestra di configurazione, indipendente
/// dal tema chiaro/scuro di sistema di Windows.
fn tema_scuro_contrasto() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    let bianco_testo = Color32::from_rgb(245, 247, 252);
    v.override_text_color = Some(bianco_testo);
    v.panel_fill = Color32::from_rgb(26, 28, 34);
    v.window_fill = Color32::from_rgb(26, 28, 34);

    v.widgets.noninteractive.bg_fill = Color32::from_rgb(38, 42, 52);
    v.widgets.noninteractive.weak_bg_fill = Color32::from_rgb(38, 42, 52);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(72, 78, 94));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, bianco_testo);

    v.widgets.inactive.bg_fill = Color32::from_rgb(56, 62, 78);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(56, 62, 78);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(95, 104, 128));
    v.widgets.inactive.fg_stroke = Stroke::new(1.2, Color32::WHITE);

    v.widgets.hovered.bg_fill = Color32::from_rgb(78, 88, 112);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(78, 88, 112);
    v.widgets.hovered.bg_stroke = Stroke::new(1.2, Color32::from_rgb(140, 155, 190));
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, Color32::WHITE);

    v.widgets.active.bg_fill = Color32::from_rgb(98, 112, 145);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(98, 112, 145);
    v.widgets.active.bg_stroke = Stroke::new(1.5, Color32::WHITE);
    v.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);

    v.selection.bg_fill = Color32::from_rgb(45, 125, 220);
    v.selection.stroke = Stroke::new(1.2, Color32::WHITE);
    v
}


