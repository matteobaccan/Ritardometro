//! L'omino cartoon disegnato in vettoriale con il painter di egui.
//!
//! Niente immagini da caricare: ogni parte (testa, faccia, braccia, guanti,
//! bandierina) è una forma geometrica, così si anima muovendo i singoli pezzi.
//! Le coordinate sono in uno spazio virtuale 200 x 230 scalato sul rettangolo.

use eframe::egui::{
    epaint::{CubicBezierShape, QuadraticBezierShape},
    Align2, Color32, FontId, Mesh, Painter, Pos2, Rect, Shape, Stroke, Vec2,
};

pub const LARGHEZZA: f32 = 200.0;
pub const ALTEZZA: f32 = 230.0;

pub const VERDE: Color32 = Color32::from_rgb(46, 204, 113);
pub const ROSSO: Color32 = Color32::from_rgb(231, 76, 60);
pub const GIALLO: Color32 = Color32::from_rgb(245, 197, 24);

const PELLE: Color32 = Color32::from_rgb(246, 201, 160);
const PELLE_OMBRA: Color32 = Color32::from_rgb(226, 173, 130);
const CAPELLI: Color32 = Color32::from_rgb(58, 42, 30);
const MAGLIA: Color32 = Color32::from_rgb(59, 111, 216);
const MAGLIA_SCURA: Color32 = Color32::from_rgb(44, 86, 172);
const INCHIOSTRO: Color32 = Color32::from_rgb(40, 32, 30);
const GOCCIA: Color32 = Color32::from_rgb(120, 190, 255);

#[derive(Debug, Clone)]
pub struct Bandiera {
    pub testo: String,
    pub colore: Color32,
    /// 0 = bandiera abbassata, 1 = alzata e sventolante
    pub alzata: f32,
}

#[derive(Debug, Clone)]
pub struct StatoAvatar {
    /// -1 disperato … 0 neutro … 1 felicissimo
    pub umore: f32,
    /// Quanto è alzata la mano col guanto verde (0..1)
    pub alza_verde: f32,
    /// Quanto è alzata la mano col guanto rosso (0..1)
    pub alza_rosso: f32,
    pub sudore: bool,
    pub bandiera: Option<Bandiera>,
}

impl Default for StatoAvatar {
    fn default() -> Self {
        Self {
            umore: 0.4,
            alza_verde: 0.2,
            alza_rosso: 0.2,
            sudore: false,
            bandiera: None,
        }
    }
}

/// Trasforma coordinate virtuali in coordinate schermo.
struct Tela<'a> {
    painter: &'a Painter,
    origine: Pos2,
    u: f32,
}

impl Tela<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origine + Vec2::new(x, y) * self.u
    }
    fn s(&self, v: f32) -> f32 {
        v * self.u
    }
    fn cerchio(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.painter.circle_filled(self.p(x, y), self.s(r), c);
    }
    fn linea(&self, a: (f32, f32), b: (f32, f32), w: f32, c: Color32) {
        self.painter.line_segment(
            [self.p(a.0, a.1), self.p(b.0, b.1)],
            Stroke::new(self.s(w), c),
        );
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn dir(gradi: f32) -> (f32, f32) {
    let r = gradi.to_radians();
    (r.cos(), r.sin())
}

/// Disegna l'avatar dentro `rect` (mantiene le proporzioni, centrato).
/// `t` è il tempo in secondi, usato per le animazioni continue.
pub fn disegna(painter: &Painter, rect: Rect, stato: &StatoAvatar, t: f32) {
    let u = (rect.width() / LARGHEZZA).min(rect.height() / ALTEZZA);
    let origine = rect.center() - Vec2::new(LARGHEZZA, ALTEZZA) * u * 0.5;
    let tela = Tela {
        painter,
        origine,
        u,
    };

    let umore = stato.umore.clamp(-1.0, 1.0);
    // Leggero dondolio continuo: l'omino "respira".
    let respiro = (t * 2.0).sin() * 1.2;

    let alza_rosso = match &stato.bandiera {
        Some(b) => stato.alza_rosso.max(b.alzata),
        None => stato.alza_rosso,
    };

    // Il busto (e quindi le spalle) dondola con il respiro.
    let spalla_sx = (66.0, 164.0 + respiro);
    let spalla_dx = (134.0, 164.0 + respiro);

    // ---------- braccia (dietro al busto) ----------
    let mano_sx = braccio(&tela, spalla_sx, stato.alza_verde, -1.0, VERDE, t);
    let mano_dx = braccio(&tela, spalla_dx, alza_rosso, 1.0, ROSSO, t + 1.3);

    // ---------- busto ----------
    let busto = Rect::from_min_max(tela.p(62.0, 150.0 + respiro), tela.p(138.0, 232.0));
    painter.rect_filled(busto, tela.s(24.0), MAGLIA);
    // collo
    tela.cerchio(100.0, 146.0 + respiro, 11.0, PELLE_OMBRA);
    // scollo a V
    painter.add(Shape::convex_polygon(
        vec![
            tela.p(88.0, 151.0 + respiro),
            tela.p(112.0, 151.0 + respiro),
            tela.p(100.0, 166.0 + respiro),
        ],
        PELLE_OMBRA,
        Stroke::NONE,
    ));
    // laccetto con il badge da speaker
    tela.linea(
        (89.0, 152.0 + respiro),
        (108.0, 190.0 + respiro),
        2.0,
        MAGLIA_SCURA,
    );
    tela.linea(
        (111.0, 152.0 + respiro),
        (112.0, 190.0 + respiro),
        2.0,
        MAGLIA_SCURA,
    );
    let badge = Rect::from_min_max(
        tela.p(100.0, 188.0 + respiro),
        tela.p(124.0, 204.0 + respiro),
    );
    painter.rect_filled(badge, tela.s(3.0), Color32::WHITE);
    painter.rect_filled(
        Rect::from_min_max(
            tela.p(100.0, 188.0 + respiro),
            tela.p(124.0, 193.0 + respiro),
        ),
        tela.s(2.0),
        ROSSO,
    );
    tela.linea(
        (104.0, 198.0 + respiro),
        (120.0, 198.0 + respiro),
        1.5,
        Color32::GRAY,
    );

    // ---------- mani davanti al busto ----------
    guanto(&tela, mano_sx, VERDE, -1.0);
    guanto(&tela, mano_dx, ROSSO, 1.0);

    // ---------- bandierina ----------
    if let Some(b) = &stato.bandiera {
        if b.alzata > 0.05 {
            bandiera(&tela, mano_dx, b, t);
        }
    }

    // ---------- testa ----------
    testa(&tela, umore, stato.sudore, t, respiro * 0.6);
}

/// Disegna un braccio e restituisce la posizione della mano.
/// `lato` = -1 per il braccio a sinistra dello schermo, +1 per quello a destra.
fn braccio(
    tela: &Tela,
    spalla: (f32, f32),
    alza: f32,
    lato: f32,
    _guanto: Color32,
    t: f32,
) -> (f32, f32) {
    let alza = alza.clamp(0.0, 1.0);
    let ondeggia = (t * 1.7).sin() * 4.0;
    // Angoli in gradi (y verso il basso): giù-in-fuori -> su-in-fuori.
    let (giu, su) = if lato < 0.0 {
        (115.0, 245.0)
    } else {
        (65.0, -65.0)
    };
    let a = lerp(giu, su, alza) + ondeggia * lato;
    let piega = lerp(-14.0, 14.0, alza);
    let a2 = if lato < 0.0 { a + piega } else { a - piega };

    let (dx1, dy1) = dir(a);
    let gomito = (spalla.0 + dx1 * 30.0, spalla.1 + dy1 * 30.0);
    let (dx2, dy2) = dir(a2);
    let mano = (gomito.0 + dx2 * 28.0, gomito.1 + dy2 * 28.0);

    tela.linea(spalla, gomito, 12.0, MAGLIA);
    tela.cerchio(gomito.0, gomito.1, 6.0, MAGLIA);
    tela.linea(gomito, mano, 11.0, MAGLIA);
    mano
}

fn guanto(tela: &Tela, mano: (f32, f32), colore: Color32, lato: f32) {
    // polsino bianco
    tela.cerchio(mano.0, mano.1, 7.5, Color32::WHITE);
    // pollice
    tela.cerchio(mano.0 - lato * 8.0, mano.1 - 2.0, 4.5, colore);
    // palmo
    tela.cerchio(mano.0, mano.1 - 3.0, 10.0, colore);
    tela.painter.circle_stroke(
        tela.p(mano.0, mano.1 - 3.0),
        tela.s(10.0),
        Stroke::new(tela.s(1.5), colore.gamma_multiply(0.7)),
    );
}

fn testa(tela: &Tela, umore: f32, sudore: bool, t: f32, dy: f32) {
    let cy = 104.0 + dy;
    let painter = tela.painter;

    // capelli dietro + orecchie
    tela.cerchio(100.0, cy - 7.0, 40.0, CAPELLI);
    tela.cerchio(63.0, cy + 2.0, 7.0, PELLE_OMBRA);
    tela.cerchio(137.0, cy + 2.0, 7.0, PELLE_OMBRA);
    // viso
    tela.cerchio(100.0, cy + 2.0, 36.0, PELLE);
    // ciuffo
    painter.add(QuadraticBezierShape::from_points_stroke(
        [
            tela.p(76.0, cy - 26.0),
            tela.p(98.0, cy - 6.0),
            tela.p(124.0, cy - 26.0),
        ],
        true,
        CAPELLI,
        Stroke::NONE,
    ));
    tela.linea((96.0, cy - 46.0), (92.0, cy - 54.0), 3.0, CAPELLI);
    tela.linea((102.0, cy - 46.0), (104.0, cy - 55.0), 3.0, CAPELLI);

    // guance rosa quando è contento
    if umore > 0.3 {
        let a = ((umore - 0.3) / 0.7 * 90.0) as u8;
        let rosa = Color32::from_rgba_unmultiplied(240, 110, 110, a);
        tela.cerchio(76.0, cy + 16.0, 6.0, rosa);
        tela.cerchio(124.0, cy + 16.0, 6.0, rosa);
    }

    // occhi (con battito di palpebre ogni ~4 secondi)
    let ammicca = (t % 4.3) < 0.13;
    let r_occhio = if umore < -0.6 { 4.6 } else { 3.6 };
    for x in [87.0, 113.0] {
        if ammicca {
            tela.linea((x - 4.0, cy + 2.0), (x + 4.0, cy + 2.0), 2.0, INCHIOSTRO);
        } else {
            tela.cerchio(x, cy + 2.0, r_occhio, INCHIOSTRO);
            tela.cerchio(x + 1.2, cy + 0.8, 1.1, Color32::WHITE);
        }
    }

    // occhiali tondi da sviluppatore
    let vetro = Stroke::new(tela.s(2.0), INCHIOSTRO);
    painter.circle_stroke(tela.p(87.0, cy + 2.0), tela.s(10.5), vetro);
    painter.circle_stroke(tela.p(113.0, cy + 2.0), tela.s(10.5), vetro);
    tela.linea((97.5, cy + 1.0), (102.5, cy + 1.0), 2.0, INCHIOSTRO);
    tela.linea((76.5, cy), (67.0, cy - 2.0), 2.0, INCHIOSTRO);
    tela.linea((123.5, cy), (133.0, cy - 2.0), 2.0, INCHIOSTRO);

    // sopracciglia: da tristi (punte interne in su) a rilassate
    let triste = (-umore).max(0.0);
    let alto = cy - 14.0;
    tela.linea(
        (79.0, alto + 1.0),
        (95.0, alto - 1.0 - triste * 6.0 + umore.max(0.0) * 2.0),
        2.6,
        CAPELLI,
    );
    tela.linea(
        (121.0, alto + 1.0),
        (105.0, alto - 1.0 - triste * 6.0 + umore.max(0.0) * 2.0),
        2.6,
        CAPELLI,
    );

    // bocca
    let by = cy + 21.0 - umore.min(0.0) * 3.0;
    if umore > 0.65 {
        // sorriso aperto
        painter.add(CubicBezierShape::from_points_stroke(
            [
                tela.p(86.0, by - 2.0),
                tela.p(90.0, by + 14.0 * umore),
                tela.p(110.0, by + 14.0 * umore),
                tela.p(114.0, by - 2.0),
            ],
            true,
            Color32::from_rgb(150, 40, 50),
            Stroke::new(tela.s(2.0), INCHIOSTRO),
        ));
    } else {
        painter.add(QuadraticBezierShape::from_points_stroke(
            [
                tela.p(88.0, by),
                tela.p(100.0, by + 16.0 * umore),
                tela.p(112.0, by),
            ],
            false,
            Color32::TRANSPARENT,
            Stroke::new(tela.s(2.6), INCHIOSTRO),
        ));
    }

    // goccia di sudore quando è nel panico
    if sudore {
        let fase = (t * 0.8).fract();
        let gy = cy - 18.0 + fase * 22.0;
        let alpha = ((1.0 - fase) * 255.0) as u8;
        let c = Color32::from_rgba_unmultiplied(GOCCIA.r(), GOCCIA.g(), GOCCIA.b(), alpha);
        painter.add(Shape::convex_polygon(
            vec![
                tela.p(131.0, gy - 9.0),
                tela.p(135.0, gy),
                tela.p(127.0, gy),
            ],
            c,
            Stroke::NONE,
        ));
        tela.cerchio(131.0, gy, 4.2, c);
    }
}

fn bandiera(tela: &Tela, mano: (f32, f32), b: &Bandiera, t: f32) {
    let alzata = b.alzata.clamp(0.0, 1.0);
    let asta_x = mano.0 + 1.0;
    let lunghezza = 82.0 * alzata;
    let base = mano.1 - 4.0;
    let cima = base - lunghezza;

    // asta
    tela.linea(
        (asta_x, base + 8.0),
        (asta_x, cima),
        3.0,
        Color32::from_rgb(120, 85, 50),
    );
    tela.cerchio(asta_x, cima, 3.2, GIALLO);

    if alzata < 0.4 {
        return;
    }

    // telo sventolante: una striscia di quadrilateri con un'onda che scorre
    let larghezza = 98.0;
    let altezza = 34.0;
    let y0 = cima + 3.0;
    let passi = 24;
    let mut mesh = Mesh::default();
    for i in 0..=passi {
        let d = i as f32 / passi as f32;
        let x = asta_x - d * larghezza * alzata.min(1.0);
        let onda = (d * 7.0 - t * 7.0).sin() * 5.0 * d;
        let luce = 0.82 + 0.18 * (d * 7.0 - t * 7.0).cos();
        let c = Color32::from_rgb(
            (b.colore.r() as f32 * luce) as u8,
            (b.colore.g() as f32 * luce) as u8,
            (b.colore.b() as f32 * luce) as u8,
        );
        mesh.colored_vertex(tela.p(x, y0 + onda), c);
        mesh.colored_vertex(tela.p(x, y0 + altezza + onda * 0.8), c);
        if i > 0 {
            let k = (i * 2) as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    tela.painter.add(Shape::mesh(mesh));

    // scritta
    let caratteri = b.testo.chars().count().max(1) as f32;
    let dimensione = (larghezza * 0.85 / (caratteri * 0.62)).min(17.0);
    let centro = (
        asta_x - larghezza * 0.5,
        y0 + altezza * 0.5 + ((0.5 * 7.0 - t * 7.0).sin() * 2.5),
    );
    let colore_testo = if b.colore == ROSSO {
        Color32::WHITE
    } else {
        INCHIOSTRO
    };
    // Il font di default è sottile: lo ripasso con un piccolo scarto per farlo "grassetto".
    for dx in [-0.35, 0.35] {
        tela.painter.text(
            tela.p(centro.0 + dx, centro.1),
            Align2::CENTER_CENTER,
            &b.testo,
            FontId::proportional(tela.s(dimensione)),
            colore_testo,
        );
    }
}
